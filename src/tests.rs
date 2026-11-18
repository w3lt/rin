//! End to end on one machine: a real hub and runtimes over HTTP, with a fake
//! `claude` (a shell script) as every agent's harness. No containers, no models.

use std::collections::HashMap;
use std::net::SocketAddr;
use std::path::PathBuf;
use std::sync::Arc;
use std::time::{Duration, Instant};

use serde_json::json;

use crate::client::Client;
use crate::config::Harness;
use crate::hub::auth::{Principal, Resolver};
use crate::hub::db::Db;
use crate::hub::{Hub, HubError};
use crate::proto::{Kind, SendRequest};
use crate::agent_runtime::Runtime;

/// Answers `<first prompt line> | hops=<RIN_HOPS> | <message>`; `sleep N …` sleeps N seconds
/// (in a child process, whose pid it records), `fail` exits 3, `bg` leaves a
/// `sleep 30` running and answers at once.
const FAKE_CLAUDE: &str = r#"#!/bin/sh
prompt="$2"
first=$(printf '%s\n' "$prompt" | head -n 1)
text=$(printf '%s\n' "$prompt" | tail -n 1)
echo "start $text" >> "$(dirname "$0")/starts"
case "$text" in
  sleep*) set -- $text; sleep "$2" & echo $! > "$(dirname "$0")/sleep.pid"; wait ;;
  fail) echo "working"; echo "boom: no such model" >&2; exit 3 ;;
  bg) sleep 30 & echo $! > "$(dirname "$0")/sleep.pid" ;;
esac
printf '{"type":"result","is_error":false,"result":"%s | hops=%s | %s"}\n' "$first" "$RIN_HOPS" "$text"
"#;

struct Team {
    hub: Arc<Hub>,
    hub_url: String,
    user_token: String,
    dir: PathBuf,
}

async fn serve(router: axum::Router) -> SocketAddr {
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    tokio::spawn(async move {
        axum::serve(
            listener,
            router.into_make_service_with_connect_info::<SocketAddr>(),
        )
        .await
        .unwrap();
    });
    addr
}

/// Agents `a` (maxSessions 1) and `b` (3); `down` has no runtime; `stale`'s
/// runtime holds an old token.
async fn team(name: &str, timeout: Duration) -> Team {
    let dir = std::env::temp_dir().join(format!("rin-e2e-{name}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    let fake = dir.join("claude");
    std::fs::write(&fake, FAKE_CLAUDE).unwrap();
    std::fs::set_permissions(&fake, std::os::unix::fs::PermissionsExt::from_mode(0o755)).unwrap();

    let agent = |id: &str, max: u32| {
        json!({ "id": id, "type": if id == "a" { "leader" } else { "member" }, "label": format!("Agent {id}"),
                "harness": "claude-code", "authentication": "subscription", "model": "fake-model",
                "maxSessions": max, "resources": { "cpus": 1, "ram": "1g" } })
    };
    let config = crate::config::from_value(json!({
        "rin": "tohsaka", "name": "e2e", "version": crate::config::VERSION,
        "messaging": { "timeout": "1m", "maxHops": 3 },
        "agents": [agent("a", 1), agent("b", 3), agent("down", 1), agent("stale", 1)],
    }))
    .unwrap();

    let mut tokens = HashMap::new();
    let mut run_urls = HashMap::new();
    for id in ["user", "a", "b", "down", "stale"] {
        tokens.insert(id.to_string(), format!("token-{id}-{name}"));
    }
    for id in ["a", "b", "stale"] {
        let home = dir.join(id);
        std::fs::create_dir_all(&home).unwrap();
        let token = if id == "stale" {
            "token-from-before"
        } else {
            &tokens[id]
        };
        let mut runtime = Runtime::new(
            Harness::ClaudeCode,
            "fake-model",
            Duration::from_secs(60),
            token,
            "http://127.0.0.1:7000",
            &home,
        );
        runtime.program = Some(fake.clone());
        let addr = serve(crate::agent_runtime::router(Arc::new(runtime))).await;
        run_urls.insert(id.to_string(), format!("http://{addr}/run"));
    }
    // Nothing listens there: `down`'s container is stopped.
    let closed = std::net::TcpListener::bind("127.0.0.1:0")
        .unwrap()
        .local_addr()
        .unwrap();
    run_urls.insert("down".to_string(), format!("http://{closed}/run"));

    // Agents' "container IPs" are not 127.0.0.1, so test requests over HTTP come from "your machine".
    let ips = [
        ("a", "10.9.0.2"),
        ("b", "10.9.0.3"),
        ("down", "10.9.0.4"),
        ("stale", "10.9.0.5"),
    ]
    .into_iter()
    .map(|(id, ip)| (id.to_string(), ip.parse().unwrap()))
    .collect();
    let mut hub = Hub::new(
        config,
        tokens.clone(),
        Resolver::Static { ips, run_urls },
        Db::memory(),
    );
    hub.timeout = timeout;
    let hub = Arc::new(hub);
    let addr = serve(crate::hub::router(hub.clone())).await;
    Team {
        hub,
        hub_url: format!("http://{addr}"),
        user_token: tokens["user"].clone(),
        dir,
    }
}

impl Team {
    fn user(&self) -> Client {
        Client::new(&self.hub_url, &self.user_token)
    }

    async fn send_as(
        &self,
        from: &str,
        to: &str,
        text: &str,
        hops: u32,
    ) -> Result<String, HubError> {
        let principal = if from == "user" {
            Principal::User
        } else {
            Principal::Agent(from.into())
        };
        self.hub
            .send(
                &principal,
                SendRequest {
                    to: to.into(),
                    text: text.into(),
                    hops,
                },
            )
            .await
    }

    /// Waits for the process the fake harness recorded in `sleep.pid` to be gone.
    async fn sleep_killed(&self) {
        let pid: i32 = std::fs::read_to_string(self.dir.join("sleep.pid"))
            .unwrap()
            .trim()
            .parse()
            .unwrap();
        let gone = async {
            while nix::sys::signal::kill(nix::unistd::Pid::from_raw(pid), None).is_ok() {
                tokio::time::sleep(Duration::from_millis(50)).await;
            }
        };
        tokio::time::timeout(Duration::from_secs(3), gone)
            .await
            .expect("the session's process was killed");
    }

    /// Sessions the fake harness started, in order (all agents share it).
    fn starts(&self) -> Vec<String> {
        std::fs::read_to_string(self.dir.join("starts"))
            .unwrap_or_default()
            .lines()
            .map(str::to_string)
            .collect()
    }
}

impl Drop for Team {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.dir);
    }
}

