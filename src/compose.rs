//! The generated `docker-compose.yml`. It holds no secrets: tokens and
//! virtual keys reach containers through `env/<id>.env` files.

use crate::config::{Agent, Authentication, Config, Harness};
use crate::yaml::{Yaml, list, map, s};

pub const BIFROST_IMAGE: &str = "maximhq/bifrost:v2.5.1";
pub const BIFROST_URL: &str = "http://bifrost:8080";

pub struct Inputs<'a> {
    pub config: &'a Config,
    /// Git revision of Rin the images build `rin` from (`v<version>` for releases).
    pub git_rev: &'a str,
}

/// Non-secret environment for an agent's container.
pub fn agent_environment(config: &Config, agent: &Agent) -> Vec<(&'static str, String)> {
    let mut env = vec![
        ("RIN_HUB_URL", "http://hub:7000".to_string()),
        ("RIN_MODEL", agent.model.clone()),
        ("RIN_TIMEOUT", config.messaging.timeout.clone()),
    ];
    if agent.authentication == Authentication::ApiKey {
        env.push(("RIN_BIFROST_URL", BIFROST_URL.to_string()));
        match agent.harness {
            Harness::ClaudeCode => env.push(("ANTHROPIC_BASE_URL", format!("{BIFROST_URL}/anthropic"))),
            Harness::Antigravity => env.push(("GOOGLE_GEMINI_BASE_URL", format!("{BIFROST_URL}/genai"))),
            // Codex and pi read their gateway from config files `rin runtime` writes.
            Harness::Codex | Harness::Pi => {}
        }
    }
    env
}

fn build(target: &str, inputs: &Inputs) -> Yaml {
    map([
        ("context", s("./images")),
        ("target", s(target)),
        ("args", map([("RIN_GIT_REV", s(inputs.git_rev))])),
    ])
}

fn volume_name(config: &Config, what: &str) -> String {
    format!("rin-{}-{what}", config.name)
}

pub fn render(inputs: &Inputs) -> String {
    let config = inputs.config;
    let version = &config.version;
    let mut services = vec![(
        "hub".to_string(),
        map([
            ("build", build("hub", inputs)),
            ("image", s(format!("rin-{}-hub:{version}", config.name))),
            ("hostname", s("hub")),
            ("init", Yaml::Raw("true".into())),
            ("environment", map([
                ("RIN_CONFIG", s("/etc/rin/rin.json")),
                ("RIN_TOKENS", s("/etc/rin/tokens.json")),
                ("RIN_DB", s("/var/lib/rin/hub.db")),
            ])),
            ("volumes", list([
                s(format!("{}:/var/lib/rin", volume_name(config, "hub"))),
                s("./rin.json:/etc/rin/rin.json:ro"),
                s("./tokens.json:/etc/rin/tokens.json:ro"),
            ])),
            ("ports", list([s("127.0.0.1:10302:7000")])),
        ]),
    )];
    let mut volumes = vec![volume_name(config, "hub")];

    for agent in &config.agents {
        let home = volume_name(config, &format!("{}-home", agent.id));
        let environment = agent_environment(config, agent).into_iter().map(|(k, v)| (k, s(v)));
        services.push((
            agent.id.clone(),
            map([
                ("build", build(agent.harness.as_str(), inputs)),
                ("image", s(format!("rin-{}-{}:{version}", config.name, agent.id))),
                ("hostname", s(agent.id.clone())),
                ("init", Yaml::Raw("true".into())),
                ("working_dir", s("/home/agent/work")),
                ("env_file", list([s(format!("./env/{}.env", agent.id))])),
                ("environment", map(environment)),
                ("volumes", list([
                    s(format!("{home}:/home/agent")),
                    s(format!(
                        "./instructions/{}/INSTRUCTIONS.md:{}:ro",
                        agent.id,
                        agent.harness.instructions_path()
                    )),
                ])),
                ("cpus", Yaml::Raw(agent.resources.cpus.to_string())),
                ("mem_limit", s(agent.resources.ram.clone())),
            ]),
        ));
        volumes.push(home);
    }

    if config.uses_api_keys() {
        services.push((
            "bifrost".to_string(),
            map([
                ("image", s(BIFROST_IMAGE)),
                ("hostname", s("bifrost")),
                ("env_file", list([s("../../keys.env"), s("./bifrost/bifrost.env")])),
                ("volumes", list([
                    s(format!("{}:/app/data", volume_name(config, "bifrost"))),
                    s("./bifrost/config.json:/app/data/config.json:ro"),
                ])),
            ]),
        ));
        volumes.push(volume_name(config, "bifrost"));
    }

    let doc = map([
        ("name", s(format!("rin-{}", config.name))),
        ("services", Yaml::Map(services)),
        ("volumes", map(volumes.into_iter().map(|v| (v.clone(), map([("name", s(v))]))))),
    ]);
    format!("# Written by `rin generate`: change rin.yml and run it again rather than editing this file.\n{}", doc.render())
}
