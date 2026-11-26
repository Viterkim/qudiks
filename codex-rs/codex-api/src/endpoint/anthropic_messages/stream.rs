use super::THINKING_PREFIX;
use crate::common::ResponseEvent;
use crate::common::ResponseStream;
use crate::endpoint::chat_completions::request::ToolNames;
use crate::endpoint::chat_completions::request::decode_tool_call;
use crate::error::ApiError;
use crate::telemetry::SseTelemetry;
use codex_client::StreamResponse;
use codex_protocol::error::ResponseIncompleteError;
use codex_protocol::models::ResponseItem;
use codex_protocol::protocol::TokenUsage;
use eventsource_stream::Eventsource;
use futures::StreamExt;
use serde_json::Value;
use serde_json::json;
use std::collections::HashSet;
use std::sync::Arc;
use std::time::Duration;
use tokio::sync::mpsc;
use tokio::time::Instant;
use tokio::time::timeout;
use uuid::Uuid;

type EventSender = mpsc::Sender<Result<ResponseEvent, ApiError>>;

async fn send(tx: &EventSender, event: ResponseEvent) -> Result<(), ApiError> {
    tx.send(Ok(event))
        .await
        .map_err(|_| ApiError::Stream("response consumer closed".into()))
}

fn item(value: Value) -> Result<ResponseItem, ApiError> {
    serde_json::from_value(value)
        .map_err(|error| ApiError::Stream(format!("invalid Claude response item: {error}")))
}

struct Block {
    index: u64,
    id: String,
    value: Value,
    arguments: String,
}

impl Block {
    fn display_item(&self, complete: bool) -> Result<ResponseItem, ApiError> {
        if self.value["type"] == "text" {
            return item(json!({
                "type":"message","id":self.id,"role":"assistant",
                "content":[{"type":"output_text","text":self.value["text"]}]
            }));
        }
        let content = self.value["thinking"]
            .as_str()
            .filter(|text| !text.is_empty())
            .map(|text| json!([{"type":"reasoning_text","text":text}]));
        let encoded = (complete
            && (self.value["type"] == "redacted_thinking"
                || self.value["signature"]
                    .as_str()
                    .is_some_and(|s| !s.is_empty())))
        .then(|| format!("{THINKING_PREFIX}{}", self.value));
        item(json!({
            "type":"reasoning","id":self.id,"summary":[],
            "content":content,"encrypted_content":encoded
        }))
    }
}

#[derive(Default)]
struct Completion {
    id: Option<String>,
    active: Option<Block>,
    usage: Option<TokenUsage>,
    stop: Option<String>,
    call_ids: HashSet<String>,
    pending: Vec<ResponseItem>,
}

impl Completion {
    fn update_usage(&mut self, value: &Value) {
        if !value.is_object() {
            return;
        }
        let usage = self.usage.get_or_insert_default();
        let uncached_input_tokens = value["input_tokens"].as_i64().unwrap_or(
            usage.input_tokens - usage.cached_input_tokens - usage.cache_write_input_tokens,
        );
        if let Some(tokens) = value["cache_read_input_tokens"].as_i64() {
            usage.cached_input_tokens = tokens;
        }
        if let Some(tokens) = value["cache_creation_input_tokens"].as_i64() {
            usage.cache_write_input_tokens = tokens;
        }
        usage.input_tokens =
            uncached_input_tokens + usage.cached_input_tokens + usage.cache_write_input_tokens;
        if let Some(tokens) = value["output_tokens"].as_i64() {
            usage.output_tokens = tokens;
        }
        if let Some(tokens) = value["output_tokens_details"]["thinking_tokens"].as_i64() {
            usage.reasoning_output_tokens = tokens;
        }
        usage.total_tokens = usage.input_tokens + usage.output_tokens;
    }