#[tokio::test(flavor = "multi_thread")]
async fn you_message_an_agent_and_get_its_answer() {
    let t = team("pong", Duration::from_secs(10)).await;
    let reply = t
        .user()
        .send("a", "Reply with exactly: pong", 1)
        .await
        .unwrap();
    assert_eq!(
        reply,
        "[Rin] Message from user (User). | hops=1 | Reply with exactly: pong"
    );

    let log = t.user().log(None, 0).await.unwrap();
    assert_eq!(log.len(), 2);
    assert_eq!(
        (
            log[0].kind,
            log[0].from.as_deref(),
            log[0].to.as_deref(),
            log[0].hops
        ),
        (Kind::Message, Some("user"), Some("a"), Some(1))
    );
    assert_eq!(
        (log[1].kind, log[1].from.as_deref(), log[1].to.as_deref()),
        (Kind::Reply, Some("a"), Some("user"))
    );
    assert_eq!(log[1].msg, Some(log[0].id));
}

#[tokio::test(flavor = "multi_thread")]
async fn agents_send_with_their_true_identity_and_hops() {
    let t = team("hops", Duration::from_secs(10)).await;
    let reply = t.send_as("a", "b", "hello", 2).await.unwrap();
    assert_eq!(reply, "[Rin] Message from a (Agent a). | hops=2 | hello");

    // maxHops is 3: a fourth hop is refused, and the refusal is logged.
    let err = t.send_as("b", "a", "again", 4).await.unwrap_err();
    assert!(err.to_string().starts_with("hop limit reached"), "{err}");
    let log = t.user().log(Some("b"), 0).await.unwrap();
    let last = log.last().unwrap();
    assert_eq!((last.kind, last.to.as_deref()), (Kind::Error, Some("b")));
    assert!(last.text.starts_with("hop limit reached"));
}

#[tokio::test(flavor = "multi_thread")]
async fn rejects_self_unknown_and_unreachable_receivers() {
    let t = team("reject", Duration::from_secs(10)).await;
    assert!(matches!(
        t.send_as("b", "b", "hi", 1).await,
        Err(HubError::ToSelf)
    ));
    assert!(
        t.send_as("b", "nobody", "hi", 1)
            .await
            .unwrap_err()
            .to_string()
            .starts_with("unknown agent")
    );
    let err = t.user().send("down", "hi", 1).await.unwrap_err();
    assert!(err.to_string().starts_with("receiver down"), "{err}");
    let err = t.user().send("stale", "hi", 1).await.unwrap_err();
    assert!(
        err.to_string()
            .starts_with("session failed: stale refused the hub (not authorized: wrong token)"),
        "{err}"
    );
    assert_eq!(
        t.hub.agents(&Principal::User).agents[2].sessions,
        0,
        "the failed message gave its slot back"
    );
}

