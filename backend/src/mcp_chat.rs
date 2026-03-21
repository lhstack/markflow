use std::{collections::HashMap, time::Duration};

use rig::completion::ToolDefinition as RigToolDefinition;
use rmcp::model::{CallToolRequestParams, GetPromptRequestParams, JsonObject, ReadResourceRequestParams};
use serde_json::{json, Value};

use crate::{
    mcp::{self, McpConnectionManager, ResolvedMcpServerConfig},
    models::AgentMcpServer,
    BackendRuntimeConfig,
};

const MCP_CHAT_DISCOVERY_TIMEOUT: Duration = Duration::from_secs(6);
const MAX_MCP_TOOLS_PER_SERVER: usize = 12;

#[derive(Clone)]
enum McpChatToolHandler {
    RemoteTool {
        client: rmcp::service::ServerSink,
        remote_name: String,
        server_name: String,
    },
    ReadResource {
        client: rmcp::service::ServerSink,
        server_name: String,
    },
    GetPrompt {
        client: rmcp::service::ServerSink,
        server_name: String,
    },
}

pub struct McpChatToolRegistry {
    definitions: Vec<RigToolDefinition>,
    handlers: HashMap<String, McpChatToolHandler>,
}

impl McpChatToolRegistry {
    pub fn empty() -> Self {
        Self {
            definitions: Vec::new(),
            handlers: HashMap::new(),
        }
    }

    pub fn definitions(&self) -> &[RigToolDefinition] {
        &self.definitions
    }

    pub async fn execute(&self, name: &str, arguments: &Value) -> Option<Value> {
        let handler = self.handlers.get(name)?.clone();
        Some(match handler {
            McpChatToolHandler::RemoteTool {
                client,
                remote_name,
                server_name,
            } => {
                let args = arguments.as_object().cloned().unwrap_or_else(JsonObject::default);
                match client
                    .call_tool(CallToolRequestParams {
                        name: remote_name.into(),
                        arguments: Some(args),
                        meta: None,
                        task: None,
                    })
                    .await
                {
                    Ok(result) => json!({
                        "ok": true,
                        "server": server_name,
                        "kind": "tool",
                        "result": result,
                    }),
                    Err(err) => json!({
                        "ok": false,
                        "server": server_name,
                        "kind": "tool",
                        "error": err.to_string(),
                    }),
                }
            }
            McpChatToolHandler::ReadResource { client, server_name } => {
                let uri = arguments
                    .get("uri")
                    .and_then(Value::as_str)
                    .map(str::trim)
                    .filter(|value| !value.is_empty());
                let Some(uri) = uri else {
                    return Some(json!({
                        "ok": false,
                        "server": server_name,
                        "kind": "resource",
                        "error": "缺少资源 URI"
                    }));
                };

                match client
                    .read_resource(ReadResourceRequestParams {
                        meta: None,
                        uri: uri.to_string(),
                    })
                    .await
                {
                    Ok(result) => json!({
                        "ok": true,
                        "server": server_name,
                        "kind": "resource",
                        "result": result,
                    }),
                    Err(err) => json!({
                        "ok": false,
                        "server": server_name,
                        "kind": "resource",
                        "error": err.to_string(),
                    }),
                }
            }
            McpChatToolHandler::GetPrompt { client, server_name } => {
                let prompt_name = arguments
                    .get("name")
                    .and_then(Value::as_str)
                    .map(str::trim)
                    .filter(|value| !value.is_empty());
                let Some(prompt_name) = prompt_name else {
                    return Some(json!({
                        "ok": false,
                        "server": server_name,
                        "kind": "prompt",
                        "error": "缺少 prompt 名称"
                    }));
                };

                let prompt_arguments = arguments
                    .get("arguments")
                    .and_then(Value::as_object)
                    .cloned();
                match client
                    .get_prompt(GetPromptRequestParams {
                        meta: None,
                        name: prompt_name.to_string(),
                        arguments: prompt_arguments,
                    })
                    .await
                {
                    Ok(result) => json!({
                        "ok": true,
                        "server": server_name,
                        "kind": "prompt",
                        "result": result,
                    }),
                    Err(err) => json!({
                        "ok": false,
                        "server": server_name,
                        "kind": "prompt",
                        "error": err.to_string(),
                    }),
                }
            }
        })
    }
}

pub struct McpChatBuildResult {
    pub registry: McpChatToolRegistry,
    pub warnings: Vec<String>,
}

pub async fn build_mcp_chat_registry(
    manager: &McpConnectionManager,
    runtime: &BackendRuntimeConfig,
    rows: &[AgentMcpServer],
) -> McpChatBuildResult {
    let mut registry = McpChatToolRegistry::empty();
    let mut warnings = Vec::new();

    for row in rows {
        let config = match mcp::load_runtime_server_config(row) {
            Ok(config) => config,
            Err(err) => {
                warnings.push(format!("MCP 服务“{}”配置读取失败：{}", row.name, err));
                continue;
            }
        };

        if !config.enabled {
            continue;
        }

        if let Err(err) = attach_server_tools(&mut registry, manager, runtime, &config, &mut warnings).await {
            warnings.push(format!("MCP 服务“{}”初始化失败：{}", config.name, err));
        }
    }

    McpChatBuildResult { registry, warnings }
}

