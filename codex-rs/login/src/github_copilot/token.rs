//! GitHub token -> short-lived Copilot bearer token.

use super::endpoints::GithubEndpoints;
use super::endpoints::derive_copilot_api_base_url;
use super::endpoints::identity_headers;
use super::error::GithubCopilotError;
use super::error::Result;
use super::http::parse_json_response;
use chrono::DateTime;
use chrono::Utc;
use codex_http_client::HttpClient;
use serde::Deserialize;
use serde::Serialize;

// Renew this early so a request cannot start with a token that dies mid-stream.
pub const TOKEN_REFRESH_BUFFER_SECONDS: i64 = 300;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CopilotToken {
    pub token: String,
    pub expires_at: DateTime<Utc>,
    pub api_base_url: String,
}

impl CopilotToken {
    pub fn is_fresh_at(&self, now: DateTime<Utc>) -> bool {
        self.expires_at.signed_duration_since(now).num_seconds() > TOKEN_REFRESH_BUFFER_SECONDS
    }
}

#[derive(Deserialize)]
struct CopilotTokenResponse {
    token: String,
    expires_at: i64,
}

pub async fn exchange_github_token(
    client: &HttpClient,
    endpoints: &GithubEndpoints,
    github_token: &str,
) -> Result<CopilotToken> {
    const CONTEXT: &str = "GitHub Copilot token exchange";
    let authorization = format!("Bearer {github_token}");
    let response = client
        .get(&endpoints.copilot_token_url)
        .headers(identity_headers())
        .header("authorization", authorization)
        .send()
        .await
        .map_err(|source| GithubCopilotError::Transport {
            context: CONTEXT,
            source,
        })?;

    let parsed: CopilotTokenResponse = parse_json_response(response, CONTEXT).await?;
    let expires_at = DateTime::from_timestamp(parsed.expires_at, 0).ok_or_else(|| {
        GithubCopilotError::InvalidResponse {
            context: CONTEXT,
            detail: format!(
                "expires_at `{}` is not a valid timestamp",
                parsed.expires_at
            ),
        }
    })?;
    let api_base_url = derive_copilot_api_base_url(&parsed.token, endpoints.enterprise_domain())?;
    Ok(CopilotToken {
        token: parsed.token,
        expires_at,
        api_base_url,
    })
}

#[cfg(test)]
#[path = "token_tests.rs"]
mod tests;