    async fn event(
        &mut self,
        event: Value,
        names: &ToolNames,
        tx: &EventSender,
    ) -> Result<(), ApiError> {
        match event["type"].as_str() {
            Some("message_start") => {
                let message = &event["message"];
                self.id = message["id"].as_str().map(str::to_owned);
                self.update_usage(&message["usage"]);
                send(
                    tx,
                    ResponseEvent::Created {
                        response_id: self.id.clone(),
                    },
                )
                .await?;
                if let Some(model) = message["model"].as_str() {
                    send(tx, ResponseEvent::ServerModel(model.into())).await?;
                }
            }
            Some("content_block_start") => {
                let index = event["index"]
                    .as_u64()
                    .filter(|_| self.active.is_none())
                    .ok_or_else(|| ApiError::Stream("unexpected Claude block start".into()))?;
                let value = &event["content_block"];
                let prefix = match value["type"].as_str() {
                    Some("text") => "msg",
                    Some("thinking" | "redacted_thinking") => "rs",
                    Some("tool_use") => "fc",
                    kind => {
                        return Err(ApiError::Stream(format!(
                            "unsupported Claude block {kind:?}"
                        )));
                    }
                };
                let block = Block {
                    index,
                    id: format!("{prefix}_{}", Uuid::new_v4().simple()),
                    value: value.clone(),
                    arguments: String::new(),
                };
                if self.pending.is_empty() && block.value["type"] != "tool_use" {
                    send(
                        tx,
                        ResponseEvent::OutputItemAdded(block.display_item(/*complete*/ false)?),
                    )
                    .await?;
                }
                self.active = Some(block);
            }
            Some("content_block_delta") => {
                let block = self
                    .active
                    .as_mut()
                    .filter(|block| event["index"].as_u64() == Some(block.index))
                    .ok_or_else(|| ApiError::Stream("unexpected Claude block delta".into()))?;
                let delta = &event["delta"];
                let emit = match (block.value["type"].as_str(), delta["type"].as_str()) {
                    (Some("text"), Some("text_delta")) => {
                        let text = delta["text"].as_str().unwrap_or_default();
                        if let Value::String(previous) = &mut block.value["text"] {
                            previous.push_str(text);
                        }
                        Some(ResponseEvent::OutputTextDelta(text.into()))
                    }
                    (Some("thinking"), Some("thinking_delta")) => {
                        let text = delta["thinking"].as_str().unwrap_or_default();
                        if let Value::String(previous) = &mut block.value["thinking"] {
                            previous.push_str(text);
                        }
                        Some(ResponseEvent::ReasoningContentDelta {
                            delta: text.into(),
                            content_index: 0,
                        })
                    }
                    (Some("thinking"), Some("signature_delta")) => {
                        block.value["signature"] = delta["signature"].clone();
                        None
                    }
                    (Some("tool_use"), Some("input_json_delta")) => {
                        block
                            .arguments
                            .push_str(delta["partial_json"].as_str().unwrap_or_default());
                        None
                    }
                    (Some("text"), Some("citations_delta")) => None,
                    (_, kind) => {
                        return Err(ApiError::Stream(format!(
                            "unsupported Claude delta {kind:?}"
                        )));
                    }
                };
                if self.pending.is_empty()
                    && let Some(event) = emit
                {
                    send(tx, event).await?;
                }
            }
            Some("content_block_stop") => {
                let block = self
                    .active
                    .take()
                    .filter(|block| event["index"].as_u64() == Some(block.index))
                    .ok_or_else(|| ApiError::Stream("unexpected Claude block stop".into()))?;
                if block.value["type"] == "tool_use" {
                    let id = block.value["id"]
                        .as_str()
                        .filter(|id| !id.is_empty())
                        .ok_or_else(|| ApiError::Stream("Claude tool call missing id".into()))?;
                    if !self.call_ids.insert(id.into()) {
                        return Err(ApiError::Stream(format!(
                            "duplicate Claude tool call id: {id}"
                        )));
                    }
                    let wire = block.value["name"].as_str().unwrap_or_default();
                    let arguments = if block.arguments.is_empty() {
                        block.value["input"].to_string()
                    } else {
                        block.arguments
                    };
                    self.pending
                        .push(decode_tool_call(names, id, wire, &arguments)?);
                } else {
                    let item = block.display_item(/*complete*/ true)?;
                    if self.pending.is_empty() {
                        send(tx, ResponseEvent::OutputItemDone(item)).await?;
                    } else {
                        self.pending.push(item);
                    }
                }
            }
            Some("message_delta") => {
                self.update_usage(&event["usage"]);
                if let Some(reason) = event["delta"]["stop_reason"].as_str() {
                    self.stop = Some(reason.into());
                }
            }
            Some("error") => {
                return Err(ApiError::Stream(format!(
                    "Claude Messages error: {}",
                    event["error"]
                )));
            }
            _ => {}
        }
        Ok(())
    }

    async fn complete(self, tx: &EventSender) -> Result<(), ApiError> {
        let has_calls = !self.call_ids.is_empty();
        match self.stop.as_deref() {
            Some("end_turn" | "stop_sequence") if !has_calls => {}
            Some("tool_use") if has_calls => {}
            Some("model_context_window_exceeded") => return Err(ApiError::ContextWindowExceeded),
            Some(reason) => {
                return Err(ApiError::ResponseIncomplete(Box::new(
                    ResponseIncompleteError {
                        partial_output: Vec::new(),
                        reason: reason.into(),
                        response_id: self.id,
                        token_usage: self.usage,
                    },
                )));
            }
            None => {
                return Err(ApiError::Stream(
                    "Claude stream stopped without a stop reason".into(),
                ));
            }
        }
        let id = self
            .id
            .filter(|_| self.active.is_none())
            .ok_or_else(|| ApiError::Stream("incomplete Claude message".into()))?;
        // Delay dispatch until the provider confirms the whole tool batch completed.
        for item in self.pending {
            send(tx, ResponseEvent::OutputItemAdded(item.clone())).await?;
            send(tx, ResponseEvent::OutputItemDone(item)).await?;
        }
        send(
            tx,
            ResponseEvent::Completed {
                response_id: id,
                token_usage: self.usage,
                reasoning_removals: None,
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
        .get("request-id")
        .or_else(|| response.headers.get("x-request-id"))
        .and_then(|value| value.to_str().ok())
        .map(str::to_owned);
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
                    .map_err(|_| ApiError::Stream("Claude Messages stream timed out".into()))?
                    .ok_or_else(|| {
                        ApiError::Stream("Claude stream closed before message_stop".into())
                    })?
                    .map_err(|error| {
                        ApiError::Stream(format!("Claude Messages stream: {error}"))
                    })?;
                let event: Value = serde_json::from_str(&event.data).map_err(|error| {
                    ApiError::Stream(format!("invalid Claude Messages event: {error}"))
                })?;
                if event["type"] == "message_stop" {
                    return completion.complete(&tx).await;
                }
                completion.event(event, &names, &tx).await?;
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
