//! `rin runtime`: the agent container's main process. Writes the harness's
//! settings, then runs each message from the hub as a fresh headless session.

pub mod harness;
pub mod settings;

use std::net::{IpAddr, SocketAddr};
use std::path::{Path, PathBuf};
use std::process::Stdio;
use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::Duration;

use anyhow::{Context, Result, anyhow, bail};
use axum::Router;
use axum::body::Bytes;
use axum::extract::{ConnectInfo, State};
use axum::http::{HeaderMap, StatusCode, header};
use axum::response::{IntoResponse, Response};
use axum::routing::post;
use nix::sys::signal::{Signal, killpg};
use nix::unistd::Pid;

use crate::config::{Harness, format_duration, parse_minutes};
use crate::proto::{RunRequest, RunResponse};

/// How many lines of a failed session's output go back to the sender.
const LAST_LINES: usize = 20;

pub struct Runtime {
    pub harness: Harness,
    pub model: String,
    /// The team's timeout; no session runs longer.
    pub timeout: Duration,
    token: String,
    /// Host of `RIN_HUB_URL`: only requests from its address are accepted.
    hub_host: String,
    home: PathBuf,
    /// Replaces the harness binary (tests use a fake harness).
    pub program: Option<PathBuf>,
    sessions: AtomicU64,
}

#[derive(Debug)]
pub enum SessionError {
    Timeout(Duration),
    Failed(String),
}

/// The prompt a delegated session starts with.
pub fn prompt(from: &str, from_label: &str, text: &str) -> String {
    format!("[Rin] Message from {from} ({from_label}).\nYour final answer will be sent back to them as the reply.\n\n{text}")
}

/// The host in `RIN_HUB_URL`, without IPv6 brackets.
fn host_of(url: &str) -> String {
    let parsed = reqwest::Url::parse(url).ok();
    let host = parsed.as_ref().and_then(|u| u.host_str()).unwrap_or(url);
    host.trim_start_matches('[').trim_end_matches(']').to_string()
}

impl Runtime {
    pub fn new(harness: Harness, model: &str, timeout: Duration, token: &str, hub_url: &str, home: &Path) -> Runtime {
        Runtime {
            harness,
            model: model.to_string(),
            timeout,
            token: token.to_string(),
            hub_host: host_of(hub_url),
            home: home.to_path_buf(),
            program: None,
            sessions: AtomicU64::new(0),
        }
    }

    /// Only the hub may call: with this agent's token, from the hub's address.
    async fn authorized(&self, headers: &HeaderMap, peer: IpAddr) -> bool {
        let expected = format!("Bearer {}", self.token);
        let given = headers.get(header::AUTHORIZATION).and_then(|v| v.to_str().ok()).unwrap_or("");
        if self.token.is_empty() || given != expected {
            return false;
        }
        let hub = tokio::net::lookup_host((self.hub_host.as_str(), 0))
            .await
            .ok()
            .and_then(|mut addrs| addrs.find(|a| a.is_ipv4()))
            .map(|a| a.ip());
        hub == Some(peer.to_canonical())
    }

