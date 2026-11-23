//! `rin hub`: routes every message, enforces hops, session caps and the
//! timeout, logs everything, and watches for deadlocks.

pub mod auth;
pub mod db;
pub mod sched;

use crate::rinerrln;
use anyhow::{Context, Result};
use axum::Router;
use axum::body::Bytes;
use axum::extract::{ConnectInfo, Query, State};
use axum::http::{HeaderMap, StatusCode, header};
use axum::response::{IntoResponse, Response};
use axum::routing::{get, post};
use serde::Deserialize;
use serde_json::json;
use std::collections::HashMap;
use std::net::{IpAddr, SocketAddr};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex, MutexGuard};
use std::time::Duration;
use tokio::time::Instant;

use crate::proto::{AgentRow, AgentsResponse, RunRequest, RunResponse, SendRequest};
use crate::shared::config::{self, Config};
use auth::{Principal, Resolver};
use db::Db;
use sched::{Admit, MsgId, Sched};

/// Extra time the hub gives a runtime to report its own timeout before giving up on it.
const RUNTIME_GRACE: Duration = Duration::from_secs(5);
const DEADLOCK_CHECK_EVERY: Duration = Duration::from_secs(30);
/// How often the hub looks up every container's address in the background.
const ADDRESS_REFRESH_EVERY: Duration = Duration::from_secs(5);
/// A queued message that only gets a slot with less than this much of its
/// timeout left fails without starting: the session would be killed moments
/// later, spending tokens on an answer nobody gets.
const MIN_START_SHARE: u32 = 10;
const MIN_START_MAX: Duration = Duration::from_secs(10);

pub struct Hub {
    config: config::Config,
    /// `messaging.timeout` (tests shorten it).
    pub(crate) timeout: Duration,
    /// Hostnames of the team's containers: the agents, and Bifrost if it runs.
    containers: Vec<String>,
    /// token → who holds it
    principals: HashMap<String, Principal>,
    /// agent id → its token, used to call its runtime
    agent_tokens: HashMap<String, String>,
    resolver: Resolver,
    db: Db,
    sched: Mutex<Sched>,
    next_id: AtomicU64,
    http: reqwest::Client,
}

#[derive(Debug)]
pub enum HubError {
    NotAuthorized,
    BadRequest(String),
    UnknownAgent(String),
    ToSelf,
    HopLimit {
        hops: u32,
        max: u32,
    },
    /// Why the message ran out of time; starts with `timeout:`.
    Timeout(String),
    ReceiverDown {
        to: String,
    },
    /// An error reported by the receiver's runtime (`session failed: …`).
    Session(String),
}

impl std::fmt::Display for HubError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            HubError::NotAuthorized => write!(f, "not authorized"),
            HubError::BadRequest(why) => write!(f, "bad request: {why}"),
            HubError::UnknownAgent(id) => write!(f, "unknown agent: {id:?}"),
            HubError::ToSelf => write!(f, "rejected: an agent cannot send a message to itself"),
            HubError::HopLimit { hops, max } => {
                write!(
                    f,
                    "hop limit reached: this message would be hop {hops}, maxHops is {max}"
                )
            }
            HubError::Timeout(why) => write!(f, "{why}"),
            HubError::ReceiverDown { to } => write!(f, "receiver down: cannot reach {to}"),
            HubError::Session(text) => write!(f, "{text}"),
        }
    }
}

impl IntoResponse for HubError {
    fn into_response(self) -> Response {
        let status = match self {
            HubError::NotAuthorized => StatusCode::UNAUTHORIZED,
            HubError::BadRequest(_) | HubError::ToSelf => StatusCode::BAD_REQUEST,
            HubError::UnknownAgent(_) => StatusCode::NOT_FOUND,
            HubError::HopLimit { .. } => StatusCode::CONFLICT,
            HubError::Timeout(_) => StatusCode::GATEWAY_TIMEOUT,
            HubError::ReceiverDown { .. } | HubError::Session(_) => StatusCode::BAD_GATEWAY,
        };
        (status, axum::Json(json!({ "error": self.to_string() }))).into_response()
    }
}

