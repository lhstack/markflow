//! The OpenAI Responses API.
//!
//! By default when creating a completion client, this is the API that gets used.
//!
//! If you'd like to switch back to the regular Completions API, you can do so by using the `.completions_api()` function - see below for an example:
//! ```rust
//! use rig_core::client::{CompletionClient, ProviderClient};
//!
//! # fn example() -> Result<(), Box<dyn std::error::Error>> {
//! let openai_client = rig_core::providers::openai::Client::from_env()?;
//! let model = openai_client.completion_model("gpt-4o").completions_api();
//! # let _ = model;
//! # Ok(())
//! # }
//! ```
use super::InputAudio;
use super::completion::ToolChoice;
use super::responses_api::streaming::StreamingCompletionResponse;
use crate::completion::{CompletionError, GetTokenUsage};
use crate::http_client;
use crate::http_client::HttpClientExt;
use crate::json_utils;
use crate::message::{
    AudioMediaType, Document, DocumentMediaType, DocumentSourceKind, ImageDetail, MessageError,
    MimeType, Text,
};
use crate::one_or_many::string_or_one_or_many;

use crate::wasm_compat::{WasmCompatSend, WasmCompatSync};
use crate::{OneOrMany, completion, message};
use serde::{Deserialize, Deserializer, Serialize, Serializer};
use serde_json::{Map, Value};
use tracing::{Instrument, Level, enabled, info_span};

use std::convert::Infallible;
use std::ops::Add;
use std::str::FromStr;

pub mod streaming;
#[cfg(all(not(target_family = "wasm"), feature = "websocket"))]
pub mod websocket;

/// The completion request type for OpenAI's Response API: <https://platform.openai.com/docs/api-reference/responses/create>
/// Intended to be derived from [`crate::completion::request::CompletionRequest`].
#[derive(Debug, Deserialize, Clone)]
pub struct CompletionRequest {
    /// Message inputs
    pub input: Vec<InputItem>,
    /// The model name
    pub model: String,
    /// Instructions (also referred to as preamble, although in other APIs this would be the "system prompt")
    #[serde(default)]
    pub instructions: String,
    /// The maximum number of output tokens.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub max_output_tokens: Option<u64>,
    /// Toggle to true for streaming responses.
    pub stream: bool,
    /// The temperature. Set higher (up to a max of 1.0) for more creative responses.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub temperature: Option<f64>,
    /// Whether the LLM should be forced to use a tool before returning a response.
    /// If none provided, the default option is "auto".
    tool_choice: Option<ToolChoice>,
    /// The tools you want to use. This supports both function tools and hosted tools
    /// such as `web_search`, `file_search`, and `computer_use`.
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub tools: Vec<ResponsesToolDefinition>,
    /// Additional parameters
    #[serde(flatten)]
    pub additional_parameters: AdditionalParameters,
}

impl Serialize for CompletionRequest {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        let mut value = serde_json::to_value(&self.additional_parameters)
            .map_err(serde::ser::Error::custom)?;

        let map = value
            .as_object_mut()
            .ok_or_else(|| serde::ser::Error::custom("additional parameters must be an object"))?;
        for (key, value) in &self.additional_parameters.extra {
            if key == "thinking" {
                continue;
            }
            map.entry(key.clone()).or_insert_with(|| value.clone());
        }

        map.insert("model".to_string(), Value::String(self.model.clone()));
        if !self.instructions.trim().is_empty() {
            map.insert(
                "instructions".to_string(),
                Value::String(self.instructions.clone()),
            );
        }
        map.insert(
            "input".to_string(),
            serde_json::to_value(&self.input).map_err(serde::ser::Error::custom)?,
        );
        map.insert(
            "tools".to_string(),
            serde_json::to_value(&self.tools).map_err(serde::ser::Error::custom)?,
        );
        map.insert(
            "tool_choice".to_string(),
            serde_json::to_value(self.tool_choice.as_ref().unwrap_or(&ToolChoice::Auto))
                .map_err(serde::ser::Error::custom)?,
        );
        map.insert(
            "parallel_tool_calls".to_string(),
            Value::Bool(self.additional_parameters.parallel_tool_calls.unwrap_or(false)),
        );
        map.insert(
            "store".to_string(),
            Value::Bool(self.additional_parameters.store.unwrap_or(false)),
        );
        map.insert("stream".to_string(), Value::Bool(self.stream));
        map.entry("include".to_string())
            .or_insert_with(|| Value::Array(Vec::new()));
        value.serialize(serializer)
    }
}

impl CompletionRequest {
    pub fn with_structured_outputs<S>(mut self, schema_name: S, schema: serde_json::Value) -> Self
    where
        S: Into<String>,
    {
        self.additional_parameters.text = Some(TextConfig::structured_output(schema_name, schema));

        self
    }

    pub fn with_reasoning(mut self, reasoning: Reasoning) -> Self {
        self.additional_parameters.reasoning = Some(reasoning);

        self
    }

    /// Adds a provider-native hosted tool (e.g. `web_search`, `file_search`, `computer_use`)
    /// to the request. These tools are executed by OpenAI's infrastructure, not by Rig's
    /// agent loop.
    pub fn with_tool(mut self, tool: impl Into<ResponsesToolDefinition>) -> Self {
        self.tools.push(tool.into());
        self
    }

    /// Adds multiple provider-native hosted tools to the request. These tools are executed
    /// by OpenAI's infrastructure, not by Rig's agent loop.
    pub fn with_tools<I, Tool>(mut self, tools: I) -> Self
    where
        I: IntoIterator<Item = Tool>,
        Tool: Into<ResponsesToolDefinition>,
    {
        self.tools.extend(tools.into_iter().map(Into::into));
        self
    }
}

/// An input item for [`CompletionRequest`].
#[derive(Debug, Deserialize, Clone)]
pub struct InputItem {
    /// The role of an input item/message.
    /// Input messages should be Some(Role::User), and output messages should be Some(Role::Assistant).
    /// Everything else should be None.
    #[serde(skip_serializing_if = "Option::is_none")]
    role: Option<Role>,
    /// The input content itself.
    #[serde(flatten)]
    input: InputContent,
}

impl Serialize for InputItem {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        let mut value = serde_json::to_value(&self.input).map_err(serde::ser::Error::custom)?;
        let map = value.as_object_mut().ok_or_else(|| {
            serde::ser::Error::custom("Input content must serialize to an object")
        })?;

        if let Some(role) = &self.role
            && !map.contains_key("role")
        {
            map.insert(
                "role".to_string(),
                serde_json::to_value(role).map_err(serde::ser::Error::custom)?,
            );
        }
        if map.get("type").and_then(Value::as_str) == Some("message") {
            map.remove("id");
            map.remove("name");
            map.remove("status");
        }

        value.serialize(serializer)
    }
}

impl InputItem {
    pub fn system_message(content: impl Into<String>) -> Self {
        Self {
            role: Some(Role::System),
            input: InputContent::Message(Message::System {
                content: OneOrMany::one(SystemContent::InputText {
                    text: content.into(),
                }),
                name: None,
            }),
        }
    }

    pub(crate) fn system_text(&self) -> Option<String> {
        match &self.input {
            InputContent::Message(Message::System { content, .. }) => Some(
                content
                    .iter()
                    .map(|item| match item {
                        SystemContent::InputText { text } => text.as_str(),
                    })
                    .collect::<Vec<_>>()
                    .join("\n"),
            ),
            _ => None,
        }
    }
}

/// Message roles. Used by OpenAI Responses API to determine who created a given message.
#[derive(Debug, Deserialize, Serialize, Clone)]
#[serde(rename_all = "lowercase")]
pub enum Role {
    User,
    Assistant,
    System,
}

/// The type of content used in an [`InputItem`]. Additionally holds data for each type of input content.
#[derive(Debug, Deserialize, Serialize, Clone)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum InputContent {
    Message(Message),
    Reasoning(OpenAIReasoning),
    FunctionCall(OutputFunctionCall),
    FunctionCallOutput(ToolResult),
}

#[derive(Debug, Deserialize, Serialize, Clone, PartialEq)]
pub struct OpenAIReasoning {
    #[serde(skip_serializing)]
    id: String,
    pub summary: Vec<ReasoningSummary>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub encrypted_content: Option<String>,
    #[serde(skip_serializing)]
    pub status: Option<ToolStatus>,
}

#[derive(Debug, Deserialize, Serialize, Clone, PartialEq)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum ReasoningSummary {
    SummaryText { text: String },
}

impl ReasoningSummary {
    fn new(input: &str) -> Self {
        Self::SummaryText {
            text: input.to_string(),
        }
    }

    pub fn text(&self) -> String {
        let ReasoningSummary::SummaryText { text } = self;
        text.clone()
    }
}

/// A tool result.
#[derive(Debug, Deserialize, Serialize, Clone)]
pub struct ToolResult {
    /// The call ID of a tool (this should be linked to the call ID for a tool call, otherwise an error will be received)
    call_id: String,
    /// The result of a tool call.
    output: String,
    /// The status of a tool call (if used in a completion request, this should always be Completed)
    #[serde(skip_serializing_if = "Option::is_none")]
    status: Option<ToolStatus>,
}

impl From<Message> for InputItem {
    fn from(value: Message) -> Self {
        match value {
            Message::User { .. } => Self {
                role: Some(Role::User),
                input: InputContent::Message(value),
            },
            Message::Assistant { ref content, .. } => {
                let role = if content
                    .iter()
                    .any(|x| matches!(x, AssistantContentType::Reasoning(_)))
                {
                    None
                } else {
                    Some(Role::Assistant)
                };
                Self {
                    role,
                    input: InputContent::Message(value),
                }
            }
            Message::System { .. } => Self {
                role: Some(Role::System),
                input: InputContent::Message(value),
            },
            Message::ToolResult {
                tool_call_id,
                output,
            } => Self {
                role: None,
                input: InputContent::FunctionCallOutput(ToolResult {
                    call_id: tool_call_id,
                    output,
                    status: None,
                }),
            },
        }
    }
}

impl TryFrom<crate::completion::Message> for Vec<InputItem> {
    type Error = CompletionError;

