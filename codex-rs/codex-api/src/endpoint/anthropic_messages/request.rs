use super::THINKING_PREFIX;
use crate::endpoint::chat_completions::request::ToolKind;
use crate::endpoint::chat_completions::request::ToolNames;
use crate::endpoint::chat_completions::request::add_tools;
use crate::endpoint::chat_completions::request::tool_name;
use crate::error::ApiError;
use serde_json::Value;
use serde_json::json;
use std::collections::HashSet;

fn invalid(message: impl Into<String>) -> ApiError {
    ApiError::InvalidRequest {
        message: message.into(),
    }
}

fn content(parts: &Value) -> Result<Vec<Value>, ApiError> {
    if let Some(text) = parts.as_str() {
        return Ok(vec![json!({"type":"text","text":text})]);
    }
    parts
        .as_array()
        .ok_or_else(|| invalid("missing Claude message content"))?
        .iter()
        .filter_map(|part| match part["type"].as_str() {
            Some("input_text" | "output_text") => part["text"]
                .as_str()
                .filter(|text| !text.is_empty())
                .map(|text| Ok(json!({"type":"text","text":text}))),
            Some("input_image") => Some(
                part["image_url"]
                    .as_str()
                    .ok_or_else(|| invalid("missing image URL"))
                    .and_then(|url| {
                        let source = if let Some(data) = url.strip_prefix("data:") {
                            let (media_type, data) = data
                                .split_once(";base64,")
                                .ok_or_else(|| invalid("expected a base64 image data URL"))?;
                            json!({"type":"base64","media_type":media_type,"data":data})
                        } else {
                            json!({"type":"url","url":url})
                        };
                        Ok(json!({"type":"image","source":source}))
                    }),
            ),
            kind => Some(Err(invalid(format!(
                "unsupported Claude content type {kind:?}"
            )))),
        })
        .collect()
}

#[derive(serde::Serialize)]
struct Message {
    role: &'static str,
    content: Vec<Value>,
}

fn append(messages: &mut Vec<Message>, role: &'static str, blocks: Vec<Value>) {
    if blocks.is_empty() {
        return;
    }
    if let Some(last) = messages.last_mut().filter(|last| last.role == role) {
        last.content.extend(blocks);
    } else {
        messages.push(Message {
            role,
            content: blocks,
        });
    }
}