impl Hub {
    pub fn new(config: Config, tokens: HashMap<String, String>, resolver: Resolver, db: Db) -> Hub {
        let agent_ids: Vec<String> = config.agents.iter().map(|a| a.id.clone()).collect();
        let mut principals = HashMap::new();
        let mut agent_tokens = HashMap::new();
        for (id, token) in tokens {
            if id == "user" {
                principals.insert(token, Principal::User);
            } else if agent_ids.contains(&id) {
                agent_tokens.insert(id.clone(), token.clone());
                principals.insert(token, Principal::Agent(id));
            }
        }
        let sched = Sched::new(
            config
                .agents
                .iter()
                .map(|a| (a.id.as_str(), a.max_sessions as usize)),
        );
        let mut containers = agent_ids.clone();
        if config.uses_api_keys() {
            containers.push("bifrost".to_string());
        }
        Hub {
            containers,
            timeout: config.messaging.timeout(),
            config,
            principals,
            agent_tokens,
            resolver,
            db,
            sched: Mutex::new(sched),
            next_id: AtomicU64::new(1),
            http: crate::client::http(),
        }
    }

    fn sched(&self) -> MutexGuard<'_, Sched> {
        self.sched.lock().unwrap_or_else(|e| e.into_inner())
    }

    pub async fn authenticate(
        &self,
        headers: &HeaderMap,
        peer: IpAddr,
    ) -> Result<Principal, HubError> {
        let authorization = headers
            .get(header::AUTHORIZATION)
            .and_then(|v| v.to_str().ok());
        auth::authenticate(
            &self.resolver,
            &self.principals,
            &self.containers,
            authorization,
            peer,
        )
        .await
        .ok_or(HubError::NotAuthorized)
    }

    /// Sends one message from `from` and waits for the receiver's final answer.
    /// The message and its outcome are logged.
    pub async fn send(&self, from: &Principal, request: SendRequest) -> Result<String, HubError> {
        let deadline = Instant::now() + self.timeout;
        let msg = self
            .db
            .message(from.id(), &request.to, request.hops, &request.text);
        let mut outcome = Outcome {
            hub: self,
            msg,
            from: from.id(),
            to: &request.to,
            logged: false,
        };
        let result = self.deliver(from.id(), &request, deadline).await;
        outcome.log(&result);
        result
    }

    async fn deliver(
        &self,
        from: &str,
        request: &SendRequest,
        deadline: Instant,
    ) -> Result<String, HubError> {
        let to = request.to.as_str();
        if self.config.agent(to).is_none() {
            return Err(HubError::UnknownAgent(to.to_string()));
        }
        if to == from {
            return Err(HubError::ToSelf);
        }
        if request.hops == 0 {
            return Err(HubError::BadRequest("hops must be at least 1".into()));
        }
        let max = self.config.messaging.max_hops;
        if request.hops > max {
            return Err(HubError::HopLimit {
                hops: request.hops,
                max,
            });
        }

        let id = self.next_id.fetch_add(1, Ordering::Relaxed);
        let _slot = Slot { hub: self, id };
        let admit = self.sched().admit(id, from, to, std::time::Instant::now());
        if let Admit::Queued(handover) = admit {
            self.check_deadlocks();
            // Either way the message never starts; `_slot` passes the slot on.
            if !matches!(
                tokio::time::timeout_at(deadline, handover).await,
                Ok(Ok(()))
            ) {
                return Err(HubError::Timeout(format!(
                    "timeout: {to} was busy for the whole {}; the message never started",
                    self.timeout_text()
                )));
            }
            let left = deadline.saturating_duration_since(Instant::now());
            if left < (self.timeout / MIN_START_SHARE).min(MIN_START_MAX) {
                return Err(HubError::Timeout(format!(
                    "timeout: {to} was busy until {} before the {} timeout; the message was not started",
                    config::format_duration(left),
                    self.timeout_text()
                )));
            }
        }
        self.run(from, request, deadline).await
    }

    fn timeout_text(&self) -> String {
        config::format_duration(self.timeout)
    }

    fn timed_out(&self, to: &str) -> HubError {
        HubError::Timeout(format!(
            "timeout: no reply from {to} within {}",
            self.timeout_text()
        ))
    }

    /// Asks the receiver's runtime to run the message as a fresh session.
    async fn run(
        &self,
        from: &str,
        request: &SendRequest,
        deadline: Instant,
    ) -> Result<String, HubError> {
        let to = request.to.as_str();
        let remaining = deadline.saturating_duration_since(Instant::now());
        if remaining.is_zero() {
            return Err(self.timed_out(to));
        }
        let from_label = match self.config.agent(from) {
            Some(agent) => agent.label.clone(),
            None => "User".to_string(),
        };
        let body = RunRequest {
            from: from.to_string(),
            from_label,
            text: request.text.clone(),
            hops: request.hops,
            // Never 0: the runtime caps the session at this.
            timeout_ms: (remaining.as_millis() as u64).max(1),
        };
        let token = self.agent_tokens.get(to).cloned().unwrap_or_default();
        let response = self
            .http
            .post(self.resolver.run_url(to))
            .bearer_auth(token)
            .json(&body)
            .timeout(remaining + RUNTIME_GRACE)
            .send()
            .await;
        let response = match response {
            Ok(response) => response,
            Err(e) if e.is_timeout() => return Err(self.timed_out(to)),
            Err(_) => return Err(HubError::ReceiverDown { to: to.to_string() }),
        };
        let status = response.status();
        let body: RunResponse = match response.json().await {
            Ok(body) => body,
            Err(e) if e.is_timeout() => return Err(self.timed_out(to)),
            Err(_) => return Err(HubError::ReceiverDown { to: to.to_string() }),
        };
        match (status, body) {
            (
                StatusCode::OK,
                RunResponse {
                    reply: Some(reply), ..
                },
            ) => Ok(reply),
            (StatusCode::GATEWAY_TIMEOUT, _) => Err(self.timed_out(to)),
            // The runtime didn't take the hub's call: say why, and how to fix a stale token.
            (StatusCode::UNAUTHORIZED, RunResponse { error, .. }) => {
                let why = error.unwrap_or_else(|| "not authorized".into());
                let hint = if why.contains("wrong token") {
                    "; `rin up` restarts it with the current tokens.json"
                } else {
                    ""
                };
                Err(HubError::Session(format!(
                    "session failed: {to} refused the hub ({why}){hint}"
                )))
            }
            (
                _,
                RunResponse {
                    error: Some(error), ..
                },
            ) => Err(HubError::Session(error)),
            (status, _) => Err(HubError::Session(format!(
                "session failed: {to} answered {status}"
            ))),
        }
    }

    /// Logs deadlocks that just appeared.
    pub fn check_deadlocks(&self) {
        let fresh = self.sched().new_deadlocks(std::time::Instant::now());
        for deadlock in fresh {
            let text = deadlock.to_string();
            rinerrln!("hub: {text}");
            self.db.warning(&deadlock.agents, &text);
        }
    }

    pub fn agents(&self, caller: &Principal) -> AgentsResponse {
        let sched = self.sched();
        let agents = self
            .config
            .agents
            .iter()
            .map(|agent| {
                let usage = sched.usage(&agent.id);
                AgentRow {
                    id: agent.id.clone(),
                    label: agent.label.clone(),
                    kind: agent.agent_type.as_str().to_string(),
                    harness: agent.harness.as_str().to_string(),
                    sessions: usage.running,
                    max_sessions: usage.max,
                    queued: usage.queued,
                    you: caller.id() == agent.id,
                }
            })
            .collect();
        let warnings = sched
            .deadlocks(std::time::Instant::now())
            .iter()
            .map(ToString::to_string)
            .collect();
        AgentsResponse { agents, warnings }
    }
}