    fn try_from(value: crate::completion::Message) -> Result<Self, Self::Error> {
        match value {
            crate::completion::Message::System { content } => Ok(vec![InputItem {
                role: Some(Role::System),
                input: InputContent::Message(Message::System {
                    content: OneOrMany::one(content.into()),
                    name: None,
                }),
            }]),
            crate::completion::Message::User { content } => {
                let mut items = Vec::new();

                for user_content in content {
                    match user_content {
                        crate::message::UserContent::Text(Text { text, .. }) => {
                            items.push(InputItem {
                                role: Some(Role::User),
                                input: InputContent::Message(Message::User {
                                    content: OneOrMany::one(UserContent::InputText { text }),
                                    name: None,
                                }),
                            });
                        }
                        crate::message::UserContent::ToolResult(
                            crate::completion::message::ToolResult {
                                call_id,
                                content: tool_content,
                                ..
                            },
                        ) => {
                            for tool_result_content in tool_content {
                                let crate::completion::message::ToolResultContent::Text(Text {
                                    text,
                                    ..
                                }) = tool_result_content
                                else {
                                    return Err(CompletionError::ProviderError(
                                        "This thing only supports text!".to_string(),
                                    ));
                                };
                                // let output = serde_json::from_str(&text)?;
                                items.push(InputItem {
                                    role: None,
                                    input: InputContent::FunctionCallOutput(ToolResult {
                                        call_id: require_call_id(call_id.clone(), "Tool result")?,
                                        output: text,
                                        status: None,
                                    }),
                                });
                            }
                        }
                        crate::message::UserContent::Document(Document {
                            data: DocumentSourceKind::FileId(file_id),
                            ..
                        }) => items.push(InputItem {
                            role: Some(Role::User),
                            input: InputContent::Message(Message::User {
                                content: OneOrMany::one(UserContent::InputFile {
                                    file_id: Some(file_id),
                                    file_data: None,
                                    file_url: None,
                                    filename: None,
                                }),
                                name: None,
                            }),
                        }),
                        crate::message::UserContent::Document(Document {
                            data,
                            media_type: Some(media_type),
                            additional_params,
                        }) => {
                            let (file_data, file_url) = match data {
                                DocumentSourceKind::Base64(data) => (Some(data), None),
                                DocumentSourceKind::Url(url) => (None, Some(url)),
                                DocumentSourceKind::Raw(_) => {
                                    return Err(CompletionError::RequestError(
                                        "Raw file data not supported, encode as base64 first"
                                            .into(),
                                    ));
                                }
                                doc => {
                                    return Err(CompletionError::RequestError(
                                        format!("Unsupported document type: {doc}").into(),
                                    ));
                                }
                            };

                            items.push(InputItem {
                                role: Some(Role::User),
                                input: InputContent::Message(Message::User {
                                    content: OneOrMany::one(UserContent::InputFile {
                                        file_id: None,
                                        file_data,
                                        file_url,
                                        filename: additional_params
                                            .as_ref()
                                            .and_then(|params| params.get("filename"))
                                            .and_then(Value::as_str)
                                            .map(str::to_string)
                                            .or_else(|| {
                                                Some(default_filename_for_document(&media_type))
                                            }),
                                    }),
                                    name: None,
                                }),
                            })
                        }
                        crate::message::UserContent::Document(Document {
                            data:
                                DocumentSourceKind::Base64(text) | DocumentSourceKind::String(text),
                            ..
                        }) => items.push(InputItem {
                            role: Some(Role::User),
                            input: InputContent::Message(Message::User {
                                content: OneOrMany::one(UserContent::InputText { text }),
                                name: None,
                            }),
                        }),
                        crate::message::UserContent::Image(crate::message::Image {
                            data,
                            media_type,
                            detail,
                            ..
                        }) => {
                            let url = match data {
                                DocumentSourceKind::Base64(data) => {
                                    let media_type = if let Some(media_type) = media_type {
                                        media_type.to_mime_type().to_string()
                                    } else {
                                        String::new()
                                    };
                                    format!("data:{media_type};base64,{data}")
                                }
                                DocumentSourceKind::Url(url) => url,
                                DocumentSourceKind::Raw(_) => {
                                    return Err(CompletionError::RequestError(
                                        "Raw file data not supported, encode as base64 first"
                                            .into(),
                                    ));
                                }
                                doc => {
                                    return Err(CompletionError::RequestError(
                                        format!("Unsupported document type: {doc}").into(),
                                    ));
                                }
                            };
                            items.push(InputItem {
                                role: Some(Role::User),
                                input: InputContent::Message(Message::User {
                                    content: OneOrMany::one(UserContent::InputImage {
                                        image_url: url,
                                        detail: detail.unwrap_or_default(),
                                    }),
                                    name: None,
                                }),
                            });
                        }
                        crate::message::UserContent::Audio(crate::message::Audio {
                            data: DocumentSourceKind::Base64(data),
                            media_type,
                            ..
                        }) => {
                            items.push(InputItem {
                                role: Some(Role::User),
                                input: InputContent::Message(Message::User {
                                    content: OneOrMany::one(UserContent::Audio {
                                        input_audio: InputAudio {
                                            data,
                                            format: media_type.unwrap_or(AudioMediaType::MP3),
                                        },
                                    }),
                                    name: None,
                                }),
                            });
                        }
                        crate::message::UserContent::Audio(_) => {
                            return Err(CompletionError::RequestError(
                                "Audio must be base64 encoded data".into(),
                            ));
                        }
                        crate::message::UserContent::Video(crate::message::Video {
                            data: DocumentSourceKind::Base64(data),
                            media_type,
                            ..
                        }) => {
                            items.push(InputItem {
                                role: Some(Role::User),
                                input: InputContent::Message(Message::User {
                                    content: OneOrMany::one(UserContent::Audio {
                                        input_audio: InputAudio {
                                            data,
                                            format: media_type
                                                .map(audio_media_type_for_video)
                                                .unwrap_or(AudioMediaType::MP4),
                                        },
                                    }),
                                    name: None,
                                }),
                            });
                        }
                        crate::message::UserContent::Video(_) => {
                            return Err(CompletionError::RequestError(
                                "Video must be base64 encoded data".into(),
                            ));
                        }
                        message => {
                            return Err(CompletionError::ProviderError(format!(
                                "Unsupported message: {message:?}"
                            )));
                        }
                    }
                }

                Ok(items)
            }
            crate::completion::Message::Assistant { id, content } => {
                let mut reasoning_items = Vec::new();
                let mut other_items = Vec::new();

                for assistant_content in content {
                    match assistant_content {
                        crate::message::AssistantContent::Text(Text { text, .. }) => {
                            let id = id.as_ref().unwrap_or(&String::default()).clone();
                            other_items.push(InputItem {
                                role: Some(Role::Assistant),
                                input: InputContent::Message(Message::Assistant {
                                    content: OneOrMany::one(AssistantContentType::Text(
                                        AssistantContent::OutputText(Text::new(text)),
                                    )),
                                    id,
                                    name: None,
                                    status: ToolStatus::Completed,
                                }),
                            });
                        }
                        crate::message::AssistantContent::ToolCall(crate::message::ToolCall {
                            id: tool_id,
                            call_id,
                            function,
                            ..
                        }) => {
                            other_items.push(InputItem {
                                role: None,
                                input: InputContent::FunctionCall(OutputFunctionCall {
                                    arguments: function.arguments,
                                    call_id: require_call_id(call_id, "Assistant tool call")?,
                                    id: tool_id,
                                    name: function.name,
                                    status: ToolStatus::Completed,
                                }),
                            });
                        }
                        crate::message::AssistantContent::Reasoning(reasoning) => {
                            let openai_reasoning = openai_reasoning_from_core(&reasoning)
                                .map_err(|err| CompletionError::ProviderError(err.to_string()))?;
                            reasoning_items.push(InputItem {
                                role: None,
                                input: InputContent::Reasoning(openai_reasoning),
                            });
                        }
                        crate::message::AssistantContent::Image(_) => {
                            return Err(CompletionError::ProviderError(
                                "Assistant image content is not supported in OpenAI Responses API"
                                    .to_string(),
                            ));
                        }
                    }
                }

                let mut items = reasoning_items;
                items.extend(other_items);
                Ok(items)
            }
        }
    }
}

impl From<OneOrMany<String>> for Vec<ReasoningSummary> {
    fn from(value: OneOrMany<String>) -> Self {
        value.iter().map(|x| ReasoningSummary::new(x)).collect()
    }
}

fn require_call_id(call_id: Option<String>, context: &str) -> Result<String, CompletionError> {
    call_id.ok_or_else(|| {
        CompletionError::RequestError(
            format!("{context} `call_id` is required for OpenAI Responses API").into(),
        )
    })
}

fn audio_media_type_for_video(media_type: message::VideoMediaType) -> AudioMediaType {
    match media_type {
        message::VideoMediaType::AVI => AudioMediaType::AVI,
        message::VideoMediaType::MP4 => AudioMediaType::MP4,
        message::VideoMediaType::MPEG => AudioMediaType::MPEG,
        message::VideoMediaType::MOV => AudioMediaType::MOV,
        message::VideoMediaType::WEBM => AudioMediaType::WEBM,
    }
}

fn default_filename_for_document(media_type: &DocumentMediaType) -> String {
    match media_type {
        DocumentMediaType::PDF => "document.pdf",
        DocumentMediaType::TXT => "document.txt",
        DocumentMediaType::RTF => "document.rtf",
        DocumentMediaType::HTML => "document.html",
        DocumentMediaType::CSS => "document.css",
        DocumentMediaType::MARKDOWN => "document.md",
        DocumentMediaType::CSV => "document.csv",
        DocumentMediaType::TSV => "document.tsv",
        DocumentMediaType::JSON => "document.json",
        DocumentMediaType::YAML => "document.yaml",
        DocumentMediaType::XML => "document.xml",
        DocumentMediaType::Javascript => "document.js",
        DocumentMediaType::Python => "document.py",
        DocumentMediaType::Typescript => "document.ts",
        DocumentMediaType::Excel => "spreadsheet.xlsx",
        DocumentMediaType::Word => "document.docx",
        DocumentMediaType::Presentation => "presentation.pptx",
        DocumentMediaType::OpenDocumentText => "document.odt",
        DocumentMediaType::Pages => "document.pages",
        DocumentMediaType::Keynote => "presentation.key",
        DocumentMediaType::Calendar => "calendar.ics",
        DocumentMediaType::Email => "message.eml",
        DocumentMediaType::VCard => "contact.vcf",
        DocumentMediaType::SRT => "captions.srt",
        DocumentMediaType::VTT => "captions.vtt",
        DocumentMediaType::TOML => "document.toml",
        DocumentMediaType::SQL => "query.sql",
        DocumentMediaType::Shell => "script.sh",
        DocumentMediaType::Rust => "main.rs",
        DocumentMediaType::Go => "main.go",
        DocumentMediaType::Java => "Main.java",
        DocumentMediaType::Php => "index.php",
        DocumentMediaType::Ruby => "script.rb",
        DocumentMediaType::C => "main.c",
        DocumentMediaType::Cpp => "main.cpp",
        DocumentMediaType::CSharp => "Program.cs",
        DocumentMediaType::Kotlin => "Main.kt",
        DocumentMediaType::Swift => "main.swift",
        DocumentMediaType::Lua => "script.lua",
        DocumentMediaType::R => "script.r",
        DocumentMediaType::Perl => "script.pl",
        DocumentMediaType::Diff => "changes.diff",
        DocumentMediaType::Protobuf => "schema.proto",
        DocumentMediaType::Graphql => "schema.graphql",
        DocumentMediaType::Ndjson => "data.ndjson",
    }
    .to_string()
}