#[tokio::test(flavor = "multi_thread")]
async fn failed_sessions_report_their_last_lines() {
    let t = team("fail", Duration::from_secs(10)).await;
    let err = t.user().send("a", "fail", 1).await.unwrap_err().to_string();
    assert!(err.starts_with("session failed"), "{err}");
    assert!(
        err.contains("boom: no such model") && err.contains("working"),
        "{err}"
    );
}

#[tokio::test(flavor = "multi_thread")]
async fn timeout_kills_the_session() {
    let t = team("timeout", Duration::from_secs(1)).await;
    let started = Instant::now();
    let err = t
        .user()
        .send("b", "sleep 30", 1)
        .await
        .unwrap_err()
        .to_string();
    assert!(err.starts_with("timeout"), "{err}");
    assert!(
        started.elapsed() < Duration::from_secs(5),
        "took {:?}",
        started.elapsed()
    );

    // The session's child process is gone, and the slot is free again.
    t.sleep_killed().await;
    assert_eq!(t.hub.agents(&Principal::User).agents[1].sessions, 0);
}

#[tokio::test(flavor = "multi_thread")]
async fn a_sender_that_stops_waiting_ends_the_session() {
    // The chain behind nested messages: a killed sender drops its request,
    // the hub drops the runtime's, and the runtime kills the session.
    let t = Arc::new(team("cancel", Duration::from_secs(30)).await);
    let send = {
        let t = t.clone();
        tokio::spawn(async move { t.user().send("b", "sleep 30", 1).await })
    };
    while !t.dir.join("sleep.pid").exists() {
        tokio::time::sleep(Duration::from_millis(20)).await;
    }
    assert_eq!(t.hub.agents(&Principal::User).agents[1].sessions, 1);
    send.abort();
    t.sleep_killed().await;
    let freed = async {
        while t.hub.agents(&Principal::User).agents[1].sessions != 0 {
            tokio::time::sleep(Duration::from_millis(20)).await;
        }
    };
    tokio::time::timeout(Duration::from_secs(3), freed)
        .await
        .expect("the slot was freed");
    let log = t.user().log(None, 0).await.unwrap();
    assert!(
        log.last().unwrap().text.starts_with("cancelled"),
        "{:?}",
        log.last()
    );
}

#[tokio::test(flavor = "multi_thread")]
async fn leftovers_of_a_finished_session_are_stopped() {
    let t = team("leftovers", Duration::from_secs(10)).await;
    let started = Instant::now();
    let reply = t.user().send("b", "bg", 1).await.unwrap();
    assert!(reply.ends_with("| bg"), "{reply}");
    assert!(
        started.elapsed() < Duration::from_secs(5),
        "a background process held up the reply"
    );
    t.sleep_killed().await;
}

#[tokio::test(flavor = "multi_thread")]
async fn full_agents_queue_first_come_first_served() {
    let t = Arc::new(team("queue", Duration::from_secs(20)).await);
    let mut sends = Vec::new();
    for n in 1..=3 {
        let t = t.clone();
        sends.push(tokio::spawn(async move {
            t.user().send("a", &format!("sleep 0.4 n{n}"), 1).await
        }));
        tokio::time::sleep(Duration::from_millis(100)).await;
    }
    let usage = &t.hub.agents(&Principal::User).agents[0];
    assert_eq!(
        (usage.sessions, usage.max_sessions, usage.queued),
        (1, 1, 2)
    );
    for (n, send) in sends.into_iter().enumerate() {
        let reply = send.await.unwrap().unwrap();
        assert!(reply.ends_with(&format!("sleep 0.4 n{}", n + 1)), "{reply}");
    }
    assert_eq!(
        t.starts(),
        [
            "start sleep 0.4 n1",
            "start sleep 0.4 n2",
            "start sleep 0.4 n3"
        ]
    );
}

#[tokio::test(flavor = "multi_thread")]
async fn queued_messages_time_out_without_starting() {
    // E6: message 1 hangs; message 2, sent right after, waits behind it and
    // fails with it, never starting a session.
    let t = Arc::new(team("queued-timeout", Duration::from_secs(1)).await);
    let first = {
        let t = t.clone();
        tokio::spawn(async move { t.user().send("a", "sleep 30", 1).await })
    };
    tokio::time::sleep(Duration::from_millis(30)).await;
    let started = Instant::now();
    let second = t
        .user()
        .send("a", "never runs", 1)
        .await
        .unwrap_err()
        .to_string();
    assert!(
        second.starts_with("timeout: a was busy") && second.ends_with("not started"),
        "{second}"
    );
    assert!(
        started.elapsed() < Duration::from_millis(1500),
        "{:?} after its own send",
        started.elapsed()
    );
    assert!(
        first
            .await
            .unwrap()
            .unwrap_err()
            .to_string()
            .starts_with("timeout")
    );
    assert_eq!(t.starts(), ["start sleep 30"]);
    let usage = &t.hub.agents(&Principal::User).agents[0];
    assert_eq!((usage.sessions, usage.queued), (0, 0));
}