/// Logs how a message ended, even when the sender stops waiting.
struct Outcome<'a> {
    hub: &'a Hub,
    msg: i64,
    from: &'a str,
    to: &'a str,
    logged: bool,
}

impl Outcome<'_> {
    fn log(&mut self, result: &Result<String, HubError>) {
        match result {
            Ok(reply) => self.hub.db.reply(self.msg, self.to, self.from, reply),
            Err(e) => self
                .hub
                .db
                .error(self.msg, self.to, self.from, &e.to_string()),
        }
        self.logged = true;
    }
}

impl Drop for Outcome<'_> {
    fn drop(&mut self) {
        if !self.logged {
            self.hub.db.error(
                self.msg,
                self.to,
                self.from,
                "cancelled: the sender stopped waiting",
            );
        }
    }
}

/// A message's place on its receiver: a running session or a queue spot.
/// Dropping it hands the slot to the next queued message.
struct Slot<'a> {
    hub: &'a Hub,
    id: MsgId,
}

impl Drop for Slot<'_> {
    fn drop(&mut self) {
        self.hub.sched().finish(self.id);
        self.hub.check_deadlocks();
    }
}

pub fn router(hub: Arc<Hub>) -> Router {
    Router::new()
        .route("/messages", post(post_messages))
        .route("/agents", get(get_agents))
        .route("/log", get(get_log))
        .layer(axum::extract::DefaultBodyLimit::max(crate::proto::MAX_BODY))
        .with_state(hub)
}

