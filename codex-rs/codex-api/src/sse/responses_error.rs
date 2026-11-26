//! Classifies failed Responses events and their retry advice.
//! Only rate-limit codes derive retry delays from plaintext messages.

use crate::error::ApiError;
use crate::error::parse_terminal_provider_error;
use crate::rate_limits::retry_after_from_message;
use crate::responses_headers::json_headers_to_http_headers;
use codex_http_client::RetryAfter;
use serde::Deserialize;
use serde_json::Value;

// Keep decoding unused fields: malformed values currently produce a stream error.
#[derive(Debug, Deserialize)]
#[allow(dead_code)]
struct Error {
    r#type: Option<String>,
    code: Option<String>,
    message: Option<String>,
    plan_type: Option<String>,
    resets_at: Option<i64>,
    #[serde(default)]
    misalignment: Option<Value>,
}

fn retry_after_header(headers: Option<&Value>) -> Option<RetryAfter> {
    headers
        .and_then(Value::as_object)
        .and_then(|headers| RetryAfter::from_headers(&json_headers_to_http_headers(headers)))
}

/// Classifies recognized statusless error events without treating unknown notifications as failures.
pub(super) fn parse_error_event(
    error: Option<&Value>,
    headers: Option<&Value>,
) -> Option<ApiError> {
    if let Some(error) = parse_terminal_provider_error(error, /*status*/ None) {
        return Some(error);
    }
    let error = error?;
    let code = error.get("code")?.as_str()?;
    match code {
        "policy_capacity_unavailable" | "server_is_overloaded" => {
            let retry_after =
                retry_after_header(error.get("headers")).or_else(|| retry_after_header(headers));
            Some(if code == "policy_capacity_unavailable" {
                ApiError::PolicyCapacityUnavailable { retry_after }
            } else {
                ApiError::ServerOverloaded { retry_after }
            })
        }
        _ => None,
    }
}

pub(super) fn parse_failed_response(response: Option<Value>) -> ApiError {
    let error = response.as_ref().and_then(|response| response.get("error"));
    let retry_after_header = retry_after_header(error.and_then(|error| error.get("headers")));
    if let Some(error) = parse_terminal_provider_error(error, /*status*/ None) {
        return error;
    }
    let Some(error) = error.and_then(|error| serde_json::from_value::<Error>(error.clone()).ok())
    else {
        let message = "response.failed event received".to_string();
        return match retry_after_header {
            Some(_) => ApiError::Retryable {
                message,
                retry_after: retry_after_header,
            },
            None => ApiError::Stream(message),
        };
    };

    match error.code.as_deref() {
        Some("policy_capacity_unavailable") => ApiError::PolicyCapacityUnavailable {
            retry_after: retry_after_header,
        },
        Some("server_is_overloaded") => ApiError::ServerOverloaded {
            retry_after: retry_after_header,
        },
        Some("rate_limit_exceeded" | "slow_down") => {
            let retry_after = retry_after_header
                .or_else(|| error.message.as_deref().and_then(retry_after_from_message));
            ApiError::RateLimitExceeded {
                message: error.message.unwrap_or_default(),
                retry_after,
            }
        }
        _ => ApiError::Retryable {
            message: error.message.unwrap_or_default(),
            retry_after: retry_after_header,
        },
    }
}

#[cfg(test)]
#[path = "responses_error_parser_tests.rs"]
mod tests;
