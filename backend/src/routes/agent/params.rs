//! 模型运行参数 → provider additional_params 拼装。
//!
//! 从模型级 [`AgentModelConfig`] 生成透传给上游的 additional_params：
//! - OpenAI：区分 responses / completions 两套推理参数形状，处理 top_p/top_k、
//!   停止序列、thinking/reasoning、prompt_cache_key 等。
//! - Anthropic：thinking budget、top_p/top_k、stop_sequences。
//! 用户在 config.additional_params 中的自定义字段始终优先合并覆盖。

use serde_json::{json, Map, Value};

use super::provider::AgentModelConfig;
use super::AgentChatStreamRequest;

/// 合并两个 JSON 对象：extra 覆盖 base。非对象时 extra 优先。
fn merge_json_values(base: Option<Value>, extra: Option<Value>) -> Option<Value> {
    match (base, extra) {
        (Some(Value::Object(mut left)), Some(Value::Object(right))) => {
            for (key, value) in right {
                left.insert(key, value);
            }
            Some(Value::Object(left))
        }
        (Some(base), Some(extra)) => Some(match (base, extra) {
            (Value::Null, value) => value,
            (value, Value::Null) => value,
            (_, value) => value,
        }),
        (Some(value), None) | (None, Some(value)) => Some(value),
        (None, None) => None,
    }
}

fn append_unique_string_array_value(params: &mut Map<String, Value>, key: &str, value: &str) {
    let entry = params
        .entry(key.to_string())
        .or_insert_with(|| Value::Array(Vec::new()));
    if let Some(items) = entry.as_array_mut() {
        if !items.iter().any(|item| item.as_str() == Some(value)) {
            items.push(Value::String(value.to_string()));
        }
    }
}

fn responses_prompt_cache_key(provider_id: i64, model: &str) -> String {
    format!("markflow:responses:provider:{}:model:{}", provider_id, model.trim())
}

fn completions_prompt_cache_key(provider_id: i64, model: &str) -> String {
    format!("markflow:completions:provider:{}:model:{}", provider_id, model.trim())
}

/// OpenAI additional_params 拼装。`use_responses` 区分 responses / chat/completions。
pub fn build_openai_additional_params(
    payload: &AgentChatStreamRequest,
    config: &AgentModelConfig,
    use_responses: bool,
) -> Option<Value> {
    let mut params = Map::new();

    if let Some(top_p) = config.top_p {
        params.insert("top_p".to_string(), json!(top_p));
    }
    if !use_responses {
        if let Some(top_k) = config.top_k {
            params.insert("top_k".to_string(), json!(top_k));
        }
    }
    if !config.stop_sequences.is_empty() {
        params.insert("stop".to_string(), json!(config.stop_sequences));
    }

    let reasoning_enabled = config.reasoning_enabled.unwrap_or(false);
    params.insert(
        "thinking".to_string(),
        json!({ "type": if reasoning_enabled { "enabled" } else { "disabled" } }),
    );

    if reasoning_enabled {
        let effort = config.reasoning_effort.as_deref().unwrap_or("medium");
        let summary = config.reasoning_summary.as_deref().unwrap_or("detailed");
        if use_responses {
            params
                .entry("reasoning".to_string())
                .or_insert_with(|| json!({ "effort": effort, "summary": summary }));
            append_unique_string_array_value(&mut params, "include", "reasoning.encrypted_content");
        } else if effort != "none" {
            params.insert("reasoning_effort".to_string(), json!(effort));
            params.insert("reasoning_summary".to_string(), json!(summary));
        }
    }

    if use_responses {
        params.entry("tool_choice".to_string()).or_insert(json!("auto"));
        params.entry("parallel_tool_calls".to_string()).or_insert(json!(false));
        params.entry("store".to_string()).or_insert(json!(false));
        params.entry("prompt_cache_key".to_string()).or_insert_with(|| {
            json!(responses_prompt_cache_key(payload.provider.provider_id, &payload.provider.model))
        });
        if let Some(previous_response_id) = payload
            .previous_response_id
            .as_deref()
            .map(str::trim)
            .filter(|value| !value.is_empty())
        {
            params.insert(
                "previous_response_id".to_string(),
                Value::String(previous_response_id.to_string()),
            );
        }
    } else {
        params.entry("prompt_cache_key".to_string()).or_insert_with(|| {
            json!(completions_prompt_cache_key(payload.provider.provider_id, &payload.provider.model))
        });
    }

    merge_json_values(
        if params.is_empty() { None } else { Some(Value::Object(params)) },
        config.additional_params.clone(),
    )
}

/// Anthropic additional_params 拼装。
pub fn build_anthropic_additional_params(config: &AgentModelConfig) -> Option<Value> {
    let mut params = Map::new();

    if let Some(top_p) = config.top_p {
        params.insert("top_p".to_string(), json!(top_p));
    }
    if let Some(top_k) = config.top_k {
        params.insert("top_k".to_string(), json!(top_k));
    }
    if !config.stop_sequences.is_empty() {
        params.insert("stop_sequences".to_string(), json!(config.stop_sequences));
    }

    let reasoning_enabled = config.reasoning_enabled.unwrap_or(false);
    if reasoning_enabled {
        let budget = config.thinking_budget_tokens.unwrap_or(1024);
        params.insert(
            "thinking".to_string(),
            json!({ "type": "enabled", "budget_tokens": budget }),
        );
    } else {
        params.insert("thinking".to_string(), json!({ "type": "disabled" }));
    }

    merge_json_values(Some(Value::Object(params)), config.additional_params.clone())
}