#[tokio::test(flavor = "multi_thread")]
async fn queued_messages_with_time_left_run_when_a_slot_frees() {
    let t = Arc::new(team("queued-runs", Duration::from_secs(1)).await);
    let first = {
        let t = t.clone();
        tokio::spawn(async move { t.user().send("a", "sleep 30", 1).await })
    };
    tokio::time::sleep(Duration::from_millis(400)).await;
    let second = t.user().send("a", "runs late", 1).await.unwrap();
    assert!(second.ends_with("runs late"), "{second}");
    assert!(first.await.unwrap().is_err());
}

#[tokio::test(flavor = "multi_thread")]
async fn too_long_messages_are_refused_before_sending() {
    let t = team("too-big", Duration::from_secs(10)).await;
    let err = t
        .user()
        .send("b", &"x".repeat(3 << 20), 1)
        .await
        .unwrap_err()
        .to_string();
    assert!(
        err.starts_with("message too long: 3073 KiB as JSON, the hub takes at most 120 KiB"),
        "{err}"
    );
    assert!(
        t.user().log(None, 0).await.unwrap().is_empty(),
        "nothing reached the hub"
    );
    // Just under the limit gets all the way through the harness's command line.
    let reply = t
        .user()
        .send("b", &"x".repeat(crate::proto::MAX_BODY - 100), 1)
        .await
        .unwrap();
    assert!(reply.contains("| xxx"), "{}", &reply[..80]);
}

#[tokio::test(flavor = "multi_thread")]
async fn plain_text_errors_from_the_hub_are_shown() {
    // Errors raised before the hub's handlers (here: a bad query string) aren't JSON.
    let t = team("plain", Duration::from_secs(10)).await;
    let response = reqwest::Client::new()
        .get(format!("{}/log?after=abc", t.hub_url))
        .bearer_auth(&t.user_token)
        .send()
        .await;
    let err = t
        .user()
        .read::<serde_json::Value>(response)
        .await
        .unwrap_err()
        .to_string();
    assert!(
        err.starts_with("the hub answered 400 Bad Request: "),
        "{err}"
    );
}

#[tokio::test(flavor = "multi_thread")]
async fn tokens_are_checked() {
    let t = team("auth", Duration::from_secs(10)).await;
    // No token, garbage, and an agent's token from outside its container.
    for token in ["", "garbage", "token-a-auth"] {
        let err = Client::new(&t.hub_url, token)
            .send("b", "hi", 1)
            .await
            .unwrap_err()
            .to_string();
        assert_eq!(err, "not authorized");
        assert_eq!(
            Client::new(&t.hub_url, token)
                .agents()
                .await
                .unwrap_err()
                .to_string(),
            "not authorized"
        );
        assert_eq!(
            Client::new(&t.hub_url, token)
                .log(None, 0)
                .await
                .unwrap_err()
                .to_string(),
            "not authorized"
        );
    }
    // Unauthorized attempts are not logged.
    assert!(t.user().log(None, 0).await.unwrap().is_empty());
}

#[tokio::test(flavor = "multi_thread")]
async fn runtime_rejects_anyone_but_the_hub() {
    let dir = std::env::temp_dir().join(format!("rin-e2e-runtime-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    // The "hub" is some other host, so even the right token from here is refused.
    let runtime = Runtime::new(
        Harness::ClaudeCode,
        "m",
        Duration::from_secs(60),
        "tok",
        "http://192.0.2.1:7000",
        &dir,
    );
    let addr = serve(crate::agent_runtime::router(Arc::new(runtime))).await;
    let http = reqwest::Client::new();
    let body = json!({ "from": "a", "fromLabel": "A", "text": "hi", "hops": 1, "timeoutMs": 1000 });
    for (token, why) in [("tok", "not from the hub"), ("wrong", "wrong token")] {
        let response = http
            .post(format!("http://{addr}/run"))
            .bearer_auth(token)
            .json(&body)
            .send()
            .await
            .unwrap();
        assert_eq!(response.status(), 401);
        let body: serde_json::Value = response.json().await.unwrap();
        assert_eq!(body["error"], format!("not authorized: {why}"));
    }
    let _ = std::fs::remove_dir_all(&dir);
}

#[tokio::test(flavor = "multi_thread")]
async fn list_agents_marks_the_caller() {
    let t = team("list", Duration::from_secs(10)).await;
    let listed = t.hub.agents(&Principal::Agent("b".into()));
    let you: Vec<_> = listed
        .agents
        .iter()
        .filter(|a| a.you)
        .map(|a| a.id.as_str())
        .collect();
    assert_eq!(you, ["b"]);
    let text = crate::display::agents(&listed);
    assert!(
        text.contains("Agent b") && text.lines().nth(2).unwrap().ends_with("you"),
        "{text}"
    );
}
