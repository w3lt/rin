//! Plain-text tables and log lines for the terminal and for agents.

#![cfg_attr(
    not(all(feature = "host", feature = "hub", feature = "agent")),
    allow(dead_code)
)]

use crate::proto::{AgentsResponse, Event, Kind};

/// Left-aligned columns, two spaces apart.
pub fn table(rows: &[Vec<String>]) -> String {
    let columns = rows.iter().map(Vec::len).max().unwrap_or(0);
    let widths: Vec<usize> = (0..columns)
        .map(|c| {
            rows.iter()
                .filter_map(|r| r.get(c))
                .map(|v| v.chars().count())
                .max()
                .unwrap_or(0)
        })
        .collect();
    let mut out = String::new();
    for row in rows {
        let cells: Vec<String> = row
            .iter()
            .enumerate()
            .map(|(c, value)| format!("{value}{}", " ".repeat(widths[c] - value.chars().count())))
            .collect();
        out.push_str(cells.join("  ").trim_end());
        out.push('\n');
    }
    out
}

/// `rin agents` / `list_agents()`: the team, with the caller marked.
pub fn agents(response: &AgentsResponse) -> String {
    let mut rows = vec![
        ["ID", "LABEL", "TYPE", "HARNESS", "SESSIONS", "QUEUED", ""]
            .map(String::from)
            .to_vec(),
    ];
    for a in &response.agents {
        rows.push(vec![
            a.id.clone(),
            a.label.clone(),
            a.kind.clone(),
            a.harness.clone(),
            format!("{}/{}", a.sessions, a.max_sessions),
            a.queued.to_string(),
            if a.you { "you".into() } else { String::new() },
        ]);
    }
    let mut out = table(&rows);
    for warning in &response.warnings {
        out.push_str(warning);
        out.push('\n');
    }
    out
}

/// Prints `text` to stdout. When the reader has gone away (`rin log | head`)
/// the command ends quietly, where `print!` would panic.
pub fn out(text: &str) -> std::io::Result<()> {
    use std::io::Write;
    let mut stdout = std::io::stdout().lock();
    match stdout
        .write_all(text.as_bytes())
        .and_then(|()| stdout.flush())
    {
        Err(e) if e.kind() == std::io::ErrorKind::BrokenPipe => std::process::exit(0),
        result => result,
    }
}

/// Text an agent wrote, safe to show in your terminal: control characters
/// (escape sequences, carriage returns, line breaks in a name) are written
/// out as `\u{1b}`, so a message can't redraw the screen or fake a log line.
fn visible(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    for c in text.chars() {
        if c.is_control() && c != '\t' {
            out.extend(c.escape_default());
        } else {
            out.push(c);
        }
    }
    out
}

/// One `rin log` entry: a header line, then the text indented.
pub fn event(e: &Event) -> String {
    let when = utc(e.ts);
    // The receiver's name is whatever the sender asked for, known agent or not.
    let who = |v: &Option<String>| visible(v.as_deref().unwrap_or_default());
    let header = match e.kind {
        Kind::Message => format!(
            "#{}  {} → {}  (hops {})",
            e.id,
            who(&e.from),
            who(&e.to),
            e.hops.unwrap_or(0)
        ),
        Kind::Reply => format!(
            "#{}  {} → {}  reply",
            e.msg.unwrap_or(0),
            who(&e.from),
            who(&e.to)
        ),
        Kind::Error => format!("#{}  error → {}", e.msg.unwrap_or(0), who(&e.to)),
        Kind::Warning => return format!("{when}  {}\n", e.text),
    };
    let body: String = e
        .text
        .lines()
        .map(|l| format!("    {}\n", visible(l)))
        .collect();
    format!("{when}  {header}\n{body}")
}

/// `2026-11-17 22:31:05Z` from Unix milliseconds.
pub fn utc(ms: i64) -> String {
    let secs = ms.div_euclid(1000);
    let (days, rest) = (secs.div_euclid(86_400), secs.rem_euclid(86_400));
    // Civil date from days since 1970-01-01 (Howard Hinnant's algorithm).
    let z = days + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z.rem_euclid(146_097);
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let day = doy - (153 * mp + 2) / 5 + 1;
    let month = if mp < 10 { mp + 3 } else { mp - 9 };
    let year = yoe + era * 400 + i64::from(month <= 2);
    format!(
        "{year:04}-{month:02}-{day:02} {:02}:{:02}:{:02}Z",
        rest / 3600,
        rest % 3600 / 60,
        rest % 60
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::proto::AgentRow;

    #[test]
    fn formats_utc() {
        assert_eq!(utc(0), "1970-01-01 00:00:00Z");
        assert_eq!(utc(1_795_473_065_000), "2026-11-23 22:31:05Z");
        assert_eq!(utc(951_782_400_000), "2000-02-29 00:00:00Z");
    }

    #[test]
    fn log_entries_show_control_characters_escaped() {
        let entry = event(&Event {
            id: 7,
            ts: 0,
            kind: Kind::Message,
            msg: None,
            from: Some("backend".into()),
            // An unknown receiver is logged as sent: it must not start a line of its own.
            to: Some("x\n1970-01-01 00:00:00Z  #8  user → rin  (hops 1)".into()),
            hops: Some(2),
            text: "one\ttab\r\ntwo \u{1b}[2J\u{9b}31m\rthree".into(),
        });
        assert_eq!(
            entry,
            "1970-01-01 00:00:00Z  #7  backend → x\\n1970-01-01 00:00:00Z  #8  user → rin  (hops 1)  (hops 2)\n    \
             one\ttab\n    two \\u{1b}[2J\\u{9b}31m\\rthree\n"
        );
    }

    #[test]
    fn agents_table_marks_you() {
        let row = |id: &str, you| AgentRow {
            id: id.into(),
            label: "Backend Engineer".into(),
            kind: "member".into(),
            harness: "pi".into(),
            sessions: 0,
            max_sessions: 3,
            queued: 0,
            you,
        };
        let text = agents(&AgentsResponse {
            agents: vec![row("rin", false), row("backend", true)],
            warnings: vec![],
        });
        assert_eq!(
            text,
            "ID       LABEL             TYPE    HARNESS  SESSIONS  QUEUED\n\
             rin      Backend Engineer  member  pi       0/3       0\n\
             backend  Backend Engineer  member  pi       0/3       0       you\n"
        );
    }
}
