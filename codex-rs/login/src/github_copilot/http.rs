use super::error::GithubCopilotError;
use super::error::Result;
use codex_http_client::HttpResponse;
use serde::de::DeserializeOwned;

// Cap error bodies: a response may be huge or carry credentials.
const MAX_BODY_SNIPPET: usize = 512;

fn body_snippet(body: &str) -> String {
    let trimmed = body.trim();
    match trimmed.char_indices().nth(MAX_BODY_SNIPPET) {
        Some((index, _)) => format!("{}…", &trimmed[..index]),
        None => trimmed.to_string(),
    }
}

pub async fn parse_json_response<T: DeserializeOwned>(
    response: HttpResponse,
    context: &'static str,
) -> Result<T> {
    let status = response.status();
    let body = response
        .text()
        .await
        .map_err(|source| GithubCopilotError::Transport { context, source })?;

    if !status.is_success() {
        return Err(GithubCopilotError::HttpStatus {
            context,
            status: status.as_u16(),
            body: body_snippet(&body),
        });
    }

    serde_json::from_str(&body).map_err(|err| GithubCopilotError::InvalidResponse {
        context,
        detail: err.to_string(),
    })
}

#[cfg(test)]
#[path = "http_tests.rs"]
mod tests;
