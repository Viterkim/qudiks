pub(in crate::endpoint) mod request;
mod stream;

use crate::common::ResponseStream;
use crate::endpoint::session::EndpointSession;
use crate::error::ApiError;
use crate::telemetry::SseTelemetry;
use codex_client::EncodedJsonBody;
use codex_client::HttpTransport;
use codex_protocol::models::ResponseItem;
use http::HeaderMap;
use http::HeaderValue;
use http::Method;
use serde_json::Value;
use std::sync::Arc;
use stream::StreamMode;

pub const COPILOT_GEMINI_REASONING_PREFIX: &str = "copilot_gemini:";
const MAX_REASONING_SIGNATURE_BYTES: usize = 64 * 1024;

pub(crate) fn enabled(provider: &str, model: &str) -> bool {
    is_copilot_gemini(provider, model)
        || provider == "GitHub Copilot"
            && (model.starts_with("kimi-")
                || matches!(model, "claude-sonnet-5.5" | "claude-opus-5.5"))
}

fn is_copilot_gemini(provider: &str, model: &str) -> bool {
    provider == "GitHub Copilot" && model.starts_with("gemini-")
}

fn reasoning_signature(encoded: &str) -> Option<(&str, &str)> {
    let (model, signature) = encoded
        .strip_prefix(COPILOT_GEMINI_REASONING_PREFIX)?
        .split_once(':')?;
    (!signature.is_empty() && signature.len() <= MAX_REASONING_SIGNATURE_BYTES)
        .then_some((model, signature))
}

pub fn retain_copilot_gemini_reasoning(input: &mut Vec<ResponseItem>, provider: &str, model: &str) {
    input.retain(|item| match item {
        ResponseItem::Reasoning {
            encrypted_content: Some(encoded),
            ..
        } if encoded.starts_with(COPILOT_GEMINI_REASONING_PREFIX) => {
            is_copilot_gemini(provider, model)
                && reasoning_signature(encoded)
                    .is_some_and(|(source_model, _)| source_model == model)
        }
        _ => true,
    });
}

pub(crate) async fn stream<T: HttpTransport>(
    session: &EndpointSession<T>,
    request: Value,
    headers: HeaderMap,
    telemetry: Option<Arc<dyn SseTelemetry>>,
    on_encoded: impl FnOnce(&bytes::Bytes),
) -> Result<ResponseStream, ApiError> {
    let provider = &session.provider().name;
    let model = request["model"].as_str().unwrap_or_default();
    let stream_mode = if is_copilot_gemini(provider, model) {
        StreamMode::CopilotGemini(model.to_string())
    } else {
        StreamMode::Standard
    };
    let (body, names) = request::translate(request, provider)?;
    let body = EncodedJsonBody::encode(&body)
        .map_err(|error| ApiError::Stream(format!("encoding Chat Completions request: {error}")))?;
    on_encoded(body.shared_bytes());
    let response = session
        .stream_encoded_json_with(
            Method::POST,
            "/chat/completions",
            headers,
            Some(body),
            |request| {
                request.headers.insert(
                    http::header::ACCEPT,
                    HeaderValue::from_static("text/event-stream"),
                );
            },
        )
        .await?;

    Ok(stream::spawn(
        response,
        session.provider().stream_idle_timeout,
        telemetry,
        names,
        stream_mode,
    ))
}

#[cfg(test)]
#[path = "chat_completions_tests.rs"]
mod tests;