fn openai_reasoning_from_core(
    reasoning: &crate::message::Reasoning,
) -> Result<OpenAIReasoning, MessageError> {
    let id = reasoning.id.clone().ok_or_else(|| {
        MessageError::ConversionError(
            "An OpenAI-generated ID is required when using OpenAI reasoning items".to_string(),
        )
    })?;
    let mut summary = Vec::new();
    let mut encrypted_content = None;
    for content in &reasoning.content {
        match content {
            crate::message::ReasoningContent::Text { text, .. }
            | crate::message::ReasoningContent::Summary(text) => {
                summary.push(ReasoningSummary::new(text));
            }
            // OpenAI reasoning input has one opaque payload field; preserve either
            // encrypted or redacted blocks there, preferring the first one seen.
            crate::message::ReasoningContent::Encrypted(data)
            | crate::message::ReasoningContent::Redacted { data } => {
                encrypted_content.get_or_insert_with(|| data.clone());
            }
        }
    }

    Ok(OpenAIReasoning {
        id,
        summary,
        encrypted_content,
        status: None,
    })
}

/// The definition of a tool response, repurposed for OpenAI's Responses API.
#[derive(Debug, Deserialize, Serialize, Clone, PartialEq)]
pub struct ResponsesToolDefinition {
    /// The type of tool.
    #[serde(rename = "type")]
    pub kind: String,
    /// Tool name
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub name: String,
    /// Parameters - this should be a JSON schema. Tools should additionally ensure an "additionalParameters" field has been added with the value set to false, as this is required if using OpenAI's strict mode (enabled by default).
    #[serde(default, skip_serializing_if = "is_json_null")]
    pub parameters: serde_json::Value,
    /// Whether to use strict mode. Kept explicit for compatibility with
    /// Responses-compatible gateways that distinguish an omitted value from
    /// `false`.
    #[serde(default)]
    pub strict: bool,
    /// Tool description.
    #[serde(
        default,
        deserialize_with = "default_string_on_null",
        skip_serializing_if = "String::is_empty"
    )]
    pub description: String,
    /// Additional provider-specific configuration for hosted tools.
    #[serde(flatten, default, skip_serializing_if = "Map::is_empty")]
    pub config: Map<String, Value>,
}

fn is_json_null(value: &Value) -> bool {
    value.is_null()
}

fn default_string_on_null<'de, D>(deserializer: D) -> Result<String, D::Error>
where
    D: Deserializer<'de>,
{
    Ok(Option::<String>::deserialize(deserializer)?.unwrap_or_default())
}

impl ResponsesToolDefinition {
    /// Creates a function tool definition.
    pub fn function(
        name: impl Into<String>,
        description: impl Into<String>,
        parameters: serde_json::Value,
    ) -> Self {
        Self {
            kind: "function".to_string(),
            name: name.into(),
            parameters,
            strict: false,
            description: description.into(),
            config: Map::new(),
        }
    }

    /// Creates a hosted tool definition for an arbitrary hosted tool type.
    pub fn hosted(kind: impl Into<String>) -> Self {
        Self {
            kind: kind.into(),
            name: String::new(),
            parameters: Value::Null,
            strict: false,
            description: String::new(),
            config: Map::new(),
        }
    }

    /// Creates a hosted `web_search` tool definition.
    pub fn web_search() -> Self {
        Self::hosted("web_search")
    }

    /// Creates a hosted `file_search` tool definition.
    pub fn file_search() -> Self {
        Self::hosted("file_search")
    }

    /// Creates a hosted `computer_use` tool definition.
    pub fn computer_use() -> Self {
        Self::hosted("computer_use")
    }

    /// Adds hosted-tool configuration fields.
    pub fn with_config(mut self, key: impl Into<String>, value: Value) -> Self {
        self.config.insert(key.into(), value);
        self
    }

    fn normalize(mut self) -> Self {
        if self.kind == "function" {
            self.strict = false;
        }
        self
    }
}

impl From<completion::ToolDefinition> for ResponsesToolDefinition {
    fn from(value: completion::ToolDefinition) -> Self {
        let completion::ToolDefinition {
            name,
            parameters,
            description,
        } = value;

        Self::function(name, description, parameters)
    }
}

/// Token usage.
/// Token usage from the OpenAI Responses API generally shows the input tokens and output tokens (both with more in-depth details) as well as a total tokens field.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct ResponsesUsage {
    /// Input tokens
    pub input_tokens: u64,
    /// In-depth detail on input tokens (cached tokens)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub input_tokens_details: Option<InputTokensDetails>,
    /// Output tokens
    pub output_tokens: u64,
    /// In-depth detail on output tokens (reasoning tokens)
    pub output_tokens_details: OutputTokensDetails,
    /// Total tokens used (for a given prompt)
    pub total_tokens: u64,
}

impl ResponsesUsage {
    /// Create a new ResponsesUsage instance
    pub(crate) fn new() -> Self {
        Self {
            input_tokens: 0,
            input_tokens_details: Some(InputTokensDetails::new()),
            output_tokens: 0,
            output_tokens_details: OutputTokensDetails::new(),
            total_tokens: 0,
        }
    }
}

impl GetTokenUsage for ResponsesUsage {
    fn token_usage(&self) -> Option<crate::completion::Usage> {
        Some(crate::completion::Usage {
            input_tokens: self.input_tokens,
            output_tokens: self.output_tokens,
            total_tokens: self.total_tokens,
            cached_input_tokens: self
                .input_tokens_details
                .as_ref()
                .map(|details| details.cached_tokens)
                .unwrap_or(0),
            cache_creation_input_tokens: 0,
            tool_use_prompt_tokens: 0,
            reasoning_tokens: self.output_tokens_details.reasoning_tokens,
        })
    }
}

impl Add for ResponsesUsage {
    type Output = Self;

    fn add(self, rhs: Self) -> Self::Output {
        let input_tokens = self.input_tokens + rhs.input_tokens;
        let input_tokens_details = self.input_tokens_details.map(|lhs| {
            if let Some(tokens) = rhs.input_tokens_details {
                lhs + tokens
            } else {
                lhs
            }
        });
        let output_tokens = self.output_tokens + rhs.output_tokens;
        let output_tokens_details = self.output_tokens_details + rhs.output_tokens_details;
        let total_tokens = self.total_tokens + rhs.total_tokens;
        Self {
            input_tokens,
            input_tokens_details,
            output_tokens,
            output_tokens_details,
            total_tokens,
        }
    }
}

/// In-depth details on input tokens.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct InputTokensDetails {
    /// Cached tokens from OpenAI
    pub cached_tokens: u64,
}

impl InputTokensDetails {
    pub(crate) fn new() -> Self {
        Self { cached_tokens: 0 }
    }
}

impl Add for InputTokensDetails {
    type Output = Self;
    fn add(self, rhs: Self) -> Self::Output {
        Self {
            cached_tokens: self.cached_tokens + rhs.cached_tokens,
        }
    }
}

/// In-depth details on output tokens.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct OutputTokensDetails {
    /// Reasoning tokens
    pub reasoning_tokens: u64,
}

impl OutputTokensDetails {
    pub(crate) fn new() -> Self {
        Self {
            reasoning_tokens: 0,
        }
    }
}

impl Add for OutputTokensDetails {
    type Output = Self;
    fn add(self, rhs: Self) -> Self::Output {
        Self {
            reasoning_tokens: self.reasoning_tokens + rhs.reasoning_tokens,
        }
    }
}

/// Occasionally, when using OpenAI's Responses API you may get an incomplete response. This struct holds the reason as to why it happened.
#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct IncompleteDetailsReason {
    /// The reason for an incomplete [`CompletionResponse`].
    pub reason: String,
}

/// A response error from OpenAI's Response API.
#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct ResponseError {
    /// Error code
    pub code: String,
    /// Error message
    pub message: String,
}

/// A response object as an enum (ensures type validation)
#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum ResponseObject {
    Response,
}

/// The response status as an enum (ensures type validation)
#[derive(Clone, Debug, Deserialize, Serialize, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum ResponseStatus {
    InProgress,
    Completed,
    Failed,
    Cancelled,
    Queued,
    Incomplete,
}

fn default_response_status() -> ResponseStatus {
    ResponseStatus::Completed
}

