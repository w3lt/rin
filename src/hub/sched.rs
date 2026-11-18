//! Session slots per agent (`maxSessions`), the first-come-first-served queue
//! in front of them, and deadlock detection over who is waiting on whom.

use std::collections::{BTreeSet, HashMap, HashSet, VecDeque};
use std::fmt;
use std::time::{Duration, Instant};

use tokio::sync::oneshot;

pub type MsgId = u64;

#[derive(Default)]
pub struct Sched {
    agents: HashMap<String, Slots>,
    inflight: HashMap<MsgId, Inflight>,
    /// Cycles already reported, so each is logged once.
    reported: HashSet<Vec<String>>,
}

struct Slots {
    max: usize,
    running: usize,
    queue: VecDeque<(MsgId, oneshot::Sender<()>)>,
}

struct Inflight {
    from: String,
    to: String,
    state: State,
}

#[derive(Clone, Copy)]
enum State {
    Queued(Instant),
    Running,
}

pub enum Admit {
    /// A session slot is held.
    Running,
    /// Queued; resolves once a slot is handed over.
    Queued(oneshot::Receiver<()>),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Usage {
    pub running: usize,
    pub max: usize,
    pub queued: usize,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Deadlock {
    /// Sorted agent ids.
    pub agents: Vec<String>,
    pub usage: Vec<Usage>,
    /// Messages between these agents still waiting in a queue.
    pub queued: usize,
    /// How long the oldest of them has been queued.
    pub since: Duration,
}

impl Sched {
    pub fn new<'a>(agents: impl IntoIterator<Item = (&'a str, usize)>) -> Sched {
        let agents = agents
            .into_iter()
            .map(|(id, max)| {
                (
                    id.to_string(),
                    Slots {
                        max: max.max(1),
                        running: 0,
                        queue: VecDeque::new(),
                    },
                )
            })
            .collect();
        Sched {
            agents,
            ..Sched::default()
        }
    }

    /// Takes a slot on `to` for message `id`, or queues it.
    pub fn admit(&mut self, id: MsgId, from: &str, to: &str, now: Instant) -> Admit {
        let slots = self.agents.get_mut(to).expect("receiver is a known agent");
        let (state, admit) = if slots.running < slots.max {
            slots.running += 1;
            (State::Running, Admit::Running)
        } else {
            let (tx, rx) = oneshot::channel();
            slots.queue.push_back((id, tx));
            (State::Queued(now), Admit::Queued(rx))
        };
        self.inflight.insert(
            id,
            Inflight {
                from: from.to_string(),
                to: to.to_string(),
                state,
            },
        );
        admit
    }

    /// Message `id` is done (answered, failed, timed out or abandoned): frees
    /// its slot for the next queued message, or takes it out of the queue.
    pub fn finish(&mut self, id: MsgId) {
        let Some(msg) = self.inflight.remove(&id) else {
            return;
        };
        let slots = self
            .agents
            .get_mut(&msg.to)
            .expect("receiver is a known agent");
        match msg.state {
            State::Queued(_) => slots.queue.retain(|(queued, _)| *queued != id),
            State::Running => loop {
                let Some((next, tx)) = slots.queue.pop_front() else {
                    slots.running -= 1;
                    break;
                };
                if tx.send(()).is_ok() {
                    if let Some(next) = self.inflight.get_mut(&next) {
                        next.state = State::Running;
                    }
                    break;
                }
            },
        }
    }

    pub fn usage(&self, agent: &str) -> Usage {
        self.agents.get(agent).map_or(
            Usage {
                running: 0,
                max: 0,
                queued: 0,
            },
            |s| Usage {
                running: s.running,
                max: s.max,
                queued: s.queue.len(),
            },
        )
    }

