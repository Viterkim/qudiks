use super::chat_completions::request::ToolKind;
use super::chat_completions::request::ToolNames;
use super::chat_completions::request::add_tools;
use super::chat_completions::request::tool_name;
use crate::common::ResponseEvent;
use crate::common::ResponseStream;
use crate::error::ApiError;
use codex_protocol::models::ResponseItem;
use serde_json::Value;
use serde_json::json;
use tokio::sync::mpsc;

pub(super) fn enabled(provider: &str, model: &str) -> bool {
    provider == "GitHub Copilot" && model.starts_with("grok")
}

pub(super) fn translate(body: &mut Value) -> Result<ToolNames, ApiError> {
    let mut names = ToolNames::new();
    if let Some(tools) = body["tools"].as_array_mut() {
        let mut translated = Vec::new();
        for tool in std::mem::take(tools) {
            if matches!(
                tool["type"].as_str(),
                Some("function" | "namespace" | "custom")
            ) {
                let mut functions = Vec::new();
                add_tools(&[tool], None, &mut functions, &mut names)?;
                translated.extend(functions.into_iter().map(|mut tool| {
                    let mut function = tool["function"].take();
                    function["type"] = "function".into();
                    function
                }));
            } else {
                translated.push(tool);
            }
        }
        *tools = translated;
    }
    if let Some(input) = body["input"].as_array_mut() {
        for item in input {
            match item["type"].as_str() {
                Some("custom_tool_call") if item["name"].is_string() => {
                    let name = item["name"].as_str().unwrap_or_default();
                    let wire = tool_name(&mut names, item["namespace"].as_str(), name)?;
                    *item = json!({
                        "type":"function_call", "call_id":item["call_id"], "name":wire,
                        "arguments":json!({"input":item["input"]}).to_string()
                    });
                }
                Some("function_call" | "function_call_output") if item["name"].is_string() => {
                    let name = item["name"].as_str().unwrap_or_default();
                    let wire = tool_name(&mut names, item["namespace"].as_str(), name)?;
                    item["name"] = wire.into();
                    if let Some(item) = item.as_object_mut() {
                        item.remove("namespace");
                    }
                }
                Some("custom_tool_call_output") => {
                    *item = json!({
                        "type":"function_call_output", "call_id":item["call_id"],
                        "output":item["output"]
                    });
                }
                _ => {}
            }
        }
    }
    Ok(names)
}

pub(super) fn restore_calls(mut stream: ResponseStream, names: ToolNames) -> ResponseStream {
    if !names
        .values()
        .any(|tool| tool.namespace.is_some() || tool.kind == ToolKind::Custom)
    {
        return stream;
    }
    let (tx, rx) = mpsc::channel(32);
    let mut upstream = std::mem::replace(&mut stream.rx_event, rx);
    tokio::spawn(async move {
        loop {
            let mut event = tokio::select! {
                () = tx.closed() => break,
                event = upstream.recv() => match event {
                    Some(event) => event,
                    None => break,
                },
            };
            let added = matches!(event, Ok(ResponseEvent::OutputItemAdded(_)));
            if let Ok(ResponseEvent::OutputItemDone(item) | ResponseEvent::OutputItemAdded(item)) =
                &mut event
                && let ResponseItem::FunctionCall {
                    name,
                    namespace,
                    id,
                    call_id,
                    arguments,
                    ..
                } = item
                && let Some(tool) = names.get(name)
            {
                if tool.kind == ToolKind::Custom {
                    let input = if added {
                        Some(String::new())
                    } else {
                        serde_json::from_str::<Value>(arguments)
                            .ok()
                            .and_then(|args| args["input"].as_str().map(str::to_owned))
                    };
                    if let Some(input) = input {
                        *item = ResponseItem::CustomToolCall {
                            encrypted_content: None,
                            id: id.take(),
                            status: None,
                            call_id: std::mem::take(call_id),
                            name: tool.name.clone(),
                            namespace: tool.namespace.clone(),
                            input,
                            internal_chat_message_metadata_passthrough: None,
                        };
                    } else {
                        event = Err(ApiError::Stream(format!(
                            "missing custom tool input for {name}"
                        )));
                    }
                } else {
                    name.clone_from(&tool.name);
                    namespace.clone_from(&tool.namespace);
                }
            }
            if tx.send(event).await.is_err() {
                break;
            }
        }
    });
    stream
}
