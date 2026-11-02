use super::request::ToolKind;
use super::request::ToolNames;
use crate::common::ResponseEvent;
use crate::common::ResponseStream;
use crate::error::ApiError;
use crate::telemetry::SseTelemetry;
use codex_client::StreamResponse;
use codex_protocol::models::ResponseItem;
use codex_protocol::protocol::TokenUsage;
use eventsource_stream::Eventsource;
use futures::StreamExt;
use serde_json::Value;
use serde_json::json;
use std::collections::BTreeMap;
use std::sync::Arc;
use std::time::Duration;
use tokio::sync::mpsc;
use tokio::time::Instant;
use tokio::time::timeout;
use uuid::Uuid;

#[derive(Default)]
struct Completion {
    id: String,
    active: Option<Value>,
    calls: BTreeMap<u64, Value>,
    finish: Option<String>,
    usage: Option<TokenUsage>,
}

fn item(value: Value) -> Result<ResponseItem, ApiError> {
    serde_json::from_value(value)
        .map_err(|error| ApiError::Stream(format!("invalid Chat Completions item: {error}")))
}

async fn send(
    tx: &mpsc::Sender<Result<ResponseEvent, ApiError>>,
    event: ResponseEvent,
) -> Result<(), ApiError> {
    tx.send(Ok(event))
        .await
        .map_err(|_| ApiError::Stream("response consumer closed".into()))
}

impl Completion {
    async fn close(
        &mut self,
        tx: &mpsc::Sender<Result<ResponseEvent, ApiError>>,
    ) -> Result<(), ApiError> {
        if let Some(active) = self.active.take() {
            send(tx, ResponseEvent::OutputItemDone(item(active)?)).await?;
        }
        Ok(())
    }

    async fn text(
        &mut self,
        text: &str,
        kind: &str,
        tx: &mpsc::Sender<Result<ResponseEvent, ApiError>>,
    ) -> Result<(), ApiError> {
        if text.is_empty() {
            return Ok(());
        }
        if self
            .active
            .as_ref()
            .is_none_or(|active| active["type"] != kind)
        {
            self.close(tx).await?;
            let prefix = if kind == "reasoning" { "rs" } else { "msg" };
            let id = format!("{prefix}_{}", Uuid::new_v4().simple());
            let active = if kind == "reasoning" {
                json!({"type":"reasoning","id":id,"summary":[],"content":[{"type":"reasoning_text","text":""}],"encrypted_content":null})
            } else {
                json!({"type":"message","id":id,"role":"assistant","content":[{"type":"output_text","text":""}]})
            };
            send(tx, ResponseEvent::OutputItemAdded(item(active.clone())?)).await?;
            self.active = Some(active);
        }
        if let Some(active) = &mut self.active
            && let Value::String(previous) = &mut active["content"][0]["text"]
        {
            previous.push_str(text);
        }
        let event = if kind == "reasoning" {
            ResponseEvent::ReasoningContentDelta {
                delta: text.into(),
                content_index: 0,
            }
        } else {
            ResponseEvent::OutputTextDelta(text.into())
        };
        send(tx, event).await
    }

    async fn chunk(
        &mut self,
        chunk: Value,
        tx: &mpsc::Sender<Result<ResponseEvent, ApiError>>,
    ) -> Result<(), ApiError> {
        if !chunk["error"].is_null() {
            return Err(ApiError::Stream(format!(
                "Chat Completions error: {}",
                chunk["error"]
            )));
        }
        if self.id.is_empty() {
            self.id = chunk["id"].as_str().unwrap_or("chat_completion").into();
            send(
                tx,
                ResponseEvent::Created {
                    response_id: Some(self.id.clone()),
                },
            )
            .await?;
            if let Some(model) = chunk["model"].as_str() {
                send(tx, ResponseEvent::ServerModel(model.into())).await?;
            }
        }
        if let Some(usage) = chunk.get("usage").filter(|usage| usage.is_object()) {
            self.usage = Some(TokenUsage {
                input_tokens: usage["prompt_tokens"].as_i64().unwrap_or(0),
                cached_input_tokens: usage["prompt_tokens_details"]["cached_tokens"]
                    .as_i64()
                    .unwrap_or(0),
                output_tokens: usage["completion_tokens"].as_i64().unwrap_or(0),
                reasoning_output_tokens: usage["reasoning_tokens"]
                    .as_i64()
                    .or_else(|| usage["completion_tokens_details"]["reasoning_tokens"].as_i64())
                    .unwrap_or(0),
                total_tokens: usage["total_tokens"].as_i64().unwrap_or(0),
                ..TokenUsage::default()
            });
        }
        for choice in chunk["choices"].as_array().into_iter().flatten() {
            if choice["index"].as_u64().unwrap_or(0) != 0 {
                return Err(ApiError::Stream(
                    "multiple Chat Completions choices are unsupported".into(),
                ));
            }
            let delta = &choice["delta"];
            if let Some(text) = delta["reasoning_text"]
                .as_str()
                .or_else(|| delta["reasoning_content"].as_str())
            {
                self.text(text, "reasoning", tx).await?;
            }
            if let Some(text) = delta["content"].as_str() {
                self.text(text, "message", tx).await?;
            }
            if let Some(text) = delta["refusal"].as_str() {
                self.text(text, "message", tx).await?;
            }
            for call in delta["tool_calls"].as_array().into_iter().flatten() {
                let index = call["index"]
                    .as_u64()
                    .ok_or_else(|| ApiError::Stream("tool call missing index".into()))?;
                let saved = self
                    .calls
                    .entry(index)
                    .or_insert_with(|| json!({"id":"","function":{"name":"","arguments":""}}));
                for pointer in ["/id", "/function/name", "/function/arguments"] {
                    if let Some(fragment) = call.pointer(pointer).and_then(Value::as_str)
                        && let Some(Value::String(previous)) = saved.pointer_mut(pointer)
                    {
                        previous.push_str(fragment);
                    }
                }
            }
            if let Some(finish) = choice["finish_reason"].as_str() {
                self.finish = Some(finish.into());
            }
        }
        Ok(())
    }