async fn post_messages(
    State(hub): State<Arc<Hub>>,
    ConnectInfo(peer): ConnectInfo<SocketAddr>,
    headers: HeaderMap,
    body: Bytes,
) -> Response {
    let caller = match hub.authenticate(&headers, peer.ip()).await {
        Ok(caller) => caller,
        Err(e) => return e.into_response(),
    };
    let request: SendRequest = match serde_json::from_slice(&body) {
        Ok(request) => request,
        Err(e) => return HubError::BadRequest(e.to_string()).into_response(),
    };
    match hub.send(&caller, request).await {
        Ok(reply) => axum::Json(json!({ "reply": reply })).into_response(),
        Err(e) => e.into_response(),
    }
}

async fn get_agents(
    State(hub): State<Arc<Hub>>,
    ConnectInfo(peer): ConnectInfo<SocketAddr>,
    headers: HeaderMap,
) -> Response {
    match hub.authenticate(&headers, peer.ip()).await {
        Ok(caller) => axum::Json(hub.agents(&caller)).into_response(),
        Err(e) => e.into_response(),
    }
}

#[derive(Deserialize)]
struct LogQuery {
    agent: Option<String>,
    #[serde(default)]
    after: i64,
}

async fn get_log(
    State(hub): State<Arc<Hub>>,
    ConnectInfo(peer): ConnectInfo<SocketAddr>,
    headers: HeaderMap,
    Query(query): Query<LogQuery>,
) -> Response {
    match hub.authenticate(&headers, peer.ip()).await {
        Ok(Principal::User) => {}
        Ok(Principal::Agent(_)) | Err(_) => return HubError::NotAuthorized.into_response(),
    }
    match hub.db.events(query.agent.as_deref(), query.after) {
        Ok(events) => axum::Json(json!({ "events": events })).into_response(),
        Err(e) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            axum::Json(json!({ "error": e.to_string() })),
        )
            .into_response(),
    }
}

fn env_or(name: &str, default: &str) -> String {
    std::env::var(name).unwrap_or_else(|_| default.to_string())
}

/// Entry point of `rin hub` in the hub container.
pub async fn main() -> Result<()> {
    let config_path = env_or("RIN_CONFIG", "/etc/rin/rin.json");
    let tokens_path = env_or("RIN_TOKENS", "/etc/rin/tokens.json");
    let db_path = env_or("RIN_DB", "/var/lib/rin/hub.db");
    let listen = env_or("RIN_LISTEN", "0.0.0.0:7000");

    let config = config::from_json_file(std::path::Path::new(&config_path))?;
    let text = std::fs::read_to_string(&tokens_path)
        .with_context(|| format!("cannot read {tokens_path}"))?;
    let tokens: HashMap<String, String> = serde_json::from_str(&text)
        .with_context(|| format!("{tokens_path} is not a tokens file"))?;
    let db = Db::open(std::path::Path::new(&db_path))?;

    let hub = Arc::new(Hub::new(config, tokens, Resolver::dns(), db));
    let ticker = hub.clone();
    tokio::spawn(async move {
        let mut every = tokio::time::interval(DEADLOCK_CHECK_EVERY);
        loop {
            every.tick().await;
            ticker.check_deadlocks();
        }
    });

    // Learn the containers' addresses before anyone asks, so a recreated hub isn't blind.
    let refresher = hub.clone();
    tokio::spawn(async move {
        let hosts: Vec<&str> = refresher.containers.iter().map(String::as_str).collect();
        let mut every = tokio::time::interval(ADDRESS_REFRESH_EVERY);
        loop {
            every.tick().await;
            refresher.resolver.refresh(&hosts).await;
        }
    });

    let listener = tokio::net::TcpListener::bind(&listen)
        .await
        .with_context(|| format!("cannot listen on {listen}"))?;
    rinerrln!("hub: team {} listening on {listen}", hub.config.name);
    let app = router(hub).into_make_service_with_connect_info::<SocketAddr>();
    tokio::select! {
        result = axum::serve(listener, app) => result?,
        _ = crate::signals::terminated() => {}
    }
    Ok(())
}