    /// Suspected deadlocks: cycles in the wait graph (A → B while A has a
    /// message to B in flight) where every agent is at its `maxSessions` and
    /// at least one message in the cycle is queued.
    pub fn deadlocks(&self, now: Instant) -> Vec<Deadlock> {
        let full = |id: &str| self.agents.get(id).is_some_and(|s| s.running >= s.max);
        let mut edges: HashMap<&str, BTreeSet<&str>> = HashMap::new();
        for msg in self.inflight.values() {
            if full(&msg.from) && full(&msg.to) {
                edges
                    .entry(msg.from.as_str())
                    .or_default()
                    .insert(msg.to.as_str());
            }
        }
        let reach = |start: &str| -> HashSet<String> {
            let mut seen = HashSet::new();
            let mut stack = vec![start];
            while let Some(node) = stack.pop() {
                for next in edges.get(node).into_iter().flatten() {
                    if seen.insert(next.to_string()) {
                        stack.push(next);
                    }
                }
            }
            seen
        };
        let reaches: HashMap<&str, HashSet<String>> =
            edges.keys().map(|&n| (n, reach(n))).collect();

        let mut components: BTreeSet<Vec<String>> = BTreeSet::new();
        for (&node, reached) in &reaches {
            let mut component: Vec<String> = reached
                .iter()
                .filter(|other| {
                    reaches
                        .get(other.as_str())
                        .is_some_and(|back| back.contains(node))
                })
                .cloned()
                .collect();
            if !component.iter().any(|c| c == node) {
                continue; // not on a cycle
            }
            component.sort();
            if component.len() >= 2 {
                components.insert(component);
            }
        }

        components
            .into_iter()
            .filter_map(|agents| {
                let inside = |id: &String| agents.contains(id);
                let queued_since: Vec<Instant> = self
                    .inflight
                    .values()
                    .filter(|m| inside(&m.from) && inside(&m.to))
                    .filter_map(|m| match m.state {
                        State::Queued(at) => Some(at),
                        State::Running => None,
                    })
                    .collect();
                let oldest = queued_since.iter().min()?;
                Some(Deadlock {
                    usage: agents.iter().map(|a| self.usage(a)).collect(),
                    queued: queued_since.len(),
                    since: now.saturating_duration_since(*oldest),
                    agents,
                })
            })
            .collect()
    }

    /// Re-checks for deadlocks; returns the ones not reported before.
    /// Cycles that broke are forgotten, so they're reported again if they come back.
    pub fn new_deadlocks(&mut self, now: Instant) -> Vec<Deadlock> {
        let current = self.deadlocks(now);
        let fresh = current
            .iter()
            .filter(|d| !self.reported.contains(&d.agents))
            .cloned()
            .collect();
        self.reported = current.into_iter().map(|d| d.agents).collect();
        fresh
    }
}

