//! Classifies failed Responses events and their retry advice.
//! Only rate-limit codes derive retry delays from plaintext messages.

use crate::error::ApiError;
use crate::error::parse_terminal_provider_error;
use crate::rate_limits::retry_after_from_message;
use crate::responses_headers::json_headers_to_http_headers;
use codex_http_client::RetryAfter;
use codex_protocol::protocol::MisalignmentErrorDetails;
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

pub(super) fn parse_failed_response(response: Option<Value>) -> ApiError {
    let error = response.as_ref().and_then(|response| response.get("error"));
    let retry_after_header = error
        .and_then(|error| error.get("headers"))
        .and_then(Value::as_object)
        .and_then(|headers| RetryAfter::from_headers(&json_headers_to_http_headers(headers)));
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
        Some(
            "insufficient_quota"
            | "credit_balance_exhausted"
            | "organization_spend_limit_exceeded"
            | "project_spend_limit_exceeded",
        ) => ApiError::QuotaExceeded,
        Some("usage_not_included") => ApiError::UsageNotIncluded,
        Some("cyber_policy") => ApiError::CyberPolicy {
            message: cyber_policy_message(error.message),
        },
        Some("bio_policy") => {
            let message = error
                .message
                .filter(|message| !message.trim().is_empty())
                .unwrap_or_else(|| {
                    "This content was flagged for possible biological risk.".to_string()
                });
            ApiError::BioPolicy { message }
        }
        Some("misalignment_policy_violation") => {
            let message = error
                .message
                .filter(|message| !message.trim().is_empty())
                .unwrap_or_else(|| {
                    "This request was blocked due to a misalignment policy violation.".to_string()
                });
            ApiError::MisalignmentPolicyViolation {
                message,
                misalignment: error.misalignment.and_then(|details| {
                    serde_json::from_value::<MisalignmentErrorDetails>(details).ok()
                }),
            }
        }
        Some("invalid_prompt") => ApiError::InvalidPrompt {
            message: error
                .message
                .unwrap_or_else(|| "Invalid request.".to_string()),
        },
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

fn cyber_policy_fallback_message() -> String {
    "This request has been flagged for possible cybersecurity risk.".to_string()
}

fn cyber_policy_message(message: Option<String>) -> String {
    message
        .filter(|message| !message.trim().is_empty())
        .unwrap_or_else(cyber_policy_fallback_message)
}

#[cfg(test)]
#[path = "responses_error_parser_tests.rs"]
mod tests;
