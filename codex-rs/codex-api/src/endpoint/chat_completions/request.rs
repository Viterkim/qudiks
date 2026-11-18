use crate::error::ApiError;
use serde_json::Value;
use serde_json::json;
use std::collections::BTreeMap;
use std::hash::DefaultHasher;
use std::hash::Hash;
use std::hash::Hasher;

#[derive(Clone, PartialEq)]
pub(super) struct ToolName {
    pub namespace: Option<String>,
    pub name: String,
    pub kind: ToolKind,
}

#[derive(Clone, PartialEq)]
pub(super) enum ToolKind {
    Function,
    Custom,
    Search,
}

pub(super) type ToolNames = BTreeMap<String, ToolName>;

fn invalid(message: impl Into<String>) -> ApiError {
    ApiError::InvalidRequest {
        message: message.into(),
    }
}

fn tool_name(
    names: &mut ToolNames,
    namespace: Option<&str>,
    name: &str,
) -> Result<String, ApiError> {
    let wire = if let Some(namespace) = namespace {
        let mut hash = DefaultHasher::new();
        (namespace, name).hash(&mut hash);
        let readable: String = format!("{namespace}_{name}")
            .chars()
            .map(|ch| {
                if ch.is_ascii_alphanumeric() || ch == '_' || ch == '-' {
                    ch
                } else {
                    '_'
                }
            })
            .take(40)
            .collect();
        format!("{readable}_{:016x}", hash.finish())
    } else {
        name.to_string()
    };
    let original = ToolName {
        namespace: namespace.map(str::to_string),
        name: name.to_string(),
        kind: ToolKind::Function,
    };
    if let Some(previous) = names.get(&wire) {
        if previous.namespace != original.namespace || previous.name != original.name {
            return Err(invalid(format!(
                "Chat Completions tool name collision: {wire}"
            )));
        }
    } else {
        names.insert(wire.clone(), original);
    }
    Ok(wire)
}

fn add_tools(
    tools: &[Value],
    namespace: Option<&str>,
    result: &mut Vec<Value>,
    names: &mut ToolNames,
) -> Result<(), ApiError> {
    for tool in tools {
        match tool["type"].as_str() {
            Some("namespace") => add_tools(
                tool["tools"]
                    .as_array()
                    .ok_or_else(|| invalid("missing namespace tools"))?,
                tool["name"].as_str(),
                result,
                names,
            )?,
            Some("function") => {
                let mut function = tool.clone();
                function
                    .as_object_mut()
                    .ok_or_else(|| invalid("invalid function tool"))?
                    .remove("type");
                function["name"] = tool_name(
                    names,
                    namespace,
                    tool["name"]
                        .as_str()
                        .ok_or_else(|| invalid("missing tool name"))?,
                )?
                .into();
                function
                    .as_object_mut()
                    .ok_or_else(|| invalid("invalid function tool"))?
                    .remove("defer_loading");
                result.push(json!({"type":"function", "function":function}));
            }
            Some("custom") => {
                let name = tool["name"]
                    .as_str()
                    .ok_or_else(|| invalid("missing custom tool name"))?;
                let wire = tool_name(names, namespace, name)?;
                if let Some(mapped) = names.get_mut(&wire) {
                    mapped.kind = ToolKind::Custom;
                }
                result.push(json!({"type":"function","function":{
                    "name":wire,"description":tool["description"],"parameters":{
                        "type":"object","properties":{"input":{"type":"string","description":"The complete input for this tool."}},
                        "required":["input"],"additionalProperties":false
                    }
                }}));
            }
            Some("tool_search") => {
                let wire = tool_name(names, namespace, "tool_search")?;
                if let Some(mapped) = names.get_mut(&wire) {
                    mapped.kind = ToolKind::Search;
                }
                result.push(json!({"type":"function","function":{
                    "name":wire,"description":tool["description"],"parameters":tool["parameters"]
                }}));
            }
            kind => {
                return Err(invalid(format!(
                    "Chat Completions does not support tool type {kind:?}"
                )));
            }
        }
    }
    Ok(())
}

