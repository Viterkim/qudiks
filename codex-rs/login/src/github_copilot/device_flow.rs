//! GitHub OAuth device flow.

use super::endpoints::GithubEndpoints;
use super::endpoints::identity_headers;
use super::error::GithubCopilotError;
use super::error::Result;
use super::http::parse_json_response;
use codex_http_client::HttpClient;
use serde::Deserialize;
use std::time::Duration;
use std::time::Instant;

const DEVICE_CODE_SCOPE: &str = "read:user";
const DEVICE_CODE_GRANT_TYPE: &str = "urn:ietf:params:oauth:grant-type:device_code";
const FORM_CONTENT_TYPE: &str = "application/x-www-form-urlencoded";
const MAX_DEVICE_FLOW_WAIT: Duration = Duration::from_secs(15 * 60);
const MIN_POLL_INTERVAL: Duration = Duration::from_secs(1);
const SLOW_DOWN_BACKOFF: Duration = Duration::from_secs(5);

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GithubDeviceCode {
    pub device_code: String,
    pub user_code: String,
    pub verification_uri: String,
    pub interval: Duration,
    pub expires_in: Duration,
}

#[derive(Deserialize)]
struct DeviceCodeResponse {
    device_code: String,
    user_code: String,
    verification_uri: String,
    expires_in: u64,
    #[serde(default)]
    interval: Option<u64>,
}

#[derive(Deserialize)]
struct AccessTokenResponse {
    #[serde(default)]
    access_token: Option<String>,
    #[serde(default)]
    error: Option<String>,
    #[serde(default)]
    error_description: Option<String>,
    #[serde(default)]
    interval: Option<u64>,
}

fn form_body(fields: &[(&str, &str)]) -> String {
    fields
        .iter()
        .map(|(key, value)| {
            format!(
                "{}={}",
                urlencoding::encode(key),
                urlencoding::encode(value)
            )
        })
        .collect::<Vec<_>>()
        .join("&")
}

pub async fn request_device_code(
    client: &HttpClient,
    endpoints: &GithubEndpoints,
    client_id: &str,
) -> Result<GithubDeviceCode> {
    const CONTEXT: &str = "GitHub device-code request";
    let response = client
        .post(&endpoints.device_code_url)
        .headers(identity_headers())
        .header("content-type", FORM_CONTENT_TYPE)
        .body(form_body(&[
            ("client_id", client_id),
            ("scope", DEVICE_CODE_SCOPE),
        ]))
        .send()
        .await
        .map_err(|source| GithubCopilotError::Transport {
            context: CONTEXT,
            source,
        })?;

    let parsed: DeviceCodeResponse = parse_json_response(response, CONTEXT).await?;
    Ok(GithubDeviceCode {
        device_code: parsed.device_code,
        user_code: parsed.user_code,
        verification_uri: parsed.verification_uri,
        interval: Duration::from_secs(parsed.interval.unwrap_or(5)).max(MIN_POLL_INTERVAL),
        expires_in: Duration::from_secs(parsed.expires_in).min(MAX_DEVICE_FLOW_WAIT),
    })
}

pub async fn poll_for_access_token(
    client: &HttpClient,
    endpoints: &GithubEndpoints,
    client_id: &str,
    device_code: &GithubDeviceCode,
) -> Result<String> {
    const CONTEXT: &str = "GitHub device-token request";
    let started_at = Instant::now();
    let deadline = device_code.expires_in.min(MAX_DEVICE_FLOW_WAIT);
    let mut interval = device_code.interval;

    while started_at.elapsed() < deadline {
        tokio::time::sleep(interval).await;
        let response = client
            .post(&endpoints.access_token_url)
            .headers(identity_headers())
            .header("content-type", FORM_CONTENT_TYPE)
            .body(form_body(&[
                ("client_id", client_id),
                ("device_code", &device_code.device_code),
                ("grant_type", DEVICE_CODE_GRANT_TYPE),
            ]))
            .send()
            .await
            .map_err(|source| GithubCopilotError::Transport {
                context: CONTEXT,
                source,
            })?;

        let parsed: AccessTokenResponse = parse_json_response(response, CONTEXT).await?;
        if let Some(access_token) = parsed.access_token {
            return Ok(access_token);
        }

        match parsed.error.as_deref() {
            Some("authorization_pending") => {}
            Some("slow_down") => {
                let advertised = parsed.interval.map(Duration::from_secs).unwrap_or_default();
                interval = (interval + SLOW_DOWN_BACKOFF).max(advertised);
            }
            Some("access_denied") => return Err(GithubCopilotError::LoginDenied),
            Some("expired_token") => return Err(GithubCopilotError::DeviceCodeExpired),
            Some(other) => {
                let detail = parsed
                    .error_description
                    .unwrap_or_else(|| other.to_string());
                return Err(GithubCopilotError::DeviceFlow(detail));
            }
            None => {
                return Err(GithubCopilotError::InvalidResponse {
                    context: CONTEXT,
                    detail: "response contained neither access_token nor error".to_string(),
                });
            }
        }
    }
    Err(GithubCopilotError::DeviceFlowTimeout)
}

#[cfg(test)]
#[path = "device_flow_tests.rs"]
mod tests;