    async fn complete(
        mut self,
        names: ToolNames,
        tx: &mpsc::Sender<Result<ResponseEvent, ApiError>>,
    ) -> Result<(), ApiError> {
        match self.finish.as_deref() {
            Some("stop" | "tool_calls") => {}
            Some("content_filter") => return Err(ApiError::ContentFilter),
            finish => {
                return Err(ApiError::Stream(format!(
                    "incomplete Chat Completions response: {finish:?}"
                )));
            }
        }
        self.close(tx).await?;
        let has_calls = !self.calls.is_empty();
        if (self.finish.as_deref() == Some("tool_calls")) != has_calls {
            return Err(ApiError::Stream(
                "Chat Completions finish reason does not match tool calls".into(),
            ));
        }
        let mut items = Vec::new();
        for call in self.calls.into_values() {
            let id = call["id"]
                .as_str()
                .filter(|id| !id.is_empty())
                .ok_or_else(|| ApiError::Stream("tool call missing id".into()))?;
            let wire = call["function"]["name"].as_str().unwrap_or("");
            let mapped = names.get(wire).ok_or_else(|| {
                ApiError::Stream(format!("unknown Chat Completions tool: {wire}"))
            })?;
            let arguments = call["function"]["arguments"].as_str().unwrap_or("");
            let parsed = serde_json::from_str::<Value>(arguments).map_err(|error| {
                ApiError::Stream(format!("incomplete tool arguments for {wire}: {error}"))
            })?;
            let prefix = match mapped.kind {
                ToolKind::Function => "fc",
                ToolKind::Custom => "ctc",
                ToolKind::Search => "ts",
            };
            let item_id = format!("{prefix}_{}", Uuid::new_v4().simple());
            let value = match mapped.kind {
                ToolKind::Function => {
                    json!({"type":"function_call","id":item_id,"call_id":id,"name":mapped.name,"namespace":mapped.namespace,"arguments":arguments})
                }
                ToolKind::Search => {
                    json!({"type":"tool_search_call","id":item_id,"call_id":id,"execution":"client","arguments":parsed})
                }
                ToolKind::Custom => {
                    let input = parsed["input"].as_str().ok_or_else(|| {
                        ApiError::Stream(format!("missing custom tool input for {wire}"))
                    })?;
                    json!({"type":"custom_tool_call","id":item_id,"call_id":id,"name":mapped.name,"namespace":mapped.namespace,"input":input})
                }
            };
            items.push(item(value)?);
        }
        for item in items {
            send(tx, ResponseEvent::OutputItemAdded(item.clone())).await?;
            send(tx, ResponseEvent::OutputItemDone(item)).await?;
        }
        send(
            tx,
            ResponseEvent::Completed {
                response_id: self.id,
                token_usage: self.usage,
                usage_metadata: None,
                end_turn: Some(!has_calls),
            },
        )
        .await
    }
}

pub(super) fn spawn(
    response: StreamResponse,
    idle_timeout: Duration,
    telemetry: Option<Arc<dyn SseTelemetry>>,
    names: ToolNames,
) -> ResponseStream {
    let upstream_request_id = response
        .headers
        .get("x-request-id")
        .and_then(|value| value.to_str().ok())
        .map(str::to_string);
    let (tx, rx_event) = mpsc::channel(1600);
    tokio::spawn(async move {
        let mut events = response.bytes.eventsource();
        let mut completion = Completion::default();
        let result = async {
            loop {
                let started = Instant::now();
                let next = tokio::select! {
                    _ = tx.closed() => return Ok(()),
                    next = timeout(idle_timeout, events.next()) => next,
                };
                if let Some(telemetry) = &telemetry {
                    telemetry.on_sse_poll(&next, started.elapsed());
                }
                let event = next
                    .map_err(|_| ApiError::Stream("Chat Completions stream timed out".into()))?
                    .ok_or_else(|| {
                        ApiError::Stream("Chat Completions stream closed before [DONE]".into())
                    })?
                    .map_err(|error| {
                        ApiError::Stream(format!("Chat Completions stream: {error}"))
                    })?;
                if event.data.trim() == "[DONE]" {
                    return completion.complete(names, &tx).await;
                }
                let chunk = serde_json::from_str(&event.data).map_err(|error| {
                    ApiError::Stream(format!("invalid Chat Completions chunk: {error}"))
                })?;
                completion.chunk(chunk, &tx).await?;
            }
        }
        .await;
        if let Err(error) = result {
            let _ = tx.send(Err(error)).await;
        }
    });
    ResponseStream {
        routing_affinity: None,
        rx_event,
        upstream_request_id,
        interrupt: None,
    }
}
