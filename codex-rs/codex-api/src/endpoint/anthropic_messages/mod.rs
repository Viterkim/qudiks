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

const THINKING_PREFIX: &str = "claude_thinking:";
const THINKING_BINDING_BETA: &str = "thinking-binding-controls-2026-08-01";

pub(super) async fn stream<T: HttpTransport>(
    session: &EndpointSession<T>,
    request: Value,
    headers: HeaderMap,
    telemetry: Option<Arc<dyn SseTelemetry>>,
    on_encoded: impl FnOnce(&bytes::Bytes),
) -> Result<ResponseStream, ApiError> {
    let (body, names) = request::translate(request)?;
    let binding = body["thinking"]["block_binding"].is_object();
    let body = EncodedJsonBody::encode(&body)
        .map_err(|error| ApiError::Stream(format!("encoding Claude Messages request: {error}")))?;
    on_encoded(body.shared_bytes());
    let response = session
        .stream_encoded_json_with(Method::POST, "/messages", headers, Some(body), |req| {
            req.headers.insert(
                http::header::ACCEPT,
                HeaderValue::from_static("text/event-stream"),
            );
            req.headers
                .entry("anthropic-version")
                .or_insert(HeaderValue::from_static("2026-10-21"));
            if binding {
                req.headers.append(
                    "anthropic-beta",
                    HeaderValue::from_static(THINKING_BINDING_BETA),
                );
            }
        })
        .await?;
    Ok(stream::spawn(
        response,
        session.provider().stream_idle_timeout,
        telemetry,
        names,
    ))
}
