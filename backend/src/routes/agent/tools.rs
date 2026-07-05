//! 前端工具 + MCP 工具的 rig 适配层。
//!
//! 设计要点（B 方案）：
//! - 结构化的前端能力（页面状态、导航、项目/文档树、结构化编辑、保存等）作为
//!   真正的 rig 工具接入：每个包装成一个实现 `ToolDyn` 的适配器，`call()` 内部
//!   通过 SSE 下发 `tool.request`，再经 `/agent/tool-callback` 等待浏览器回传结果。
//! - MCP 工具同样包装成 `ToolDyn`，但在后端本地执行（`McpChatToolRegistry::execute`）。
//! - 文档编写、追加、替换统一通过 execute_browser_javascript 调用 markflow 方法完成，
//!   不再使用文本流写入协议。

use std::{convert::Infallible, sync::Arc, time::Duration};

use axum::response::sse::Event;
use rig::{
    completion::ToolDefinition as RigToolDefinition,
    tool::{ToolDyn, ToolError},
    wasm_compat::WasmBoxedFuture,
};
use serde_json::{json, Value};
use tokio::sync::mpsc;

use crate::mcp_chat::McpChatToolRegistry;

use super::broker::{frontend_tool_broker, PendingFrontendToolCall};
use super::send_json_event;
use super::web_fetch::WebFetchRuntimeConfig;

/// 前端工具适配器：`call()` 通过 broker 往返，等待浏览器执行并回传结果。
pub struct FrontendTool {
    definition: RigToolDefinition,
    run_id: String,
    tx: mpsc::Sender<Result<Event, Infallible>>,
}

impl FrontendTool {
    fn new(
        definition: RigToolDefinition,
        run_id: String,
        tx: mpsc::Sender<Result<Event, Infallible>>,
    ) -> Self {
        Self {
            definition,
            run_id,
            tx,
        }
    }
}

impl ToolDyn for FrontendTool {
    fn name(&self) -> String {
        self.definition.name.clone()
    }

    fn definition(&self, _prompt: String) -> WasmBoxedFuture<'_, RigToolDefinition> {
        let definition = self.definition.clone();
        Box::pin(async move { definition })
    }

    fn call<'a>(&'a self, args: String) -> WasmBoxedFuture<'a, Result<String, ToolError>> {
        Box::pin(async move {
            let arguments: Value = serde_json::from_str(&args).unwrap_or_else(|_| json!({}));
            let call_id = format!("call_{}", uuid::Uuid::new_v4().simple());
            let call = PendingFrontendToolCall {
                id: call_id.clone(),
                call_id,
                name: self.definition.name.clone(),
                arguments,
            };
            let output = request_frontend_tool_output(&self.tx, &self.run_id, &call)
                .await
                .unwrap_or_else(|error| {
                    json!({
                        "ok": false,
                        "tool": self.definition.name,
                        "error": error,
                    })
                });
            Ok(output.to_string())
        })
    }
}

/// MCP 工具适配器：`call()` 在后端本地执行。
pub struct McpChatTool {
    definition: RigToolDefinition,
    registry: Arc<McpChatToolRegistry>,
    tx: mpsc::Sender<Result<Event, Infallible>>,
}

impl ToolDyn for McpChatTool {
    fn name(&self) -> String {
        self.definition.name.clone()
    }

    fn definition(&self, _prompt: String) -> WasmBoxedFuture<'_, RigToolDefinition> {
        let definition = self.definition.clone();
        Box::pin(async move { definition })
    }

    fn call<'a>(&'a self, args: String) -> WasmBoxedFuture<'a, Result<String, ToolError>> {
        Box::pin(async move {
            let arguments: Value = serde_json::from_str(&args).unwrap_or_else(|_| json!({}));
            let output = self
                .registry
                .execute(self.definition.name.as_str(), &arguments)
                .await
                .unwrap_or_else(|| {
                    json!({
                        "ok": false,
                        "tool": self.definition.name,
                        "error": "MCP 工具未找到或未注册",
                    })
                });
            let status = if output.get("ok").and_then(Value::as_bool).unwrap_or(false) {
                "completed"
            } else {
                "failed"
            };
            let _ = send_json_event(
                &self.tx,
                "tool_event",
                json!({
                    "tool": self.definition.name,
                    "status": status,
                    "source": "mcp",
                    "arguments": arguments.clone(),
                    "output": output.clone(),
                }),
            )
            .await;
            Ok(output.to_string())
        })
    }
}