    /// Runs one message as a fresh headless session and returns its final answer.
    /// The session and everything it started are killed when it ends, when
    /// `limit` runs out, or when the hub stops waiting.
    pub async fn run_session(&self, prompt: &str, hops: u32, limit: Duration) -> Result<String, SessionError> {
        let marker = format!("{}-{}", std::process::id(), self.sessions.fetch_add(1, Ordering::Relaxed));
        let scratch = self.home.join(".cache/rin");
        let work = self.home.join("work");
        for dir in [&scratch, &work] {
            std::fs::create_dir_all(dir).map_err(|e| SessionError::Failed(format!("cannot create {}: {e}", dir.display())))?;
        }
        let files = SessionFiles::new(&scratch, &marker);
        let invocation = harness::invocation(self.harness, &self.model, prompt, &files.answer);
        let program = self.program.clone().unwrap_or_else(|| PathBuf::from(invocation.program));
        // Output goes to files, not pipes: a background process the session
        // leaves behind can't hold them open and stall the reply.
        let create = |path: &Path| {
            std::fs::File::create(path).map_err(|e| SessionError::Failed(format!("cannot create {}: {e}", path.display())))
        };
        let mut command = tokio::process::Command::new(&program);
        command
            .args(&invocation.args)
            .current_dir(&work)
            .env("RIN_HOPS", hops.to_string())
            .env("RIN_SESSION", &marker)
            .stdin(Stdio::null())
            .stdout(create(&files.stdout)?)
            .stderr(create(&files.stderr)?)
            .process_group(0)
            .kill_on_drop(true);
        let mut child = command
            .spawn()
            .map_err(|e| SessionError::Failed(format!("cannot start {}: {e}", program.display())))?;
        let _kill = KillOnDrop { group: child.id(), marker };

        let status = match tokio::time::timeout(limit, child.wait()).await {
            Err(_) => return Err(SessionError::Timeout(limit)),
            Ok(Err(e)) => return Err(SessionError::Failed(format!("lost the session: {e}"))),
            Ok(Ok(status)) => status,
        };
        let stdout = files.read(&files.stdout);
        let stderr = files.read(&files.stderr);
        let tail = || last_lines(&format!("{stdout}\n{stderr}"), LAST_LINES);
        if !status.success() {
            return Err(SessionError::Failed(format!("{} exited with {status}:\n{}", program.display(), tail())));
        }
        let answer = std::fs::read_to_string(&files.answer).ok();
        harness::answer(self.harness, &stdout, answer.as_deref())
            .map_err(|why| SessionError::Failed(format!("{why}\n{}", tail())))
    }
}

/// A session's output and answer files in `~/.cache/rin`, removed afterwards.
struct SessionFiles {
    stdout: PathBuf,
    stderr: PathBuf,
    answer: PathBuf,
}

impl SessionFiles {
    fn new(dir: &Path, marker: &str) -> SessionFiles {
        let path = |what: &str| dir.join(format!("{marker}.{what}"));
        SessionFiles { stdout: path("out"), stderr: path("err"), answer: path("answer") }
    }

    fn read(&self, path: &Path) -> String {
        std::fs::read(path).map(|b| String::from_utf8_lossy(&b).into_owned()).unwrap_or_default()
    }
}

impl Drop for SessionFiles {
    fn drop(&mut self) {
        for path in [&self.stdout, &self.stderr, &self.answer] {
            let _ = std::fs::remove_file(path);
        }
    }
}

fn last_lines(text: &str, n: usize) -> String {
    let lines: Vec<&str> = text.lines().filter(|l| !l.trim().is_empty()).collect();
    lines[lines.len().saturating_sub(n)..].join("\n")
}

/// Kills a session's process group, and every process that inherited its
/// `RIN_SESSION` marker (harness tools may start their own groups).
struct KillOnDrop {
    group: Option<u32>,
    #[cfg_attr(not(target_os = "linux"), allow(dead_code))]
    marker: String,
}

impl Drop for KillOnDrop {
    fn drop(&mut self) {
        if let Some(group) = self.group {
            let _ = killpg(Pid::from_raw(group as i32), Signal::SIGKILL);
        }
        #[cfg(target_os = "linux")]
        {
            // Reading every /proc/*/environ blocks: keep it off the async workers.
            let marker = std::mem::take(&mut self.marker);
            match tokio::runtime::Handle::try_current() {
                Ok(handle) => drop(handle.spawn_blocking(move || kill_marked(&marker))),
                Err(_) => kill_marked(&marker),
            }
        }
    }
}

#[cfg(target_os = "linux")]
fn kill_marked(marker: &str) {
    let needle = format!("RIN_SESSION={marker}");
    let Ok(entries) = std::fs::read_dir("/proc") else { return };
    for entry in entries.flatten() {
        let Ok(pid) = entry.file_name().to_string_lossy().parse::<i32>() else { continue };
        let Ok(environ) = std::fs::read(entry.path().join("environ")) else { continue };
        if environ.split(|b| *b == 0).any(|var| var == needle.as_bytes()) {
            let _ = nix::sys::signal::kill(Pid::from_raw(pid), Signal::SIGKILL);
        }
    }
}

pub fn router(runtime: Arc<Runtime>) -> Router {
    // A bit above the hub's limit: the hub adds the sender's name and label to the text.
    let limit = axum::extract::DefaultBodyLimit::max(crate::proto::MAX_BODY + (64 << 10));
    Router::new().route("/run", post(post_run)).layer(limit).with_state(runtime)
}

