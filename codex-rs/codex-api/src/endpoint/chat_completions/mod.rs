mod request;
mod stream;

use crate::common::ResponseStream;
use crate::endpoint::session::EndpointSession;
use crate::error::ApiError;
use crate::telemetry::SseTelemetry;
use codex_client::EncodedJsonBody;
use codex_client::HttpTransport;
use http::HeaderMap;
use http::HeaderValue;
use http::Method;
use serde_json::Value;
use std::sync::Arc;

pub(crate) fn enabled(provider: &str, model: &str) -> bool {
    provider == "GitHub Copilot"
        && (model.starts_with("kimi-") || matches!(model, "claude-sonnet-5.5" | "claude-opus-5.5"))
}

pub(crate) async fn stream<T: HttpTransport>(
    session: &EndpointSession<T>,
    request: Value,
    headers: HeaderMap,
    telemetry: Option<Arc<dyn SseTelemetry>>,
) -> Result<ResponseStream, ApiError> {
    let (body, names) = request::translate(request)?;
    let body = EncodedJsonBody::encode(&body)
        .map_err(|error| ApiError::Stream(format!("encoding Chat Completions request: {error}")))?;
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
    ))
}

#[cfg(test)]
#[path = "chat_completions_tests.rs"]
mod tests;
