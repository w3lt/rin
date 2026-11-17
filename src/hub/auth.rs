//! Who is calling: the hub maps the bearer token to an agent (or you) and
//! checks the request comes from that agent's own container.

use std::collections::HashMap;
use std::net::IpAddr;
use std::time::Duration;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Principal {
    /// You, from your machine.
    User,
    Agent(String),
}

impl Principal {
    pub fn id(&self) -> &str {
        match self {
            Principal::User => "user",
            Principal::Agent(id) => id,
        }
    }
}

/// Finds containers on the Compose network.
pub enum Resolver {
    /// Docker's DNS, asked on every request so restarted containers' new IPs are picked up.
    Dns,
    /// Fixed addresses and runtime URLs, for tests.
    #[cfg_attr(not(test), allow(dead_code))]
    Static { ips: HashMap<String, IpAddr>, run_urls: HashMap<String, String> },
}

/// A stopped container's name is unknown to Docker's DNS, which may ask
/// upstream; don't let that hold up a request.
const LOOKUP_TIMEOUT: Duration = Duration::from_secs(2);

async fn lookup(host: String) -> Option<IpAddr> {
    let addrs = tokio::time::timeout(LOOKUP_TIMEOUT, tokio::net::lookup_host((host.as_str(), 0))).await.ok()?.ok()?;
    addrs.map(|addr| addr.ip().to_canonical()).find(IpAddr::is_ipv4)
}

impl Resolver {
    pub async fn ip(&self, host: &str) -> Option<IpAddr> {
        match self {
            Resolver::Dns => lookup(host.to_string()).await,
            Resolver::Static { ips, .. } => ips.get(host).copied(),
        }
    }

    /// Whether `peer` is any of `hosts`, looked up concurrently.
    pub async fn is_any(&self, hosts: &[&str], peer: IpAddr) -> bool {
        match self {
            Resolver::Dns => {
                let mut lookups = tokio::task::JoinSet::new();
                for host in hosts {
                    lookups.spawn(lookup(host.to_string()));
                }
                while let Some(found) = lookups.join_next().await {
                    if found.ok().flatten() == Some(peer) {
                        return true;
                    }
                }
                false
            }
            Resolver::Static { ips, .. } => hosts.iter().any(|h| ips.get(*h) == Some(&peer)),
        }
    }

    /// Where agent `id`'s runtime listens.
    pub fn run_url(&self, id: &str) -> String {
        match self {
            Resolver::Dns => format!("http://{id}:7001/run"),
            Resolver::Static { run_urls, .. } => run_urls.get(id).cloned().unwrap_or_default(),
        }
    }
}

/// Identifies the caller from its `Authorization` header and source IP.
/// `agents` are all agent ids; `tokens` maps token → principal.
pub async fn authenticate(
    resolver: &Resolver,
    tokens: &HashMap<String, Principal>,
    agents: &[String],
    authorization: Option<&str>,
    peer: IpAddr,
) -> Option<Principal> {
    let token = authorization?.strip_prefix("Bearer ")?.trim();
    let principal = tokens.get(token)?.clone();
    let peer = peer.to_canonical();
    match &principal {
        Principal::Agent(id) => (resolver.ip(id).await == Some(peer)).then_some(principal),
        Principal::User => {
            // Your token is only good from outside the team's containers.
            let hosts: Vec<&str> = agents.iter().map(String::as_str).chain(["bifrost"]).collect();
            (!resolver.is_any(&hosts, peer).await).then_some(principal)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ip(s: &str) -> IpAddr {
        s.parse().unwrap()
    }

    #[tokio::test]
    async fn token_and_ip_must_match() {
        let resolver = Resolver::Static {
            ips: [("rin", "10.0.0.2"), ("backend", "10.0.0.3"), ("bifrost", "10.0.0.9")]
                .into_iter()
                .map(|(k, v)| (k.to_string(), ip(v)))
                .collect(),
            run_urls: HashMap::new(),
        };
        let tokens: HashMap<String, Principal> = [
            ("tok-user", Principal::User),
            ("tok-rin", Principal::Agent("rin".into())),
            ("tok-backend", Principal::Agent("backend".into())),
        ]
        .into_iter()
        .map(|(k, v)| (k.to_string(), v))
        .collect();
        let agents = vec!["rin".to_string(), "backend".to_string()];
        let auth = |header: Option<&'static str>, peer: &'static str| {
            let (resolver, tokens, agents) = (&resolver, &tokens, &agents);
            async move { authenticate(resolver, tokens, agents, header, ip(peer)).await }
        };

        assert_eq!(auth(Some("Bearer tok-backend"), "10.0.0.3").await, Some(Principal::Agent("backend".into())));
        // Another agent's token, from backend's container.
        assert_eq!(auth(Some("Bearer tok-rin"), "10.0.0.3").await, None);
        // Your token from any container.
        assert_eq!(auth(Some("Bearer tok-user"), "10.0.0.3").await, None);
        assert_eq!(auth(Some("Bearer tok-user"), "10.0.0.9").await, None);
        // Your token from your machine (the network gateway).
        assert_eq!(auth(Some("Bearer tok-user"), "10.0.0.1").await, Some(Principal::User));
        // IPv4-mapped IPv6 peers are the same address.
        assert_eq!(auth(Some("Bearer tok-backend"), "::ffff:10.0.0.3").await, Some(Principal::Agent("backend".into())));
        // No token, garbage, wrong scheme.
        assert_eq!(auth(None, "10.0.0.1").await, None);
        assert_eq!(auth(Some("Bearer nope"), "10.0.0.1").await, None);
        assert_eq!(auth(Some("tok-user"), "10.0.0.1").await, None);
        // An agent whose container is gone can't be anyone.
        assert_eq!(auth(Some("Bearer tok-rin"), "10.0.0.7").await, None);
    }
}
