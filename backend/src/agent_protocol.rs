use std::sync::OnceLock;

use serde::Deserialize;

#[derive(Debug, Deserialize)]
struct AgentProtocolFile {
    #[serde(rename = "defaultBaseUrl")]
    default_base_url: String,
    #[serde(rename = "writeActions")]
    write_actions: Vec<AgentWriteActionDefinition>,
    control: AgentControlDefinition,
    #[serde(rename = "taskAnalysis")]
    task_analysis: AgentTaskAnalysisDefinition,
    routes: Vec<AgentRouteDefinition>,
}

#[derive(Debug, Deserialize)]
struct AgentWriteActionDefinition {
    mode: String,
    marker: String,
    #[serde(rename = "payloadFormat")]
    payload_format: Option<String>,
    example: Option<String>,
}

#[derive(Debug, Deserialize)]
struct AgentControlDefinition {
    #[serde(rename = "openMarker")]
    open_marker: String,
    #[serde(rename = "closeMarker")]
    close_marker: String,
    phases: Vec<String>,
}

#[derive(Debug, Deserialize)]
struct AgentTaskAnalysisDefinition {
    modes: Vec<String>,
    complexities: Vec<String>,
    intents: Vec<String>,
    #[serde(rename = "writeScopes")]
    write_scopes: Vec<String>,
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

pub fn write_action_modes() -> Vec<&'static str> {
    protocol()
        .write_actions
        .iter()
        .map(|item| item.mode.as_str())
        .collect()
}

pub fn write_action_markers() -> Vec<&'static str> {
    protocol()
        .write_actions
        .iter()
        .map(|item| item.marker.as_str())
        .collect()
}

pub fn write_action_payload_formats() -> Vec<String> {
    protocol()
        .write_actions
        .iter()
        .map(|item| {
            let payload = item
                .payload_format
                .as_deref()
                .unwrap_or("未定义 payload 格式");
            let example = item.example.as_deref().unwrap_or("");
            if example.is_empty() {
                format!("{}: {}", item.mode, payload)
            } else {
                format!("{}: {} 示例: {}", item.mode, payload, example)
            }
        })
        .collect()
}

pub fn control_open_marker() -> &'static str {
    protocol().control.open_marker.as_str()
}

pub fn control_close_marker() -> &'static str {
    protocol().control.close_marker.as_str()
}

pub fn control_phases() -> Vec<&'static str> {
    protocol()
        .control
        .phases
        .iter()
        .map(|phase| phase.as_str())
        .collect()
}

pub fn task_analysis_modes() -> Vec<&'static str> {
    protocol()
        .task_analysis
        .modes
        .iter()
        .map(|item| item.as_str())
        .collect()
}

pub fn task_analysis_complexities() -> Vec<&'static str> {
    protocol()
        .task_analysis
        .complexities
        .iter()
        .map(|item| item.as_str())
        .collect()
}

pub fn task_analysis_intents() -> Vec<&'static str> {
    protocol()
        .task_analysis
        .intents
        .iter()
        .map(|item| item.as_str())
        .collect()
}

pub fn task_analysis_write_scopes() -> Vec<&'static str> {
    protocol()
        .task_analysis
        .write_scopes
        .iter()
        .map(|item| item.as_str())
        .collect()
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