/// Attempt to try and create a `NewCompletionRequest` from a model name and [`crate::completion::CompletionRequest`]
impl TryFrom<(String, crate::completion::CompletionRequest)> for CompletionRequest {
    type Error = CompletionError;
    fn try_from(
        (model, mut req): (String, crate::completion::CompletionRequest),
    ) -> Result<Self, Self::Error> {
        let model = req.model.clone().unwrap_or(model);
        let use_instructions =
            std::env::var_os("RIG_OPENAI_RESPONSES_SYSTEM_IN_INPUT").is_none();
        let preamble = req.preamble.take();
        let mut instructions = if use_instructions {
            preamble.clone()
        } else {
            None
        };
        let input = {
            let mut partial_history = vec![];
            if let Some(docs) = req.normalized_documents() {
                partial_history.push(docs);
            }
            partial_history.extend(req.chat_history);

            // Initialize full history with preamble (or empty if non-existent)
            // Some "Responses API compatible" providers don't support `instructions` field
            // so we need to add a system message until further notice
            let mut full_history: Vec<InputItem> = match (use_instructions, preamble) {
                (false, Some(content)) => vec![InputItem::system_message(content)],
                _ => Vec::new(),
            };

            for history_item in partial_history {
                full_history.extend(<Vec<InputItem>>::try_from(history_item)?);
            }

            if use_instructions {
                let mut retained = Vec::with_capacity(full_history.len());
                let mut system_parts = Vec::new();
                for item in full_history {
                    if let Some(text) = item.system_text() {
                        if !text.trim().is_empty() {
                            system_parts.push(text);
                        }
                    } else {
                        retained.push(item);
                    }
                }
                if !system_parts.is_empty() {
                    let system_text = system_parts.join("\n\n");
                    instructions = Some(match instructions.take() {
                        Some(existing) if !existing.trim().is_empty() => {
                            format!("{existing}\n\n{system_text}")
                        }
                        _ => system_text,
                    });
                }
                full_history = retained;
            }

            full_history
        };

        if input.is_empty() {
            return Err(CompletionError::RequestError(
                "OpenAI Responses request input must contain at least one item".into(),
            ));
        }
        let input = input;

        let mut additional_params_payload = req.additional_params.take().unwrap_or(Value::Null);
        let stream = match &additional_params_payload {
            Value::Bool(stream) => Some(*stream),
            Value::Object(map) => map.get("stream").and_then(Value::as_bool),
            _ => None,
        };

        let mut additional_tools = Vec::new();
        let mut additional_tool_choice = None;
        if let Some(additional_params_map) = additional_params_payload.as_object_mut() {
            if let Some(raw_tools) = additional_params_map.remove("tools") {
                additional_tools = serde_json::from_value::<Vec<ResponsesToolDefinition>>(
                    raw_tools,
                )
                .map_err(|err| {
                    CompletionError::RequestError(
                        format!(
                            "Invalid OpenAI Responses tools payload in additional_params: {err}"
                        )
                        .into(),
                    )
                })?;
            }
            if let Some(raw_tool_choice) = additional_params_map.remove("tool_choice") {
                additional_tool_choice = Some(serde_json::from_value::<ToolChoice>(
                    raw_tool_choice,
                )
                .map_err(|err| {
                    CompletionError::RequestError(
                        format!(
                            "Invalid OpenAI Responses tool_choice payload in additional_params: {err}"
                        )
                        .into(),
                    )
                })?);
            }
            additional_params_map.remove("stream");
        }

        if additional_params_payload.is_boolean() {
            additional_params_payload = Value::Null;
        }

        additional_tools = additional_tools
            .into_iter()
            .map(ResponsesToolDefinition::normalize)
            .collect();

        let mut additional_parameters = if additional_params_payload.is_null() {
            // If there's no additional parameters, initialise an empty object
            AdditionalParameters::default()
        } else {
            serde_json::from_value::<AdditionalParameters>(additional_params_payload).map_err(
                |err| {
                    CompletionError::RequestError(
                        format!("Invalid OpenAI Responses additional_params payload: {err}").into(),
                    )
                },
            )?
        };
        if additional_parameters.reasoning.is_some() {
            let include = additional_parameters.include.get_or_insert_with(Vec::new);
            if !include
                .iter()
                .any(|item| matches!(item, Include::ReasoningEncryptedContent))
            {
                include.push(Include::ReasoningEncryptedContent);
            }
        }
        if additional_parameters.parallel_tool_calls.is_none() {
            additional_parameters.parallel_tool_calls = Some(false);
        }
        let top_p = additional_parameters.top_p.take();
        if let Some(max_output_tokens) = req.max_tokens {
            additional_parameters
                .extra
                .entry("max_output_tokens".to_string())
                .or_insert_with(|| Value::from(max_output_tokens));
        }
        if let Some(temperature) = req.temperature {
            additional_parameters
                .extra
                .entry("temperature".to_string())
                .or_insert_with(|| Value::from(temperature));
        }
        if let Some(top_p) = top_p {
            additional_parameters
                .extra
                .entry("top_p".to_string())
                .or_insert_with(|| Value::from(top_p));
        }
        // Apply output_schema as structured output if not already configured via additional_params
        if additional_parameters.text.is_none()
            && let Some(schema) = req.output_schema
        {
            let name = schema
                .as_object()
                .and_then(|o| o.get("title"))
                .and_then(|v| v.as_str())
                .unwrap_or("response_schema")
                .to_string();
            let mut schema_value = schema.to_value();
            super::sanitize_schema(&mut schema_value);
            additional_parameters.text = Some(TextConfig::structured_output(name, schema_value));
        }

        let tool_choice = additional_tool_choice
            .or(req.tool_choice.map(ToolChoice::try_from).transpose()?)
            .or(Some(ToolChoice::Auto));
        let mut tools: Vec<ResponsesToolDefinition> = req
            .tools
            .into_iter()
            .map(ResponsesToolDefinition::from)
            .map(ResponsesToolDefinition::normalize)
            .collect();
        tools.append(&mut additional_tools);

        Ok(Self {
            input,
            model,
            instructions: instructions.unwrap_or_default(),
            max_output_tokens: req.max_tokens,
            stream: stream.unwrap_or(false),
            tool_choice,
            tools,
            temperature: req.temperature,
            additional_parameters,
        })
    }
}

/// The completion model struct for OpenAI's response API.
#[doc(hidden)]
#[derive(Clone)]
pub struct GenericResponsesCompletionModel<Ext = super::OpenAIResponsesExt, H = reqwest::Client> {
    /// The OpenAI client
    pub(crate) client: crate::client::Client<Ext, H>,
    /// Name of the model (e.g.: gpt-3.5-turbo-1106)
    pub model: String,
    /// Model-level default tools that are always added to outgoing requests.
    pub tools: Vec<ResponsesToolDefinition>,
}

/// The completion model struct for OpenAI's Responses API.
///
/// This preserves the historical public generic shape where the first generic
/// parameter is the HTTP client type.
pub type ResponsesCompletionModel<H = reqwest::Client> =
    GenericResponsesCompletionModel<super::OpenAIResponsesExt, H>;

impl<Ext, H> GenericResponsesCompletionModel<Ext, H>
where
    crate::client::Client<Ext, H>: HttpClientExt + Clone + std::fmt::Debug + 'static,
    Ext: crate::client::Provider + Clone + 'static,
    H: Clone + Default + std::fmt::Debug + 'static,
{
    /// Creates a new [`ResponsesCompletionModel`].
    pub fn new(client: crate::client::Client<Ext, H>, model: impl Into<String>) -> Self {
        Self {
            client,
            model: model.into(),
            tools: Vec::new(),
        }
    }

    pub fn with_model(client: crate::client::Client<Ext, H>, model: &str) -> Self {
        Self {
            client,
            model: model.to_string(),
            tools: Vec::new(),
        }
    }

    /// Adds a default tool to all requests from this model.
    pub fn with_tool(mut self, tool: impl Into<ResponsesToolDefinition>) -> Self {
        self.tools.push(tool.into());
        self
    }

    /// Adds default tools to all requests from this model.
    pub fn with_tools<I, Tool>(mut self, tools: I) -> Self
    where
        I: IntoIterator<Item = Tool>,
        Tool: Into<ResponsesToolDefinition>,
    {
        self.tools.extend(tools.into_iter().map(Into::into));
        self
    }

    /// Attempt to create a completion request from [`crate::completion::CompletionRequest`].
    pub(crate) fn create_completion_request(
        &self,
        completion_request: crate::completion::CompletionRequest,
    ) -> Result<CompletionRequest, CompletionError> {
        let mut req = CompletionRequest::try_from((self.model.clone(), completion_request))?;
        req.tools.extend(self.tools.clone());

        Ok(req)
    }
}

impl<T> GenericResponsesCompletionModel<super::OpenAIResponsesExt, T>
where
    T: HttpClientExt + Clone + Default + std::fmt::Debug + 'static,
{
    /// Use the Completions API instead of Responses.
    pub fn completions_api(self) -> crate::providers::openai::completion::CompletionModel<T> {
        super::completion::CompletionModel::with_model(self.client.completions_api(), &self.model)
    }
}

/// The standard response format from OpenAI's Responses API.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct CompletionResponse {
    /// The ID of a completion response.
    pub id: String,
    /// The type of the object.
    pub object: ResponseObject,
    /// The time at which a given response has been created, in seconds from the UNIX epoch (01/01/1970 00:00:00).
    pub created_at: u64,
    /// The status of the response.
    #[serde(default = "default_response_status")]
    pub status: ResponseStatus,
    /// Response error (optional)
    pub error: Option<ResponseError>,
    /// Incomplete response details (optional)
    pub incomplete_details: Option<IncompleteDetailsReason>,
    /// System prompt/preamble
    pub instructions: Option<String>,
    /// The maximum number of tokens the model should output
    pub max_output_tokens: Option<u64>,
    /// The model name
    pub model: String,
    /// Token usage
    pub usage: Option<ResponsesUsage>,
    /// The model output (messages, etc will go here)
    #[serde(default)]
    pub output: Vec<Output>,
    /// Tools
    #[serde(default)]
    pub tools: Vec<ResponsesToolDefinition>,
    /// Additional parameters
    #[serde(flatten)]
    pub additional_parameters: AdditionalParameters,
}

/// Additional parameters for the completion request type for OpenAI's Response API: <https://platform.openai.com/docs/api-reference/responses/create>
/// Intended to be derived from [`crate::completion::request::CompletionRequest`].
#[derive(Clone, Debug, Deserialize, Serialize, Default)]
pub struct AdditionalParameters {
    /// Whether or not a given model task should run in the background (ie a detached process).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub background: Option<bool>,
    /// The text response format. This is where you would add structured outputs (if you want them).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub text: Option<TextConfig>,
    /// What types of extra data you would like to include. This is mostly useless at the moment since the types of extra data to add is currently unsupported, but this will be coming soon!
    #[serde(skip_serializing_if = "Option::is_none")]
    pub include: Option<Vec<Include>>,
    /// Legacy holder for `top_p` when supplied through `additional_params`.
    /// Request serialization moves it to [`CompletionRequest::top_p`].
    #[serde(skip_serializing_if = "Option::is_none")]
    pub top_p: Option<f64>,
    /// Whether or not the response should be truncated.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub truncation: Option<TruncationStrategy>,
    /// The username of the user (that you want to use).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub user: Option<String>,
    /// Any additional metadata you'd like to add. This will additionally be returned by the response.
    #[serde(skip_serializing_if = "Map::is_empty", default)]
    pub metadata: serde_json::Map<String, serde_json::Value>,
    /// Whether or not you want tool calls to run in parallel.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub parallel_tool_calls: Option<bool>,
    /// Previous response ID. If you are not sending a full conversation, this can help to track the message flow.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub previous_response_id: Option<String>,
    /// Add thinking/reasoning to your response. The response will be emitted as a list member of the `output` field.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub reasoning: Option<Reasoning>,
    /// The service tier you're using.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub service_tier: Option<OpenAIServiceTier>,
    /// Whether or not to store the response for later retrieval by API.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub store: Option<bool>,
    /// A stable cache key used by OpenAI/Codex Responses backends to improve prompt caching.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub prompt_cache_key: Option<String>,
    /// Retention policy for the prompt cache.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub prompt_cache_retention: Option<String>,
    /// Codex Responses API client metadata. OpenAI-compatible providers that do not
    /// understand this field can omit it from `additional_params`.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub client_metadata: Option<std::collections::HashMap<String, String>>,
    /// Provider-specific parameters not modeled by Rig yet.
    #[serde(flatten, default, skip_serializing)]
    pub extra: Map<String, Value>,
}

