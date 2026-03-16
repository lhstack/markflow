use std::sync::OnceLock;

use serde::Deserialize;

#[derive(Debug, Deserialize)]
struct AgentProtocolFile {
    #[serde(rename = "defaultBaseUrl")]
    default_base_url: String,
    routes: Vec<AgentRouteDefinition>,
}

#[allow(dead_code)]
#[derive(Debug, Deserialize)]
struct AgentRouteDefinition {
    route: String,
    aliases: Vec<String>,
    path: String,
    description: String,
    params: Vec<String>,
    #[serde(rename = "nodeType")]
    node_type: Option<String>,
}

fn protocol() -> &'static AgentProtocolFile {
    static PROTOCOL: OnceLock<AgentProtocolFile> = OnceLock::new();
    PROTOCOL.get_or_init(|| {
        serde_json::from_str(include_str!("../../frontend/src/agent/agent-protocol.json"))
            .expect("agent protocol should be valid json")
    })
}

pub fn default_agent_base_url() -> &'static str {
    protocol().default_base_url.as_str()
}

pub fn route_enum_values() -> Vec<&'static str> {
    protocol()
        .routes
        .iter()
        .map(|route| route.route.as_str())
        .collect()
}

pub fn route_descriptions() -> Vec<String> {
    protocol()
        .routes
        .iter()
        .map(|route| {
            let mut line = format!("{}={}", route.route, route.description);
            if let Some(node_type) = route.node_type.as_deref() {
                line.push_str(&format!("（node_type={}）", node_type));
            }
            line
        })
        .collect()
}
