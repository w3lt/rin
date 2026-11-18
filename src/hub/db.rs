//! The hub's message log, in SQLite on the hub's volume.
//!
//! Every message, reply, error and deadlock warning is one event row.

use std::path::Path;
use std::sync::Mutex;
use std::time::{SystemTime, UNIX_EPOCH};

use anyhow::{Context, Result};
use rusqlite::{Connection, params};

use crate::proto::{Event, Kind};

fn kind_str(kind: Kind) -> &'static str {
    match kind {
        Kind::Message => "message",
        Kind::Reply => "reply",
        Kind::Error => "error",
        Kind::Warning => "warning",
    }
}

fn parse_kind(s: &str) -> Kind {
    match s {
        "message" => Kind::Message,
        "reply" => Kind::Reply,
        "error" => Kind::Error,
        _ => Kind::Warning,
    }
}

struct Row<'a> {
    kind: Kind,
    msg: Option<i64>,
    from: Option<&'a str>,
    to: Option<&'a str>,
    hops: Option<u32>,
    text: &'a str,
    agents: Option<&'a str>,
}

impl Default for Row<'_> {
    fn default() -> Self {
        Row {
            kind: Kind::Message,
            msg: None,
            from: None,
            to: None,
            hops: None,
            text: "",
            agents: None,
        }
    }
}

pub struct Db {
    conn: Mutex<Connection>,
}

pub fn now_ms() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |d| d.as_millis() as i64)
}

impl Db {
    pub fn open(path: &Path) -> Result<Db> {
        let conn =
            Connection::open(path).with_context(|| format!("cannot open {}", path.display()))?;
        Db::init(conn)
    }

    #[cfg(test)]
    pub fn memory() -> Db {
        Db::init(Connection::open_in_memory().unwrap()).unwrap()
    }

    fn init(conn: Connection) -> Result<Db> {
        conn.execute_batch(
            "PRAGMA journal_mode = WAL;
             CREATE TABLE IF NOT EXISTS events (
               id INTEGER PRIMARY KEY AUTOINCREMENT,
               ts INTEGER NOT NULL,
               kind TEXT NOT NULL,
               msg INTEGER,
               sender TEXT,
               receiver TEXT,
               hops INTEGER,
               text TEXT NOT NULL,
               agents TEXT
             );",
        )?;
        Ok(Db {
            conn: Mutex::new(conn),
        })
    }

    fn insert(&self, row: Row) -> i64 {
        let conn = self.conn.lock().unwrap_or_else(|e| e.into_inner());
        let result = conn.execute(
            "INSERT INTO events (ts, kind, msg, sender, receiver, hops, text, agents) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)",
            params![now_ms(), kind_str(row.kind), row.msg, row.from, row.to, row.hops, row.text, row.agents],
        );
        match result {
            Ok(_) => conn.last_insert_rowid(),
            Err(e) => {
                // The log must never take messaging down with it.
                eprintln!("rin hub: cannot write log: {e}");
                0
            }
        }
    }

    /// Logs a new message and returns its id.
    pub fn message(&self, from: &str, to: &str, hops: u32, text: &str) -> i64 {
        self.insert(Row {
            kind: Kind::Message,
            from: Some(from),
            to: Some(to),
            hops: Some(hops),
            text,
            ..Row::default()
        })
    }

    /// Logs the answer to message `msg`: sent by its receiver back to its sender.
    pub fn reply(&self, msg: i64, from: &str, to: &str, text: &str) {
        self.insert(Row {
            kind: Kind::Reply,
            msg: Some(msg),
            from: Some(from),
            to: Some(to),
            text,
            ..Row::default()
        });
    }

    /// Logs an error returned to the sender of message `msg`.
    pub fn error(&self, msg: i64, from: &str, to: &str, text: &str) {
        self.insert(Row {
            kind: Kind::Error,
            msg: Some(msg),
            from: Some(from),
            to: Some(to),
            text,
            ..Row::default()
        });
    }

    pub fn warning(&self, agents: &[String], text: &str) {
        let agents = agents.join(",");
        self.insert(Row {
            kind: Kind::Warning,
            text,
            agents: Some(&agents),
            ..Row::default()
        });
    }

    /// Events after `after`, optionally only those involving `agent`.
    /// Up to [`crate::proto::LOG_PAGE`] events after `after`, oldest first.
    pub fn events(&self, agent: Option<&str>, after: i64) -> Result<Vec<Event>> {
        let conn = self.conn.lock().unwrap_or_else(|e| e.into_inner());
        let mut stmt = conn.prepare(
            "SELECT id, ts, kind, msg, sender, receiver, hops, text FROM events
             WHERE id > ?1 AND (?2 IS NULL OR sender = ?2 OR receiver = ?2
                                OR (',' || agents || ',') LIKE ('%,' || ?2 || ',%'))
             ORDER BY id LIMIT ?3",
        )?;
        let rows = stmt.query_map(
            params![after, agent, crate::proto::LOG_PAGE as i64],
            |row| {
                Ok(Event {
                    id: row.get(0)?,
                    ts: row.get(1)?,
                    kind: parse_kind(&row.get::<_, String>(2)?),
                    msg: row.get(3)?,
                    from: row.get(4)?,
                    to: row.get(5)?,
                    hops: row.get(6)?,
                    text: row.get(7)?,
                })
            },
        )?;
        Ok(rows.collect::<rusqlite::Result<_>>()?)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn pages_the_log() {
        let db = Db::memory();
        let page = crate::proto::LOG_PAGE as i64;
        for _ in 0..page + 1 {
            db.message("user", "a", 1, "hi");
        }
        let first = db.events(None, 0).unwrap();
        assert_eq!(first.len() as i64, page);
        let rest = db.events(None, first.last().unwrap().id).unwrap();
        assert_eq!(rest.len(), 1);
    }

    #[test]
    fn filters_by_agent_and_cursor() {
        let db = Db::memory();
        let m1 = db.message("user", "rin", 1, "hello");
        let m2 = db.message("rin", "frontend", 2, "ping");
        db.reply(m2, "frontend", "rin", "pong");
        db.reply(m1, "rin", "user", "pong");
        db.warning(&["backend".into(), "frontend".into()], "possible deadlock");

        let all = db.events(None, 0).unwrap();
        assert_eq!(all.len(), 5);
        assert_eq!(all[2].kind, Kind::Reply);
        assert_eq!(all[2].msg, Some(m2));
        assert_eq!(all[2].from.as_deref(), Some("frontend"));

        let frontend: Vec<_> = db
            .events(Some("frontend"), 0)
            .unwrap()
            .into_iter()
            .map(|e| e.text)
            .collect();
        assert_eq!(frontend, ["ping", "pong", "possible deadlock"]);
        let backend: Vec<_> = db
            .events(Some("backend"), 0)
            .unwrap()
            .into_iter()
            .map(|e| e.text)
            .collect();
        assert_eq!(backend, ["possible deadlock"]);
        // No prefix matches: "end" is not "backend".
        assert!(db.events(Some("end"), 0).unwrap().is_empty());

        let after = db.events(None, all[3].id).unwrap();
        assert_eq!(after.len(), 1);
        assert_eq!(after[0].kind, Kind::Warning);
    }
}