impl AdditionalParameters {
    pub fn to_json(self) -> serde_json::Value {
        serde_json::to_value(self).unwrap_or_else(|_| serde_json::Value::Object(Map::new()))
    }
}

/// The truncation strategy.
/// When using auto, if the context of this response and previous ones exceeds the model's context window size, the model will truncate the response to fit the context window by dropping input items in the middle of the conversation.
/// Otherwise, does nothing (and is disabled by default).
#[derive(Clone, Debug, Default, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TruncationStrategy {
    Auto,
    #[default]
    Disabled,
}

/// The model output format configuration.
/// You can either have plain text by default, or attach a JSON schema for the purposes of structured outputs.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct TextConfig {
    /// Optional GPT-5/Codex verbosity control.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub verbosity: Option<OpenAIVerbosity>,
    /// Optional output format. Omitted when only verbosity is configured.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub format: Option<TextFormat>,
}

impl TextConfig {
    pub(crate) fn structured_output<S>(name: S, schema: serde_json::Value) -> Self
    where
        S: Into<String>,
    {
        Self {
            verbosity: None,
            format: Some(TextFormat::JsonSchema(StructuredOutputsInput {
                name: name.into(),
                schema,
                strict: true,
            })),
        }
    }

    pub fn with_verbosity(mut self, verbosity: OpenAIVerbosity) -> Self {
        self.verbosity = Some(verbosity);
        self
    }
}

/// The text format (contained by [`TextConfig`]).
/// You can either have plain text by default, or attach a JSON schema for the purposes of structured outputs.
#[derive(Clone, Debug, Serialize, Deserialize, Default)]
#[serde(tag = "type")]
#[serde(rename_all = "snake_case")]
pub enum TextFormat {
    JsonSchema(StructuredOutputsInput),
    #[default]
    Text,
}

/// GPT-5/Codex text verbosity control.
#[derive(Clone, Debug, Default, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum OpenAIVerbosity {
    Low,
    #[default]
    Medium,
    High,
}

/// The inputs required for adding structured outputs.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct StructuredOutputsInput {
    /// The name of your schema.
    pub name: String,
    /// Your required output schema. It is recommended that you use the JsonSchema macro, which you can check out at <https://docs.rs/schemars/latest/schemars/trait.JsonSchema.html>.
    pub schema: serde_json::Value,
    /// Enable strict output. If you are using your AI agent in a data pipeline or another scenario that requires the data to be absolutely fixed to a given schema, it is recommended to set this to true.
    #[serde(default)]
    pub strict: bool,
}

/// Add reasoning to a [`CompletionRequest`].
#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct Reasoning {
    /// How much effort you want the model to put into thinking/reasoning.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub effort: Option<ReasoningEffort>,
    /// How much effort you want the model to put into writing the reasoning summary.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub summary: Option<ReasoningSummaryLevel>,
    /// Which conversation turns the server should consider for reasoning context.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub context: Option<ReasoningContext>,
}

impl Reasoning {
    /// Creates a new Reasoning instantiation (with empty values).
    pub fn new() -> Self {
        Self {
            effort: None,
            summary: None,
            context: None,
        }
    }

    /// Adds reasoning effort.
    pub fn with_effort(mut self, reasoning_effort: ReasoningEffort) -> Self {
        self.effort = Some(reasoning_effort);

        self
    }

    /// Adds summary level (how detailed the reasoning summary will be).
    pub fn with_summary_level(mut self, reasoning_summary_level: ReasoningSummaryLevel) -> Self {
        self.summary = Some(reasoning_summary_level);

        self
    }

    /// Adds reasoning context scope.
    pub fn with_context(mut self, reasoning_context: ReasoningContext) -> Self {
        self.context = Some(reasoning_context);

        self
    }
}

/// Reasoning context scope used by the Codex Responses API.
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum ReasoningContext {
    Auto,
    CurrentTurn,
    AllTurns,
}

/// The billing service tier that will be used. On auto by default.
#[derive(Clone, Debug, Default)]
pub enum OpenAIServiceTier {
    /// Let OpenAI choose the service tier.
    #[default]
    Auto,
    /// Use the default service tier.
    Default,
    /// Use the flex service tier.
    Flex,
    /// Use the priority service tier.
    Priority,
    /// Use the standard service tier returned by OpenAI-compatible providers.
    Standard,
    /// Preserve an unknown provider-specific service tier.
    Other(String),
}

impl Serialize for OpenAIServiceTier {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        serializer.serialize_str(match self {
            Self::Auto => "auto",
            Self::Default => "default",
            Self::Flex => "flex",
            Self::Priority => "priority",
            Self::Standard => "standard",
            Self::Other(value) => value,
        })
    }
}

impl<'de> Deserialize<'de> for OpenAIServiceTier {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        let value = String::deserialize(deserializer)?;
        Ok(match value.as_str() {
            "auto" => Self::Auto,
            "default" => Self::Default,
            "flex" => Self::Flex,
            "priority" => Self::Priority,
            "standard" => Self::Standard,
            _ => Self::Other(value),
        })
    }
}

/// The amount of reasoning effort that will be used by a given model.
#[derive(Clone, Debug, Default, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ReasoningEffort {
    None,
    Minimal,
    Low,
    #[default]
    Medium,
    High,
    Xhigh,
}

/// The amount of effort that will go into a reasoning summary by a given model.
#[derive(Clone, Debug, Default, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ReasoningSummaryLevel {
    #[default]
    Auto,
    Concise,
    Detailed,
}

/// Results to additionally include in the OpenAI Responses API.
/// Note that most of these are currently unsupported, but have been added for completeness.
#[derive(Clone, Debug)]
pub enum Include {
    FileSearchCallResults,
    MessageInputImageImageUrl,
    ComputerCallOutputOutputImageUrl,
    ReasoningEncryptedContent,
    CodeInterpreterCallOutputs,
    Other(String),
}

impl Include {
    fn as_str(&self) -> &str {
        match self {
            Self::FileSearchCallResults => "file_search_call.results",
            Self::MessageInputImageImageUrl => "message.input_image.image_url",
            Self::ComputerCallOutputOutputImageUrl => "computer_call.output.image_url",
            Self::ReasoningEncryptedContent => "reasoning.encrypted_content",
            Self::CodeInterpreterCallOutputs => "code_interpreter_call.outputs",
            Self::Other(value) => value,
        }
    }
}

impl Serialize for Include {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        serializer.serialize_str(self.as_str())
    }
}

impl<'de> Deserialize<'de> for Include {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        let value = String::deserialize(deserializer)?;
        Ok(match value.as_str() {
            "file_search_call.results" => Self::FileSearchCallResults,
            "message.input_image.image_url" => Self::MessageInputImageImageUrl,
            "computer_call.output.image_url" => Self::ComputerCallOutputOutputImageUrl,
            "reasoning.encrypted_content" => Self::ReasoningEncryptedContent,
            "code_interpreter_call.outputs" => Self::CodeInterpreterCallOutputs,
            _ => Self::Other(value),
        })
    }
}

/// A currently non-exhaustive list of output types.
#[derive(Clone, Debug, Deserialize, Serialize, PartialEq)]
#[serde(tag = "type")]
#[serde(rename_all = "snake_case")]
pub enum Output {
    Message(OutputMessage),
    #[serde(alias = "function_call")]
    FunctionCall(OutputFunctionCall),
    Reasoning {
        #[serde(default)]
        id: Option<String>,
        summary: Vec<ReasoningSummary>,
        #[serde(default)]
        encrypted_content: Option<String>,
        #[serde(default)]
        status: Option<ToolStatus>,
    },
    /// Catch-all variant for unknown output types (e.g., `web_search_call`,
    /// `file_search_call`, `computer_use_call`). This prevents unknown types
    /// from breaking deserialization of the entire `CompletionResponse`,
    /// which previously caused streaming token usage to be silently dropped.
    #[serde(other)]
    Unknown,
}

impl From<Output> for Vec<completion::AssistantContent> {
    fn from(value: Output) -> Self {
        let res: Vec<completion::AssistantContent> = match value {
            Output::Message(OutputMessage { content, .. }) => content
                .into_iter()
                .map(completion::AssistantContent::from)
                .collect(),
            Output::FunctionCall(OutputFunctionCall {
                id,
                arguments,
                call_id,
                name,
                ..
            }) => vec![completion::AssistantContent::tool_call_with_call_id(
                id, call_id, name, arguments,
            )],
            Output::Reasoning {
                id,
                summary,
                encrypted_content,
                ..
            } => {
                let mut content = summary
                    .into_iter()
                    .map(|summary| match summary {
                        ReasoningSummary::SummaryText { text } => {
                            message::ReasoningContent::Summary(text)
                        }
                    })
                    .collect::<Vec<_>>();
                if let Some(encrypted_content) = encrypted_content {
                    content.push(message::ReasoningContent::Encrypted(encrypted_content));
                }
                vec![completion::AssistantContent::Reasoning(
                    message::Reasoning {
                        id,
                        content,
                    },
                )]
            }
            Output::Unknown => Vec::new(),
        };

        res
    }
}

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq)]
pub struct OutputReasoning {
    id: String,
    summary: Vec<ReasoningSummary>,
    status: ToolStatus,
}