/// 组装本轮对话可用的全部工具（前端工具 + MCP 工具）为 rig 的 `Box<dyn ToolDyn>` 列表。
pub fn build_agent_tools(
    run_id: &str,
    tx: &mpsc::Sender<Result<Event, Infallible>>,
    mcp_registry: Arc<McpChatToolRegistry>,
    skills_root: Option<std::path::PathBuf>,
    web_fetch_config: WebFetchRuntimeConfig,
) -> Vec<Box<dyn ToolDyn>> {
    let mut tools: Vec<Box<dyn ToolDyn>> = agent_function_tools()
        .into_iter()
        .map(|definition| {
            Box::new(FrontendTool::new(definition, run_id.to_string(), tx.clone())) as Box<dyn ToolDyn>
        })
        .collect();

    // 后端本地 HTTP 工具。
    tools.extend(super::web_fetch::build_web_fetch_tools(web_fetch_config, tx));

    // Skills 工具（后端本地执行，读技能目录文件系统）。
    tools.extend(super::skills::build_skills_tools(skills_root, tx));

    for definition in mcp_registry.definitions() {
        tools.push(Box::new(McpChatTool {
            definition: definition.clone(),
            registry: Arc::clone(&mcp_registry),
            tx: tx.clone(),
        }) as Box<dyn ToolDyn>);
    }

    tools
}

/// 通过 SSE 下发 `tool.request` 并等待前端经 `/agent/tool-callback` 回传结果。
pub async fn request_frontend_tool_output(
    tx: &mpsc::Sender<Result<Event, Infallible>>,
    run_id: &str,
    call: &PendingFrontendToolCall,
) -> Result<Value, String> {
    let receiver = frontend_tool_broker().register(run_id, &call.call_id).await;

    if !send_json_event(
        tx,
        "tool.request",
        json!({
            "run_id": run_id,
            "call_id": call.call_id,
            "name": call.name,
            "arguments": call.arguments,
        }),
    )
    .await
    {
        return Err("前端工具请求通道已关闭".to_string());
    }

    let _ = send_json_event(
        tx,
        "tool_event",
        json!({
            "tool": call.name,
            "status": "requested",
            "summary": format!("正在请求前端工具 {}", call.name),
            "call_id": call.call_id,
            "arguments": call.arguments,
        }),
    )
    .await;

    let raw_output = match tokio::time::timeout(Duration::from_secs(120), receiver).await {
        Ok(Ok(output)) => output,
        Ok(Err(_)) => return Err(format!("前端工具 {} 的结果通道已中断", call.name)),
        Err(_) => return Err(format!("等待前端工具 {} 超时", call.name)),
    };

    let summary = summarize_frontend_tool_output(call, &raw_output);
    let output = with_frontend_tool_summary(raw_output, &summary);
    let _ = send_json_event(
        tx,
        "tool_event",
        json!({
            "tool": call.name,
            "status": if output.get("ok").and_then(Value::as_bool) == Some(true) {
                "completed"
            } else {
                "failed"
            },
            "summary": summary,
            "call_id": call.call_id,
            "arguments": call.arguments,
            "output": output,
        }),
    )
    .await;

    Ok(output)
}

fn summarize_frontend_tool_output(call: &PendingFrontendToolCall, output: &Value) -> String {
    let ok = output.get("ok").and_then(Value::as_bool);
    match ok {
        Some(true) => format!("前端工具 {} 执行完成", call.name),
        Some(false) => format!("前端工具 {} 返回错误", call.name),
        None => format!("前端工具 {} 已返回结果", call.name),
    }
}

fn with_frontend_tool_summary(mut output: Value, summary: &str) -> Value {
    if summary.trim().is_empty() {
        return output;
    }
    if let Some(object) = output.as_object_mut() {
        object
            .entry("summary".to_string())
            .or_insert_with(|| Value::String(summary.to_string()));
    }
    output
}

fn function_tool(
    name: &str,
    description: impl Into<String>,
    parameters: serde_json::Value,
) -> RigToolDefinition {
    RigToolDefinition {
        name: name.to_string(),
        description: description.into(),
        parameters,
    }
}

fn agent_function_tools() -> Vec<RigToolDefinition> {
    vec![
        function_tool(
            "execute_browser_javascript",
            "Run JavaScript in the current browser page and return success, result, console stdout/stderr, and duration. This is the only execution tool for interacting with the live MarkFlow workspace UI. If timeout_secs is omitted, default is 30 seconds, hard limit 300 seconds. Only use it when you must inspect or operate the live browser/page state. The code runs inside an async function and can directly use window, document, location, history, navigator, localStorage, sessionStorage, console, editor, and markflow. Use markflow.appendCurrentDocumentContent for drafting/appending document body, and markflow.replaceCurrentDocumentContent for full document replacement.",
            json!({
                "type": "object",
                "properties": {
                    "code": {
                        "type": "string",
                        "description": "JavaScript code to execute in the current browser page. It runs inside an async function body. Use explicit return to send a serializable result back."
                    },
                    "timeout_secs": {
                        "type": "integer",
                        "description": "Optional timeout seconds. Default 30, hard limit 300."
                    }
                },
                "required": ["code"],
                "additionalProperties": false,
            }),
        ),
    ]
}