pub(super) fn translate(request: Value) -> Result<(Value, ToolNames), ApiError> {
    let model = request["model"]
        .as_str()
        .ok_or_else(|| invalid("missing Claude model"))?;
    let effort = request["reasoning"]["effort"].as_str();
    let adaptive = effort.is_some() && !model.starts_with("claude-opus-4-5");
    let mut names = ToolNames::new();
    let mut tools = Vec::new();
    if let Some(specs) = request["tools"].as_array() {
        add_tools(specs, /*namespace*/ None, &mut tools, &mut names)?;
    }
    let mut messages = Vec::new();
    let mut system = Vec::new();
    if let Some(text) = request["instructions"]
        .as_str()
        .filter(|text| !text.is_empty())
    {
        system.push(json!({"type":"text","text":text}));
    }
    for item in request["input"]
        .as_array()
        .ok_or_else(|| invalid("missing request input"))?
    {
        match item["type"].as_str() {
            Some("message") => {
                let parts = content(&item["content"])?;
                match item["role"].as_str() {
                    Some("system" | "developer") => system.extend(parts),
                    Some("assistant") => append(&mut messages, "assistant", parts),
                    Some("user") => append(&mut messages, "user", parts),
                    role => {
                        return Err(invalid(format!("unsupported Claude message role {role:?}")));
                    }
                }
            }
            Some("agent_message") => append(&mut messages, "user", content(&item["content"])?),
            Some("reasoning") => {
                if adaptive
                    && let Some(encoded) = item["encrypted_content"]
                        .as_str()
                        .and_then(|text| text.strip_prefix(THINKING_PREFIX))
                    && let Ok(block) = serde_json::from_str::<Value>(encoded)
                    && matches!(
                        block["type"].as_str(),
                        Some("thinking" | "redacted_thinking")
                    )
                {
                    append(&mut messages, "assistant", vec![block]);
                }
            }
            Some("function_call" | "custom_tool_call" | "tool_search_call") => {
                let kind = item["type"].as_str().unwrap_or_default();
                let name = if kind == "tool_search_call" {
                    "tool_search"
                } else {
                    item["name"]
                        .as_str()
                        .ok_or_else(|| invalid("missing tool call name"))?
                };
                let wire = tool_name(&mut names, item["namespace"].as_str(), name)?;
                let input = match kind {
                    "custom_tool_call"
                        if name == "apply_patch"
                            && names
                                .get(&wire)
                                .is_some_and(|mapped| mapped.kind == ToolKind::Function) =>
                    {
                        json!({"patch":item["input"]})
                    }
                    "custom_tool_call" => json!({"input":item["input"]}),
                    "tool_search_call" => item["arguments"].clone(),
                    _ => {
                        let arguments = item["arguments"].as_str().unwrap_or_default();
                        serde_json::from_str(arguments).unwrap_or_else(|_| json!({}))
                    }
                };
                append(
                    &mut messages,
                    "assistant",
                    vec![json!({
                        "type":"tool_use","id":item["call_id"],"name":wire,"input":input
                    })],
                );
            }
            Some("function_call_output" | "custom_tool_call_output" | "tool_search_output") => {
                let output = if item["type"] == "tool_search_output" {
                    let specs = item["tools"]
                        .as_array()
                        .ok_or_else(|| invalid("missing discovered tools"))?;
                    add_tools(specs, /*namespace*/ None, &mut tools, &mut names)?;
                    json!(item["tools"].to_string())
                } else if item["output"].is_string() {
                    item["output"].clone()
                } else {
                    json!(content(&item["output"])?)
                };
                let block = if let Some(id) = item["call_id"].as_str() {
                    json!({"type":"tool_result","tool_use_id":id,"content":output})
                } else {
                    json!({"type":"text","text":format!("Tool result from {}: {output}", item["name"])})
                };
                append(&mut messages, "user", vec![block]);
            }
            Some("additional_tools") => add_tools(
                item["tools"]
                    .as_array()
                    .ok_or_else(|| invalid("missing additional tools"))?,
                /*namespace*/ None,
                &mut tools,
                &mut names,
            )?,
            Some("web_search_call") => append(
                &mut messages,
                "assistant",
                vec![json!({
                    "type":"text","text":format!("Previous web search: {}", item["action"])
                })],
            ),
            kind => return Err(invalid(format!("unsupported Claude history item {kind:?}"))),
        }
    }
    // Tool results have to precede any other content in the next user message.
    for message in &mut messages {
        if message.role == "user" {
            message
                .content
                .sort_by_key(|block| block["type"] != "tool_result");
        }
    }
    let mut body = json!({
        "model": model,
        "messages": messages,
        "max_tokens": 32768,
        "stream": true,
        "cache_control": {"type": "ephemeral"}
    });
    if !system.is_empty() {
        body["system"] = system.into();
    }
    let mut seen = HashSet::new();
    let tools: Vec<_> = tools
        .into_iter()
        .filter_map(|tool| {
            let function = &tool["function"];
            let name = function["name"].as_str()?;
            seen.insert(name.to_owned()).then(|| {
                let mut tool = json!({"name":name,"input_schema":function["parameters"]});
                if let Some(description) = function["description"].as_str() {
                    tool["description"] = description.into();
                }
                tool
            })
        })
        .collect();
    if !tools.is_empty() {
        body["tools"] = tools.into();
        body["tool_choice"] = json!({
            "type":"auto",
            "disable_parallel_tool_use":request["parallel_tool_calls"] == false
        });
    }
    if let Some(effort) = effort {
        body["output_config"]["effort"] = effort.into();
        if adaptive {
            body["thinking"] = json!({
                "type":"adaptive","display":"summarized",
                "block_binding":{"prefix_mismatch_behavior":"drop_block"}
            });
        }
    }
    if let Some(schema) = request["text"]["format"].get("schema") {
        body["output_config"]["format"] = json!({"type":"json_schema","schema":schema});
    }
    Ok((body, names))
}