/// An OpenAI Responses API tool call. A call ID will be returned that must be used when creating a tool result to send back to OpenAI as a message input, otherwise an error will be received.
#[derive(Clone, Debug, Serialize, PartialEq)]
pub struct OutputFunctionCall {
    #[serde(skip_serializing)]
    pub id: String,
    #[serde(with = "json_utils::stringified_json")]
    pub arguments: serde_json::Value,
    pub call_id: String,
    pub name: String,
    #[serde(skip_serializing)]
    pub status: ToolStatus,
}

impl<'de> Deserialize<'de> for OutputFunctionCall {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        #[derive(Deserialize)]
        struct Helper {
            id: Option<String>,
            #[serde(with = "json_utils::stringified_json")]
            arguments: serde_json::Value,
            call_id: String,
            name: String,
            #[serde(default = "default_completed_tool_status")]
            status: ToolStatus,
        }

        let helper = Helper::deserialize(deserializer)?;
        Ok(Self {
            id: helper.id.unwrap_or_else(|| helper.call_id.clone()),
            arguments: helper.arguments,
            call_id: helper.call_id,
            name: helper.name,
            status: helper.status,
        })
    }
}

/// The status of a given tool.
#[derive(Clone, Debug, Deserialize, Serialize, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum ToolStatus {
    InProgress,
    Completed,
    Incomplete,
}

fn default_completed_tool_status() -> ToolStatus {
    ToolStatus::Completed
}

/// An output message from OpenAI's Responses API.
#[derive(Clone, Debug, Deserialize, Serialize, PartialEq)]
pub struct OutputMessage {
    /// The message ID. Must be included when sending the message back to OpenAI
    #[serde(default)]
    pub id: Option<String>,
    /// The role (currently only Assistant is available as this struct is only created when receiving an LLM message as a response)
    pub role: OutputRole,
    /// The status of the response
    #[serde(default)]
    pub status: Option<ResponseStatus>,
    /// The actual message content
    pub content: Vec<AssistantContent>,
}

/// The role of an output message.
#[derive(Clone, Debug, Deserialize, Serialize, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum OutputRole {
    Assistant,
}

impl<Ext, H> completion::CompletionModel for GenericResponsesCompletionModel<Ext, H>
where
    crate::client::Client<Ext, H>:
        HttpClientExt + Clone + WasmCompatSend + WasmCompatSync + 'static,
    Ext: crate::client::Provider
        + crate::client::DebugExt
        + Clone
        + WasmCompatSend
        + WasmCompatSync
        + 'static,
    H: Clone + Default + std::fmt::Debug + WasmCompatSend + WasmCompatSync + 'static,
{
    type Response = CompletionResponse;
    type StreamingResponse = StreamingCompletionResponse;

    type Client = crate::client::Client<Ext, H>;

    fn make(client: &Self::Client, model: impl Into<String>) -> Self {
        Self::new(client.clone(), model)
    }

    async fn completion(
        &self,
        completion_request: crate::completion::CompletionRequest,
    ) -> Result<completion::CompletionResponse<Self::Response>, CompletionError> {
        let span = if tracing::Span::current().is_disabled() {
            info_span!(
                target: "rig::completions",
                "chat",
                gen_ai.operation.name = "chat",
                gen_ai.provider.name = tracing::field::Empty,
                gen_ai.request.model = tracing::field::Empty,
                gen_ai.response.id = tracing::field::Empty,
                gen_ai.response.model = tracing::field::Empty,
                gen_ai.usage.output_tokens = tracing::field::Empty,
                gen_ai.usage.input_tokens = tracing::field::Empty,
                gen_ai.usage.cache_read.input_tokens = tracing::field::Empty,
                gen_ai.input.messages = tracing::field::Empty,
                gen_ai.output.messages = tracing::field::Empty,
            )
        } else {
            tracing::Span::current()
        };

        span.record("gen_ai.provider.name", "openai");
        span.record("gen_ai.request.model", &self.model);
        let request = self.create_completion_request(completion_request)?;
        let body = serde_json::to_vec(&request)?;

        let dump_id = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|duration| duration.as_millis())
            .unwrap_or_default();
        if std::env::var_os("RIG_DUMP_OPENAI_RESPONSES").is_some() {
            let _ = std::fs::write(
                format!("rig_openai_responses_request_{dump_id}.json"),
                serde_json::to_string_pretty(&request).unwrap_or_else(|_| String::new()),
            );
        }

        if enabled!(Level::TRACE) {
            tracing::trace!(
                target: "rig::completions",
                "OpenAI Responses completion request: {request}",
                request = serde_json::to_string_pretty(&request)?
            );
        }

        let req = self
            .client
            .post("/responses")?
            .body(body)
            .map_err(|e| CompletionError::HttpError(e.into()))?;

        async move {
            let response = self.client.send(req).await?;

            if response.status().is_success() {
                let t = http_client::text(response).await?;
                if std::env::var_os("RIG_DUMP_OPENAI_RESPONSES").is_some() {
                    let _ = std::fs::write("rig_openai_responses_raw.json", &t);
                }
                let response = serde_json::from_str::<Self::Response>(&t)?;
                let span = tracing::Span::current();
                span.record("gen_ai.response.id", &response.id);
                span.record("gen_ai.response.model", &response.model);
                if let Some(ref usage) = response.usage {
                    span.record("gen_ai.usage.output_tokens", usage.output_tokens);
                    span.record("gen_ai.usage.input_tokens", usage.input_tokens);
                    let cached_tokens = usage
                        .input_tokens_details
                        .as_ref()
                        .map(|d| d.cached_tokens)
                        .unwrap_or(0);
                    span.record("gen_ai.usage.cache_read.input_tokens", cached_tokens);
                }
                if enabled!(Level::TRACE) {
                    tracing::trace!(
                        target: "rig::completions",
                        "OpenAI Responses completion response: {response}",
                        response = serde_json::to_string_pretty(&response)?
                    );
                }
                response.try_into()
            } else {
                let text = http_client::text(response).await?;
                if std::env::var_os("RIG_DUMP_OPENAI_RESPONSES").is_some() {
                    let _ = std::fs::write(
                        format!("rig_openai_responses_error_{dump_id}.json"),
                        &text,
                    );
                }
                Err(CompletionError::ProviderError(text))
            }
        }
        .instrument(span)
        .await
    }

    async fn stream(
        &self,
        request: crate::completion::CompletionRequest,
    ) -> Result<
        crate::streaming::StreamingCompletionResponse<Self::StreamingResponse>,
        CompletionError,
    > {
        GenericResponsesCompletionModel::stream(self, request).await
    }
}

impl TryFrom<CompletionResponse> for completion::CompletionResponse<CompletionResponse> {
    type Error = CompletionError;

    fn try_from(response: CompletionResponse) -> Result<Self, Self::Error> {
        if response.output.is_empty() {
            return Err(CompletionError::ResponseError(
                "Response contained no parts".to_owned(),
            ));
        }

        // Extract the msg_ ID from the first Output::Message item
        let message_id = response.output.iter().find_map(|item| match item {
            Output::Message(msg) => msg.id.clone(),
            _ => None,
        });

        let content: Vec<completion::AssistantContent> = response
            .output
            .iter()
            .cloned()
            .flat_map(<Vec<completion::AssistantContent>>::from)
            .collect();

        let choice = OneOrMany::many(content).map_err(|_| {
            CompletionError::ResponseError(
                "Response contained no message or tool call (empty)".to_owned(),
            )
        })?;

        let usage = response
            .usage
            .as_ref()
            .and_then(GetTokenUsage::token_usage)
            .unwrap_or_default();

        Ok(completion::CompletionResponse {
            choice,
            usage,
            raw_response: response,
            message_id,
        })
    }
}

/// An OpenAI Responses API message.
#[derive(Debug, Serialize, Deserialize, PartialEq, Clone)]
#[serde(tag = "role", rename_all = "lowercase")]
pub enum Message {
    #[serde(alias = "developer")]
    System {
        #[serde(deserialize_with = "string_or_one_or_many")]
        content: OneOrMany<SystemContent>,
        #[serde(skip_serializing_if = "Option::is_none")]
        name: Option<String>,
    },
    User {
        #[serde(deserialize_with = "string_or_one_or_many")]
        content: OneOrMany<UserContent>,
        #[serde(skip_serializing_if = "Option::is_none")]
        name: Option<String>,
    },
    Assistant {
        content: OneOrMany<AssistantContentType>,
        #[serde(skip_serializing_if = "String::is_empty")]
        id: String,
        #[serde(skip_serializing_if = "Option::is_none")]
        name: Option<String>,
        status: ToolStatus,
    },
    #[serde(rename = "tool")]
    ToolResult {
        tool_call_id: String,
        output: String,
    },
}

/// The type of a tool result content item.
#[derive(Default, Debug, Serialize, Deserialize, PartialEq, Clone)]
#[serde(rename_all = "lowercase")]
pub enum ToolResultContentType {
    #[default]
    Text,
}

impl Message {
    pub fn system(content: &str) -> Self {
        Message::System {
            content: OneOrMany::one(content.to_owned().into()),
            name: None,
        }
    }
}

/// Text assistant content.
/// Note that the text type in comparison to the Completions API is actually `output_text` rather than `text`.
#[derive(Debug, Serialize, Deserialize, PartialEq, Clone)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum AssistantContent {
    OutputText(Text),
    Refusal { refusal: String },
}

impl From<AssistantContent> for completion::AssistantContent {
    fn from(value: AssistantContent) -> Self {
        match value {
            AssistantContent::Refusal { refusal } => {
                completion::AssistantContent::Text(Text::new(refusal))
            }
            AssistantContent::OutputText(Text { text, .. }) => {
                completion::AssistantContent::Text(Text::new(text))
            }
        }
    }
}

/// The type of assistant content.
#[derive(Debug, Serialize, Deserialize, PartialEq, Clone)]
#[serde(untagged)]
pub enum AssistantContentType {
    Text(AssistantContent),
    ToolCall(OutputFunctionCall),
    Reasoning(OpenAIReasoning),
}

/// System content for the OpenAI Responses API.
/// Uses `input_text` type to match the Responses API format.
#[derive(Debug, Serialize, Deserialize, PartialEq, Clone)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum SystemContent {
    InputText { text: String },
}

impl From<String> for SystemContent {
    fn from(s: String) -> Self {
        SystemContent::InputText { text: s }
    }
}

impl std::str::FromStr for SystemContent {
    type Err = std::convert::Infallible;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        Ok(SystemContent::InputText {
            text: s.to_string(),
        })
    }
}