impl fmt::Display for Deadlock {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let names = if self.agents.len() == 2 {
            format!("{} ⇄ {}", self.agents[0], self.agents[1])
        } else {
            self.agents.join(", ")
        };
        let first = self.usage[0];
        let usage = if self
            .usage
            .iter()
            .all(|u| u.running == first.running && u.max == first.max)
        {
            let all = if self.agents.len() == 2 {
                "both"
            } else {
                "all"
            };
            format!("{all} {}/{}", first.running, first.max)
        } else {
            self.agents
                .iter()
                .zip(&self.usage)
                .map(|(a, u)| format!("{a} {}/{}", u.running, u.max))
                .collect::<Vec<_>>()
                .join(", ")
        };
        write!(
            f,
            "⚠ possible deadlock: {names} ({usage}, {} queued for {})",
            self.queued,
            crate::config::format_duration(self.since)
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sched() -> Sched {
        Sched::new([("rin", 3), ("backend", 1), ("frontend", 1)])
    }

    #[test]
    fn queues_first_come_first_served() {
        let mut s = sched();
        let now = Instant::now();
        assert!(matches!(
            s.admit(1, "user", "frontend", now),
            Admit::Running
        ));
        let Admit::Queued(mut second) = s.admit(2, "user", "frontend", now) else {
            panic!()
        };
        let Admit::Queued(mut third) = s.admit(3, "user", "frontend", now) else {
            panic!()
        };
        assert_eq!(
            s.usage("frontend"),
            Usage {
                running: 1,
                max: 1,
                queued: 2
            }
        );

        s.finish(1);
        assert!(second.try_recv().is_ok());
        assert!(third.try_recv().is_err());
        assert_eq!(
            s.usage("frontend"),
            Usage {
                running: 1,
                max: 1,
                queued: 1
            }
        );

        s.finish(2);
        assert!(third.try_recv().is_ok());
        s.finish(3);
        assert_eq!(
            s.usage("frontend"),
            Usage {
                running: 0,
                max: 1,
                queued: 0
            }
        );
    }

    #[test]
    fn abandoned_messages_free_their_place() {
        let mut s = sched();
        let now = Instant::now();
        s.admit(1, "user", "frontend", now);
        let Admit::Queued(gone) = s.admit(2, "user", "frontend", now) else {
            panic!()
        };
        let Admit::Queued(mut waiting) = s.admit(3, "user", "frontend", now) else {
            panic!()
        };

        // 2 times out while queued: it leaves the queue without ever running.
        s.finish(2);
        drop(gone);
        assert_eq!(s.usage("frontend").queued, 1);
        s.finish(1);
        assert!(waiting.try_recv().is_ok());

        // A waiter that gave up after the slot was handed over still gives it back.
        let Admit::Queued(_late) = s.admit(4, "user", "frontend", now) else {
            panic!()
        };
        s.finish(3);
        s.finish(4);
        assert_eq!(
            s.usage("frontend"),
            Usage {
                running: 0,
                max: 1,
                queued: 0
            }
        );
    }

    #[test]
    fn dropped_receivers_are_skipped() {
        let mut s = sched();
        let now = Instant::now();
        s.admit(1, "user", "frontend", now);
        let Admit::Queued(dropped) = s.admit(2, "user", "frontend", now) else {
            panic!()
        };
        drop(dropped);
        s.finish(1);
        assert_eq!(
            s.usage("frontend"),
            Usage {
                running: 0,
                max: 1,
                queued: 0
            }
        );
    }

    #[test]
    fn spots_a_two_agent_deadlock_once() {
        let mut s = sched();
        let t0 = Instant::now();
        s.admit(1, "user", "backend", t0);
        s.admit(2, "user", "frontend", t0);
        assert!(s.new_deadlocks(t0).is_empty());
        s.admit(3, "backend", "frontend", t0);
        assert!(s.new_deadlocks(t0).is_empty(), "one direction only");
        s.admit(4, "frontend", "backend", t0);

        let found = s.new_deadlocks(t0 + Duration::from_secs(5));
        assert_eq!(found.len(), 1);
        assert_eq!(
            found[0].to_string(),
            "⚠ possible deadlock: backend ⇄ frontend (both 1/1, 2 queued for 5s)"
        );
        assert!(s.new_deadlocks(t0).is_empty(), "reported once");
        assert_eq!(
            s.deadlocks(t0 + Duration::from_secs(180))[0].to_string(),
            "⚠ possible deadlock: backend ⇄ frontend (both 1/1, 2 queued for 3m)"
        );

        // The timeout breaks it.
        s.finish(3);
        assert!(s.deadlocks(t0).is_empty());
        assert!(s.new_deadlocks(t0).is_empty());
    }

    #[test]
    fn no_deadlock_without_a_queue_or_free_slots() {
        let mut s = sched();
        let t0 = Instant::now();
        // rin has free slots, so rin ⇄ frontend can make progress.
        s.admit(1, "user", "frontend", t0);
        s.admit(2, "frontend", "rin", t0);
        s.admit(3, "rin", "frontend", t0);
        assert!(s.deadlocks(t0).is_empty());
    }

    #[test]
    fn three_agent_cycle() {
        let mut s = Sched::new([("a", 1), ("b", 1), ("c", 2)]);
        let t0 = Instant::now();
        s.admit(1, "user", "a", t0);
        s.admit(2, "a", "b", t0);
        s.admit(3, "b", "c", t0);
        s.admit(4, "user", "c", t0);
        s.admit(5, "c", "a", t0);
        let found = s.deadlocks(t0);
        assert_eq!(found.len(), 1);
        assert_eq!(
            found[0].to_string(),
            "⚠ possible deadlock: a, b, c (a 1/1, b 1/1, c 2/2, 1 queued for 0s)"
        );
    }
}