fn error(status: StatusCode, text: String) -> Response {
    (status, axum::Json(RunResponse { reply: None, error: Some(text) })).into_response()
}

async fn post_run(
    State(runtime): State<Arc<Runtime>>,
    ConnectInfo(peer): ConnectInfo<SocketAddr>,
    headers: HeaderMap,
    body: Bytes,
) -> Response {
    if !runtime.authorized(&headers, peer.ip()).await {
        return error(StatusCode::UNAUTHORIZED, "not authorized".into());
    }
    let request: RunRequest = match serde_json::from_slice(&body) {
        Ok(request) => request,
        Err(e) => return error(StatusCode::BAD_REQUEST, format!("bad request: {e}")),
    };
    let limit = Duration::from_millis(request.timeout_ms).min(runtime.timeout);
    let prompt = prompt(&request.from, &request.from_label, &request.text);
    match runtime.run_session(&prompt, request.hops, limit).await {
        Ok(reply) => axum::Json(RunResponse { reply: Some(reply), error: None }).into_response(),
        Err(SessionError::Timeout(after)) => {
            error(StatusCode::GATEWAY_TIMEOUT, format!("timeout: session killed after {}", format_duration(after)))
        }
        Err(SessionError::Failed(why)) => error(StatusCode::BAD_GATEWAY, format!("session failed: {why}")),
    }
}

fn required(name: &str) -> Result<String> {
    std::env::var(name).map_err(|_| anyhow!("{name} is not set"))
}

/// Entry point of `rin runtime` in an agent container.
pub async fn main() -> Result<()> {
    let harness_name = required("RIN_HARNESS")?;
    let harness = Harness::parse(&harness_name).ok_or_else(|| anyhow!("unknown RIN_HARNESS {harness_name:?}"))?;
    let model = required("RIN_MODEL")?;
    let timeout_text = std::env::var("RIN_TIMEOUT").unwrap_or_else(|_| "30m".into());
    let timeout = parse_minutes(&timeout_text).ok_or_else(|| anyhow!("bad RIN_TIMEOUT {timeout_text:?}"))?;
    let token = required("RIN_TOKEN")?;
    if token.is_empty() {
        bail!("RIN_TOKEN is empty");
    }
    let hub_url = std::env::var("RIN_HUB_URL").unwrap_or_else(|_| "http://hub:7000".into());
    let home = PathBuf::from(std::env::var("HOME").unwrap_or_else(|_| "/home/agent".into()));
    let bifrost = std::env::var("RIN_BIFROST_URL").ok().filter(|u| !u.is_empty());

    let written = settings::apply(
        &home,
        &settings::Settings { harness, model: &model, timeout, bifrost: bifrost.as_deref() },
    )
    .context("cannot write harness settings")?;
    for path in written {
        eprintln!("rin runtime: updated {}", path.display());
    }
    std::fs::create_dir_all(home.join("work")).context("cannot create ~/work")?;

    let runtime = Arc::new(Runtime::new(harness, &model, timeout, &token, &hub_url, &home));
    let listener = tokio::net::TcpListener::bind("0.0.0.0:7001").await.context("cannot listen on 7001")?;
    eprintln!("rin runtime: {} on {model}, listening on 7001", harness.as_str());
    let app = router(runtime).into_make_service_with_connect_info::<SocketAddr>();
    tokio::select! {
        result = axum::serve(listener, app) => result?,
        _ = crate::signals::terminated() => {}
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn prompt_header() {
        assert_eq!(
            prompt("rin", "Chief Architect", "Reply with exactly: pong"),
            "[Rin] Message from rin (Chief Architect).\nYour final answer will be sent back to them as the reply.\n\nReply with exactly: pong"
        );
    }

    #[test]
    fn hub_host_from_url() {
        assert_eq!(host_of("http://hub:7000"), "hub");
        assert_eq!(host_of("http://127.0.0.1:1234/"), "127.0.0.1");
        assert_eq!(host_of("hub"), "hub");
        assert_eq!(host_of("http://[fd00::2]:7000"), "fd00::2");
    }

    #[test]
    fn keeps_the_last_lines() {
        assert_eq!(last_lines("a\n\nb\nc\n", 2), "b\nc");
    }
}