/// Different types of user content.
#[derive(Debug, Serialize, Deserialize, PartialEq, Clone)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum UserContent {
    InputText {
        text: String,
    },
    InputImage {
        image_url: String,
        #[serde(default)]
        detail: ImageDetail,
    },
    InputFile {
        #[serde(skip_serializing_if = "Option::is_none")]
        file_id: Option<String>,
        #[serde(skip_serializing_if = "Option::is_none")]
        file_url: Option<String>,
        #[serde(skip_serializing_if = "Option::is_none")]
        file_data: Option<String>,
        #[serde(skip_serializing_if = "Option::is_none")]
        filename: Option<String>,
    },
    Audio {
        input_audio: InputAudio,
    },
    #[serde(rename = "tool")]
    ToolResult {
        tool_call_id: String,
        output: String,
    },
}

impl TryFrom<message::Message> for Vec<Message> {
    type Error = message::MessageError;

    fn try_from(message: message::Message) -> Result<Self, Self::Error> {
        match message {
            message::Message::System { content } => Ok(vec![Message::System {
                content: OneOrMany::one(content.into()),
                name: None,
            }]),
            message::Message::User { content } => {
                let (tool_results, other_content): (Vec<_>, Vec<_>) = content
                    .into_iter()
                    .partition(|content| matches!(content, message::UserContent::ToolResult(_)));

                // If there are messages with both tool results and user content, openai will only
                //  handle tool results. It's unlikely that there will be both.
                if !tool_results.is_empty() {
                    tool_results
                        .into_iter()
                        .map(|content| match content {
                            message::UserContent::ToolResult(message::ToolResult {
                                call_id,
                                content,
                                ..
                            }) => Ok::<_, message::MessageError>(Message::ToolResult {
                                tool_call_id: call_id.ok_or_else(|| {
                                    MessageError::ConversionError(
                                        "Tool result `call_id` is required for OpenAI Responses API"
                                            .into(),
                                    )
                                })?,
                                output: {
                                    let res = content.first();
                                    match res {
                                        completion::message::ToolResultContent::Text(Text {
                                            text,
                                            ..
                                        }) => text,
                                        _ => return  Err(MessageError::ConversionError("This API only currently supports text tool results".into()))
                                    }
                                },
                            }),
                            _ => Err(MessageError::ConversionError(
                                "expected tool result content while converting Responses API input"
                                    .into(),
                            )),
                        })
                        .collect::<Result<Vec<_>, _>>()
                } else {
                    let other_content = other_content
                        .into_iter()
                        .map(|content| match content {
                            message::UserContent::Text(message::Text { text, .. }) => {
                                Ok(UserContent::InputText { text })
                            }
                            message::UserContent::Image(message::Image {
                                data,
                                detail,
                                media_type,
                                ..
                            }) => {
                                let url = match data {
                                    DocumentSourceKind::Base64(data) => {
                                        let media_type = if let Some(media_type) = media_type {
                                            media_type.to_mime_type().to_string()
                                        } else {
                                            String::new()
                                        };
                                        format!("data:{media_type};base64,{data}")
                                    }
                                    DocumentSourceKind::Url(url) => url,
                                    DocumentSourceKind::Raw(_) => {
                                        return Err(MessageError::ConversionError(
                                            "Raw files not supported, encode as base64 first"
                                                .into(),
                                        ));
                                    }
                                    doc => {
                                        return Err(MessageError::ConversionError(format!(
                                            "Unsupported document type: {doc}"
                                        )));
                                    }
                                };

                                Ok(UserContent::InputImage {
                                    image_url: url,
                                    detail: detail.unwrap_or_default(),
                                })
                            }
                            message::UserContent::Document(message::Document {
                                data: DocumentSourceKind::FileId(file_id),
                                ..
                            }) => Ok(UserContent::InputFile {
                                file_id: Some(file_id),
                                file_url: None,
                                file_data: None,
                                filename: None,
                            }),
                            message::UserContent::Document(message::Document {
                                media_type: Some(media_type),
                                data,
                                additional_params,
                            }) => {
                                let (file_data, file_url, filename) = match data {
                                    DocumentSourceKind::Base64(data) => (Some(data), None, None),
                                    DocumentSourceKind::Url(url) => (None, Some(url), None),
                                    DocumentSourceKind::Raw(_) => {
                                        return Err(MessageError::ConversionError(
                                            "Raw files not supported, encode as base64 first"
                                                .into(),
                                        ));
                                    }
                                    doc => {
                                        return Err(MessageError::ConversionError(format!(
                                            "Unsupported document type: {doc}"
                                        )));
                                    }
                                };

                                Ok(UserContent::InputFile {
                                    file_id: None,
                                    file_url,
                                    file_data,
                                    filename: filename
                                        .or_else(|| {
                                            additional_params
                                                .as_ref()
                                                .and_then(|params| params.get("filename"))
                                                .and_then(Value::as_str)
                                                .map(str::to_string)
                                        })
                                        .or_else(|| Some(default_filename_for_document(&media_type))),
                                })
                            }
                            message::UserContent::Document(message::Document {
                                data: DocumentSourceKind::Base64(text),
                                ..
                            }) => Ok(UserContent::InputText { text }),
                            message::UserContent::Audio(message::Audio {
                                data: DocumentSourceKind::Base64(data),
                                media_type,
                                ..
                            }) => Ok(UserContent::Audio {
                                input_audio: InputAudio {
                                    data,
                                    format: match media_type {
                                        Some(media_type) => media_type,
                                        None => AudioMediaType::MP3,
                                    },
                                },
                            }),
                            message::UserContent::Audio(_) => Err(MessageError::ConversionError(
                                "Audio must be base64 encoded data".into(),
                            )),
                            message::UserContent::Video(message::Video {
                                data: DocumentSourceKind::Base64(data),
                                media_type,
                                ..
                            }) => Ok(UserContent::Audio {
                                input_audio: InputAudio {
                                    data,
                                    format: media_type
                                        .map(audio_media_type_for_video)
                                        .unwrap_or(AudioMediaType::MP4),
                                },
                            }),
                            message::UserContent::Video(_) => Err(MessageError::ConversionError(
                                "Video must be base64 encoded data".into(),
                            )),
                            _ => Err(MessageError::ConversionError(
                                "Unsupported user content for OpenAI Responses API".into(),
                            )),
                        })
                        .collect::<Result<Vec<_>, _>>()?;

                    let other_content = OneOrMany::many(other_content).map_err(|_| {
                        MessageError::ConversionError(
                            "User message did not contain OpenAI Responses-compatible content"
                                .to_string(),
                        )
                    })?;

                    Ok(vec![Message::User {
                        content: other_content,
                        name: None,
                    }])
                }
            }
            message::Message::Assistant { content, id } => {
                let assistant_message_id = id.ok_or_else(|| {
                    MessageError::ConversionError(
                        "Assistant message ID is required for OpenAI Responses API".into(),
                    )
                })?;

                match content.first() {
                    crate::message::AssistantContent::Text(Text { text, .. }) => {
                        Ok(vec![Message::Assistant {
                            id: assistant_message_id.clone(),
                            status: ToolStatus::Completed,
                            content: OneOrMany::one(AssistantContentType::Text(
                                AssistantContent::OutputText(Text::new(text)),
                            )),
                            name: None,
                        }])
                    }
                    crate::message::AssistantContent::ToolCall(crate::message::ToolCall {
                        id,
                        call_id,
                        function,
                        ..
                    }) => Ok(vec![Message::Assistant {
                        content: OneOrMany::one(AssistantContentType::ToolCall(
                            OutputFunctionCall {
                                call_id: call_id.ok_or_else(|| {
                                    MessageError::ConversionError(
                                        "Tool call `call_id` is required for OpenAI Responses API"
                                            .into(),
                                    )
                                })?,
                                arguments: function.arguments,
                                id,
                                name: function.name,
                                status: ToolStatus::Completed,
                            },
                        )),
                        id: assistant_message_id.clone(),
                        name: None,
                        status: ToolStatus::Completed,
                    }]),
                    crate::message::AssistantContent::Reasoning(reasoning) => {
                        let openai_reasoning = openai_reasoning_from_core(&reasoning)?;
                        Ok(vec![Message::Assistant {
                            content: OneOrMany::one(AssistantContentType::Reasoning(
                                openai_reasoning,
                            )),
                            id: assistant_message_id,
                            name: None,
                            status: ToolStatus::Completed,
                        }])
                    }
                    crate::message::AssistantContent::Image(_) => {
                        Err(MessageError::ConversionError(
                            "Assistant image content is not supported in OpenAI Responses API"
                                .into(),
                        ))
                    }
                }
            }
        }
    }
}