async fn attach_server_tools(
    registry: &mut McpChatToolRegistry,
    manager: &McpConnectionManager,
    runtime: &BackendRuntimeConfig,
    config: &ResolvedMcpServerConfig,
    warnings: &mut Vec<String>,
) -> anyhow::Result<()> {
    let connection = manager.get_or_connect(runtime, config, false).await?;
    let client = connection.peer.clone();
    let tool_prefix = format!("mcp_server_{}", config.id);

    let tools = tokio::time::timeout(MCP_CHAT_DISCOVERY_TIMEOUT, client.list_all_tools())
        .await
        .map_err(|_| anyhow::anyhow!("列出工具超时"))?
        .map_err(|err| anyhow::anyhow!(err.to_string()))?;

    for tool in tools.into_iter().take(MAX_MCP_TOOLS_PER_SERVER) {
        let definition_name = format!(
            "{}_tool_{}",
            tool_prefix,
            sanitize_tool_name_fragment(tool.name.as_ref())
        );
        registry.definitions.push(RigToolDefinition {
            name: definition_name.clone(),
            description: format!(
                "调用 MCP 服务“{}”暴露的工具“{}”。{}",
                config.name,
                tool.name,
                tool.description
                    .as_deref()
                    .map(str::trim)
                    .filter(|value| !value.is_empty())
                    .unwrap_or("无额外描述")
            ),
            parameters: serde_json::to_value(&tool.input_schema).unwrap_or_else(|_| json!({})),
        });
        registry.handlers.insert(
            definition_name,
            McpChatToolHandler::RemoteTool {
                client: connection.peer.clone(),
                remote_name: tool.name.to_string(),
                server_name: config.name.clone(),
            },
        );
    }

    let resources = match tokio::time::timeout(MCP_CHAT_DISCOVERY_TIMEOUT, client.list_all_resources()).await {
        Ok(Ok(value)) => value,
        Ok(Err(err)) => {
            warnings.push(format!("MCP 服务“{}”资源列表读取失败：{}", config.name, err));
            Vec::new()
        }
        Err(_) => {
            warnings.push(format!("MCP 服务“{}”资源列表读取超时", config.name));
            Vec::new()
        }
    };

    if !resources.is_empty() {
        let definition_name = format!("{tool_prefix}_read_resource");
        registry.definitions.push(RigToolDefinition {
            name: definition_name.clone(),
            description: format!(
                "读取 MCP 服务“{}”中的资源内容。适用于已经知道资源 URI 的场景。",
                config.name
            ),
            parameters: json!({
                "type": "object",
                "properties": {
                    "uri": {
                        "type": "string",
                        "description": "MCP 资源 URI。"
                    }
                },
                "required": ["uri"],
                "additionalProperties": false
            }),
        });
        registry.handlers.insert(
            definition_name,
            McpChatToolHandler::ReadResource {
                client: connection.peer.clone(),
                server_name: config.name.clone(),
            },
        );
    }

    let prompts = match tokio::time::timeout(MCP_CHAT_DISCOVERY_TIMEOUT, client.list_all_prompts()).await {
        Ok(Ok(value)) => value,
        Ok(Err(err)) => {
            warnings.push(format!("MCP 服务“{}”提示词列表读取失败：{}", config.name, err));
            Vec::new()
        }
        Err(_) => {
            warnings.push(format!("MCP 服务“{}”提示词列表读取超时", config.name));
            Vec::new()
        }
    };

    if !prompts.is_empty() {
        let definition_name = format!("{tool_prefix}_get_prompt");
        registry.definitions.push(RigToolDefinition {
            name: definition_name.clone(),
            description: format!(
                "获取 MCP 服务“{}”中的 prompt 模板。arguments 可选，需传对象。",
                config.name
            ),
            parameters: json!({
                "type": "object",
                "properties": {
                    "name": {
                        "type": "string",
                        "description": "Prompt 名称。"
                    },
                    "arguments": {
                        "type": "object",
                        "description": "可选的 prompt 参数对象。"
                    }
                },
                "required": ["name"],
                "additionalProperties": false
            }),
        });
        registry.handlers.insert(
            definition_name,
            McpChatToolHandler::GetPrompt {
                client: connection.peer.clone(),
                server_name: config.name.clone(),
            },
        );
    }

    Ok(())
}

fn sanitize_tool_name_fragment(value: &str) -> String {
    let mut normalized = value
        .chars()
        .map(|ch| {
            if ch.is_ascii_alphanumeric() {
                ch.to_ascii_lowercase()
            } else {
                '_'
            }
        })
        .collect::<String>();
    normalized = normalized.trim_matches('_').to_string();
    if normalized.is_empty() {
        "unnamed".to_string()
    } else {
        normalized
    }
}

#[cfg(test)]
mod tests {
    use super::sanitize_tool_name_fragment;

    #[test]
    fn mcp_chat_sanitizes_tool_fragments() {
        assert_eq!(sanitize_tool_name_fragment("Search-Web"), "search_web");
        assert_eq!(sanitize_tool_name_fragment(""), "unnamed");
    }
}