fn content(parts: &Value) -> Result<Value, ApiError> {
    let Some(parts) = parts.as_array() else {
        return Ok(parts.clone());
    };
    let mut result = Vec::new();
    for part in parts {
        result.push(match part["type"].as_str() {
            Some("input_text" | "output_text") => json!({"type":"text", "text":part["text"]}),
            Some("input_image") if part["image_url"].is_string() => {
                let mut image = json!({"url":part["image_url"]});
                if !part["detail"].is_null() {
                    image["detail"] = part["detail"].clone();
                }
                json!({"type":"image_url", "image_url":image})
            }
            kind => {
                return Err(invalid(format!(
                    "unsupported Chat Completions content type {kind:?}"
                )));
            }
        });
    }
    Ok(Value::Array(result))
}

fn assistant(messages: &mut Vec<Value>) -> &mut Value {
    let append = messages
        .last()
        .is_none_or(|last| last["role"] != "assistant");
    if append {
        messages.push(json!({"role":"assistant", "content":null}));
    }
    let index = messages.len() - 1;
    &mut messages[index]
}

pub(super) fn translate(request: Value) -> Result<(Value, ToolNames), ApiError> {
    let kimi = request["model"]
        .as_str()
        .is_some_and(|model| model.starts_with("kimi-"));
    let mut names = ToolNames::new();
    let mut tools = Vec::new();
    if let Some(specs) = request["tools"].as_array() {
        add_tools(specs, None, &mut tools, &mut names)?;
    }
    let mut messages = Vec::new();
    if let Some(instructions) = request["instructions"].as_str()
        && !instructions.is_empty()
    {
        messages.push(json!({"role":"system", "content":instructions}));
    }
    let mut pending_images = Vec::new();
    for item in request["input"]
        .as_array()
        .ok_or_else(|| invalid("missing request input"))?
    {
        if !matches!(
            item["type"].as_str(),
            Some("function_call_output" | "custom_tool_call_output")
        ) && !pending_images.is_empty()
        {
            messages.push(json!({"role":"user","content":std::mem::take(&mut pending_images)}));
        }
        match item["type"].as_str() {
            Some("message") => {
                let role = item["role"]
                    .as_str()
                    .ok_or_else(|| invalid("missing message role"))?;
                let role = if role == "developer" { "system" } else { role };
                let parts = content(&item["content"])?;
                if role == "assistant" {
                    let message = assistant(&mut messages);
                    if let Some(previous) = message["content"].as_array_mut() {
                        previous.extend(parts.as_array().cloned().unwrap_or_default());
                    } else {
                        message["content"] = parts;
                    }
                } else {
                    messages.push(json!({"role":role,"content":parts}));
                }
            }
            Some("reasoning") => {
                let text: String = item["content"]
                    .as_array()
                    .into_iter()
                    .flatten()
                    .filter_map(|part| part["text"].as_str())
                    .collect();
                if !text.is_empty() {
                    let message = assistant(&mut messages);
                    let previous = message["reasoning_content"].as_str().unwrap_or("");
                    message["reasoning_content"] = format!("{previous}{text}").into();
                }
            }
            Some("function_call" | "custom_tool_call") => {
                let name = item["name"]
                    .as_str()
                    .ok_or_else(|| invalid("missing call name"))?;
                let wire = tool_name(&mut names, item["namespace"].as_str(), name)?;
                let arguments = if item["type"] == "custom_tool_call" {
                    // Compaction replays history without active tool definitions.
                    if name == "apply_patch"
                        && names
                            .get(&wire)
                            .is_some_and(|mapped| mapped.kind == ToolKind::Function)
                    {
                        json!({"patch": item["input"]}).to_string()
                    } else {
                        json!({"input": item["input"]}).to_string()
                    }
                } else {
                    item["arguments"].as_str().unwrap_or("").to_string()
                };
                let call = json!({"id":item["call_id"],"type":"function","function":{"name":wire,"arguments":arguments}});
                let message = assistant(&mut messages);
                if message["tool_calls"].is_null() {
                    message["tool_calls"] = json!([]);
                }
                if let Some(calls) = message["tool_calls"].as_array_mut() {
                    calls.push(call);
                }
            }
            Some("function_call_output" | "custom_tool_call_output") => {
                if !item["call_id"].is_string() {
                    return Err(invalid("tool output missing call_id"));
                }
                let parts = content(&item["output"])?;
                if let Some(parts) = parts.as_array() {
                    let (text, images): (Vec<_>, Vec<_>) = parts
                        .iter()
                        .cloned()
                        .partition(|part| part["type"] == "text");
                    let text = text
                        .iter()
                        .filter_map(|part| part["text"].as_str())
                        .collect::<Vec<_>>()
                        .join("\n");
                    messages
                        .push(json!({"role":"tool","tool_call_id":item["call_id"],"content":text}));
                    pending_images.extend(images);
                } else {
                    messages.push(
                        json!({"role":"tool","tool_call_id":item["call_id"],"content":parts}),
                    );
                }
            }
            Some("tool_search_call") => {
                let wire = tool_name(&mut names, None, "tool_search")?;
                let message = assistant(&mut messages);
                if message["tool_calls"].is_null() {
                    message["tool_calls"] = json!([]);
                }
                if let Some(calls) = message["tool_calls"].as_array_mut() {
                    calls.push(json!({"id":item["call_id"],"type":"function","function":{"name":wire,"arguments":item["arguments"].to_string()}}));
                }
            }
            Some("tool_search_output") => {
                let specs = item["tools"]
                    .as_array()
                    .ok_or_else(|| invalid("missing discovered tools"))?;
                add_tools(specs, None, &mut tools, &mut names)?;
                messages.push(json!({"role":"tool","tool_call_id":item["call_id"],"content":item["tools"].to_string()}));
            }
            Some("additional_tools") => add_tools(
                item["tools"]
                    .as_array()
                    .ok_or_else(|| invalid("missing additional tools"))?,
                None,
                &mut tools,
                &mut names,
            )?,
            Some("agent_message") => {
                let parts = content(&item["content"])?;
                messages.push(json!({"role":"user","content":parts}));
            }
            Some("web_search_call") => {
                messages.push(json!({"role":"assistant","content":format!("Previous web search: {}", item["action"])}));
            }
            kind => {
                return Err(invalid(format!(
                    "unsupported Chat Completions history item {kind:?}"
                )));
            }
        }
    }
    if !pending_images.is_empty() {
        messages.push(json!({"role":"user","content":pending_images}));
    }
    let mut seen = std::collections::HashSet::new();
    tools.retain(|tool| seen.insert(tool["function"]["name"].as_str().unwrap_or("").to_string()));
    let mut body = json!({"model":request["model"],"messages":messages,"stream":true,"stream_options":{"include_usage":true}});
    if !tools.is_empty() {
        body["tools"] = tools.into();
        body["tool_choice"] = request["tool_choice"]
            .as_str()
            .filter(|choice| !choice.is_empty())
            .unwrap_or("auto")
            .into();
        body["parallel_tool_calls"] = request["parallel_tool_calls"].clone();
    }
    if let Some(effort) = request["reasoning"]["effort"].as_str() {
        body["reasoning_effort"] = if kimi && effort == "medium" {
            "high"
        } else {
            effort
        }
        .into();
    }
    if !request["text"]["format"].is_null() {
        if kimi && body["tools"].is_array() {
            return Err(invalid(
                "Copilot Kimi ignores tool calls with JSON schema output. Run without --output-schema to use tools.",
            ));
        }
        let mut format = request["text"]["format"].clone();
        if let Some(object) = format.as_object_mut() {
            object.remove("type");
        }
        body["response_format"] = json!({"type":"json_schema","json_schema":format});
    }
    Ok((body, names))
}