impl FromStr for UserContent {
    type Err = Infallible;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        Ok(UserContent::InputText {
            text: s.to_string(),
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::message;
    use serde_json::json;

    fn response_with_service_tier(service_tier: &str) -> Value {
        json!({
            "id": "resp_123",
            "object": "response",
            "created_at": 0,
            "status": "completed",
            "model": "gpt-5.4",
            "output": [],
            "service_tier": service_tier,
        })
    }

    #[test]
    fn completion_response_deserializes_standard_service_tier() {
        let response: CompletionResponse =
            serde_json::from_value(response_with_service_tier("standard"))
                .expect("response should deserialize");

        assert!(matches!(
            response.additional_parameters.service_tier,
            Some(OpenAIServiceTier::Standard)
        ));
    }

    #[test]
    fn completion_response_deserializes_priority_service_tier() {
        let response: CompletionResponse =
            serde_json::from_value(response_with_service_tier("priority"))
                .expect("response should deserialize");

        assert!(matches!(
            response.additional_parameters.service_tier,
            Some(OpenAIServiceTier::Priority)
        ));
    }

    #[test]
    fn completion_response_preserves_unknown_service_tier() {
        let response: CompletionResponse =
            serde_json::from_value(response_with_service_tier("provider_experimental"))
                .expect("response should deserialize");

        let Some(OpenAIServiceTier::Other(service_tier)) =
            response.additional_parameters.service_tier
        else {
            panic!("expected provider-specific service tier");
        };

        assert_eq!(service_tier, "provider_experimental");
    }

    #[test]
    fn service_tier_serializes_expected_strings() {
        let cases = [
            (OpenAIServiceTier::Auto, "auto"),
            (OpenAIServiceTier::Default, "default"),
            (OpenAIServiceTier::Flex, "flex"),
            (OpenAIServiceTier::Priority, "priority"),
            (OpenAIServiceTier::Standard, "standard"),
        ];

        for (service_tier, expected) in cases {
            assert_eq!(
                serde_json::to_value(service_tier).expect("service tier should serialize"),
                json!(expected)
            );
        }

        assert_eq!(
            serde_json::to_value(OpenAIServiceTier::Other(
                "provider_experimental".to_string()
            ))
            .expect("provider-specific service tier should serialize"),
            json!("provider_experimental")
        );
    }

    #[test]
    fn responses_usage_token_usage_preserves_reasoning_tokens() {
        let usage = ResponsesUsage {
            input_tokens: 100,
            input_tokens_details: Some(InputTokensDetails { cached_tokens: 25 }),
            output_tokens: 50,
            output_tokens_details: OutputTokensDetails {
                reasoning_tokens: 15,
            },
            total_tokens: 150,
        };

        let token_usage = usage.token_usage().expect("usage should be present");

        assert_eq!(token_usage.input_tokens, 100);
        assert_eq!(token_usage.cached_input_tokens, 25);
        assert_eq!(token_usage.output_tokens, 50);
        assert_eq!(token_usage.reasoning_tokens, 15);
        assert_eq!(token_usage.total_tokens, 150);
    }

    #[test]
    fn file_id_document_serializes_as_input_file_content() {
        let message = message::Message::User {
            content: OneOrMany::one(message::UserContent::Document(message::Document {
                data: DocumentSourceKind::FileId("file_abc".to_string()),
                media_type: None,
                additional_params: None,
            })),
        };

        let converted: Vec<Message> = message.try_into().expect("conversion should succeed");
        let Message::User { content, .. } = &converted[0] else {
            panic!("expected user message");
        };

        let json = serde_json::to_value(content.first_ref()).expect("serialize content");

        assert_eq!(json["type"], "input_file");
        assert_eq!(json["file_id"], "file_abc");
        assert!(json.get("file_data").is_none());
        assert!(json.get("file_url").is_none());
    }

    #[test]
    fn file_id_document_serializes_as_input_item_content() {
        let message = completion::Message::User {
            content: OneOrMany::one(message::UserContent::Document(message::Document {
                data: DocumentSourceKind::FileId("file_abc".to_string()),
                media_type: None,
                additional_params: None,
            })),
        };

        let converted: Vec<InputItem> = message.try_into().expect("conversion should succeed");
        let json = serde_json::to_value(&converted[0]).expect("serialize input item");

        assert_eq!(json["type"], "message");
        assert_eq!(json["role"], "user");
        assert_eq!(json["content"][0]["type"], "input_file");
        assert_eq!(json["content"][0]["file_id"], "file_abc");
        assert!(json["content"][0].get("file_data").is_none());
        assert!(json["content"][0].get("file_url").is_none());
    }

    #[test]
    fn additional_parameters_accept_codex_responses_fields() {
        let value = json!({
            "reasoning": {
                "effort": "minimal",
                "summary": "auto",
                "context": "all_turns"
            },
            "include": [
                "reasoning.encrypted_content",
                "provider.experimental"
            ],
            "text": {
                "verbosity": "high"
            },
            "prompt_cache_key": "cache-key",
            "client_metadata": {
                "source": "awake-claw"
            },
            "prompt_cache_retention": "24h",
            "safety_identifier": "user-123",
            "provider_experiment": {
                "enabled": true
            },
            "parallel_tool_calls": false,
            "store": false
        });

        let params: AdditionalParameters =
            serde_json::from_value(value).expect("codex fields should deserialize");
        let serialized = serde_json::to_value(&params).expect("codex fields should serialize");

        assert_eq!(serialized["reasoning"]["context"], "all_turns");
        assert_eq!(serialized["text"]["verbosity"], "high");
        assert_eq!(serialized["prompt_cache_key"], "cache-key");
        assert_eq!(serialized["prompt_cache_retention"], "24h");
        assert_eq!(serialized["client_metadata"]["source"], "awake-claw");
        assert!(serialized.get("safety_identifier").is_none());
        assert!(serialized.get("provider_experiment").is_none());
        assert_eq!(params.extra["safety_identifier"], "user-123");
        assert_eq!(params.extra["provider_experiment"]["enabled"], true);
        assert_eq!(
            serialized["include"],
            json!(["reasoning.encrypted_content", "provider.experimental"])
        );
    }

    #[test]
    fn completion_request_serializes_codex_api_key_responses_shape() {
        let request = CompletionRequest {
            model: "gpt-5.4".to_string(),
            instructions: "be precise".to_string(),
            input: vec![InputItem {
                role: Some(Role::User),
                input: InputContent::Message(Message::User {
                    content: OneOrMany::one(UserContent::InputText {
                        text: "hello".to_string(),
                    }),
                    name: None,
                }),
            }],
            max_output_tokens: Some(100),
            stream: true,
            temperature: Some(0.7),
            tool_choice: Some(ToolChoice::Auto),
            tools: Vec::new(),
            additional_parameters: serde_json::from_value(json!({
                "reasoning": {
                    "effort": "low",
                    "summary": "auto",
                    "context": "all_turns"
                },
                "include": ["reasoning.encrypted_content"],
                "text": { "verbosity": "high" },
                "parallel_tool_calls": false,
                "store": false,
                "prompt_cache_key": "thread-key",
                "prompt_cache_retention": "24h",
                "client_metadata": { "source": "awake-claw" },
                "safety_identifier": "user-123",
                "provider_experiment": { "enabled": true },
                "top_p": 0.9,
                "background": true,
                "metadata": { "legacy": true },
                "user": "legacy-user"
            }))
            .expect("additional params should parse"),
        };

        let value = serde_json::to_value(request).expect("request should serialize");

        assert_eq!(value["model"], "gpt-5.4");
        assert_eq!(value["instructions"], "be precise");
        assert_eq!(value["input"][0]["type"], "message");
        assert_eq!(value["input"][0]["role"], "user");
        assert_eq!(value["input"][0]["content"][0]["type"], "input_text");
        assert_eq!(value["tools"], json!([]));
        assert_eq!(value["tool_choice"], "auto");
        assert_eq!(value["parallel_tool_calls"], false);
        assert_eq!(value["store"], false);
        assert_eq!(value["stream"], true);
        assert_eq!(value["reasoning"]["context"], "all_turns");
        assert_eq!(value["text"]["verbosity"], "high");
        assert_eq!(value["prompt_cache_key"], "thread-key");
        assert_eq!(value["prompt_cache_retention"], "24h");
        assert_eq!(value["client_metadata"]["source"], "awake-claw");
        assert_eq!(value["safety_identifier"], "user-123");
        assert_eq!(value["provider_experiment"]["enabled"], true);

        assert_eq!(value["top_p"], 0.9);
        assert_eq!(value["background"], true);
        assert_eq!(value["metadata"]["legacy"], true);
        assert_eq!(value["user"], "legacy-user");
    }

    #[test]
    fn completion_request_keeps_thinking_out_of_responses_additional_params() {
        let request = CompletionRequest {
            model: "gpt-5.4".to_string(),
            instructions: String::new(),
            input: vec![InputItem {
                role: Some(Role::User),
                input: InputContent::Message(Message::User {
                    content: OneOrMany::one(UserContent::InputText {
                        text: "hello".to_string(),
                    }),
                    name: None,
                }),
            }],
            max_output_tokens: Some(20),
            stream: false,
            temperature: Some(0.5),
            tool_choice: Some(ToolChoice::Auto),
            tools: Vec::new(),
            additional_parameters: serde_json::from_value(json!({
                "thinking": { "type": "enabled" },
                "metadata": { "legacy": true }
            }))
            .expect("additional params should parse"),
        };

        let value = serde_json::to_value(request).expect("request should serialize");

        assert_eq!(value["metadata"]["legacy"], true);
        assert!(value.get("thinking").is_none());
    }

    #[test]
    fn completion_request_injects_responses_runtime_params_into_extra() {
        let request = crate::completion::CompletionRequest {
            model: None,
            preamble: None,
            chat_history: OneOrMany::one(completion::Message::User {
                content: OneOrMany::one(message::UserContent::Text(Text::from("hello"))),
            }),
            documents: Vec::new(),
            tools: Vec::new(),
            temperature: Some(0.5),
            max_tokens: Some(20),
            tool_choice: Some(crate::message::ToolChoice::Auto),
            additional_params: Some(json!({ "top_p": 0.3 })),
            output_schema: None,
        };

        let request = CompletionRequest::try_from(("gpt-5.4".to_string(), request))
            .expect("responses request should convert");
        let value = serde_json::to_value(request).expect("request should serialize");

        assert_eq!(value["max_output_tokens"], 20);
        assert_eq!(value["temperature"], 0.5);
        assert_eq!(value["top_p"], 0.3);
    }

    #[test]
    fn completion_request_replays_tool_history_as_codex_response_items() {
        let request = CompletionRequest {
            model: "gpt-5.4".to_string(),
            instructions: String::new(),
            input: vec![
                InputItem {
                    role: None,
                    input: InputContent::FunctionCall(OutputFunctionCall {
                        id: "fc_internal".to_string(),
                        arguments: json!({"timezone": "Asia/Shanghai"}),
                        call_id: "call_123".to_string(),
                        name: "get_time".to_string(),
                        status: ToolStatus::Completed,
                    }),
                },
                InputItem {
                    role: None,
                    input: InputContent::FunctionCallOutput(ToolResult {
                        call_id: "call_123".to_string(),
                        output: "2026-06-15 23:14:47".to_string(),
                        status: None,
                    }),
                },
            ],
            max_output_tokens: None,
            stream: true,
            temperature: None,
            tool_choice: Some(ToolChoice::Auto),
            tools: Vec::new(),
            additional_parameters: AdditionalParameters::default(),
        };

        let value = serde_json::to_value(request).expect("request should serialize");
        let call = &value["input"][0];
        assert_eq!(call["type"], "function_call");
        assert_eq!(call["name"], "get_time");
        assert_eq!(call["call_id"], "call_123");
        assert_eq!(call["arguments"], "{\"timezone\":\"Asia/Shanghai\"}");
        assert!(call.get("id").is_none(), "Codex request items omit id");
        assert!(call.get("status").is_none(), "Codex request items omit status");

        let output = &value["input"][1];
        assert_eq!(output["type"], "function_call_output");
        assert_eq!(output["call_id"], "call_123");
        assert_eq!(output["output"], "2026-06-15 23:14:47");
        assert!(output.get("status").is_none(), "Codex tool outputs omit status");
    }
}
