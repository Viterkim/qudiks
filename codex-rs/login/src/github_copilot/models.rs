//! Model discovery. The catalog varies by account and org policy.

use super::endpoints::assert_trusted_copilot_api_url;
use super::endpoints::identity_headers;
use super::error::GithubCopilotError;
use super::error::Result;
use super::http::parse_json_response;
use super::token::CopilotToken;
use codex_http_client::HttpClient;
use serde::Deserialize;
use url::Url;

// Older deployments only serve `/v1/models`.
const MODELS_PATHS: [&str; 2] = ["models", "v1/models"];

// The only transport Codex speaks. Copilot advertises it per model.
const RESPONSES_ENDPOINT: &str = "/responses";

/// A model advertised by the Copilot API.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CopilotModel {
    pub id: String,
    pub name: String,
    // Most of a catalog is chat-only, which Codex cannot use at all.
    pub supports_responses: bool,
    // Embeddings and other old entries omit endpoints, so absence only means
    // something relative to the rest of the catalog.
    pub declares_endpoints: bool,
    pub context_window: Option<i64>,
    pub reasoning_efforts: Vec<String>,
}

#[derive(Deserialize)]
#[serde(untagged)]
enum ModelsPayload {
    List(Vec<ModelEntry>),
    Wrapped {
        #[serde(alias = "models")]
        data: Vec<ModelEntry>,
    },
}

#[derive(Deserialize)]
#[serde(untagged)]
enum ModelEntry {
    Id(String),
    Object {
        #[serde(alias = "model", alias = "slug")]
        id: Option<String>,
        name: Option<String>,
        #[serde(default)]
        supported_endpoints: Vec<String>,
        #[serde(default)]
        capabilities: Option<ModelCapabilities>,
    },
}

#[derive(Deserialize, Default)]
struct ModelCapabilities {
    #[serde(default)]
    limits: Option<ModelLimits>,
    #[serde(default)]
    supports: Option<ModelSupports>,
}

#[derive(Deserialize, Default)]
struct ModelLimits {
    #[serde(default)]
    max_context_window_tokens: Option<i64>,
}

#[derive(Deserialize, Default)]
struct ModelSupports {
    #[serde(default)]
    reasoning_effort: Option<Vec<String>>,
}

impl ModelEntry {
    fn into_model(self) -> Option<CopilotModel> {
        let (id, name, declared, capabilities) = match self {
            Self::Id(id) => (id, None, Vec::new(), None),
            Self::Object {
                id,
                name,
                supported_endpoints,
                capabilities,
            } => match (id, name) {
                (Some(id), name) => (id, name, supported_endpoints, capabilities),
                // Some deployments only label an entry with `name`.
                (None, Some(name)) => (name, None, supported_endpoints, capabilities),
                (None, None) => return None,
            },
        };
        if id.is_empty() {
            return None;
        }
        let name = name.unwrap_or_else(|| id.clone());
        let capabilities = capabilities.unwrap_or_default();
        Some(CopilotModel {
            id,
            name,
            supports_responses: declared
                .iter()
                .any(|endpoint| endpoint == RESPONSES_ENDPOINT),
            declares_endpoints: !declared.is_empty(),
            context_window: capabilities
                .limits
                .and_then(|limits| limits.max_context_window_tokens),
            reasoning_efforts: capabilities
                .supports
                .and_then(|supports| supports.reasoning_effort)
                .unwrap_or_default(),
        })
    }
}

fn normalize(payload: ModelsPayload) -> Vec<CopilotModel> {
    let entries = match payload {
        ModelsPayload::List(entries) | ModelsPayload::Wrapped { data: entries } => entries,
    };
    let mut seen = std::collections::HashSet::new();
    entries
        .into_iter()
        .filter_map(ModelEntry::into_model)
        .filter(|model| seen.insert(model.id.clone()))
        .collect()
}

/// Re-validates the host: credentials on disk are user-editable. `enterprise_domain`
/// must be the signed-in domain, or a GHE `copilot-api.<domain>` host is rejected.
pub async fn fetch_models(
    client: &HttpClient,
    token: &CopilotToken,
    enterprise_domain: Option<&str>,
) -> Result<Vec<CopilotModel>> {
    let base = assert_trusted_copilot_api_url(&token.api_base_url, enterprise_domain)?;
    fetch_models_from(client, &base, &token.token).await
}

async fn fetch_models_from(
    client: &HttpClient,
    base: &Url,
    token: &str,
) -> Result<Vec<CopilotModel>> {
    const CONTEXT: &str = "GitHub Copilot models request";
    let authorization = format!("Bearer {token}");
    let mut last_error = None;

    for path in MODELS_PATHS {
        let url = base
            .join(path)
            .map_err(|_| GithubCopilotError::UntrustedHost {
                host: base.to_string(),
            })?;
        let response = client
            .get(url)
            .headers(identity_headers())
            .header("authorization", &authorization)
            .header("openai-intent", "conversation-edits")
            .send()
            .await
            .map_err(|source| GithubCopilotError::Transport {
                context: CONTEXT,
                source,
            })?;

        if response.status() == http::StatusCode::NOT_FOUND {
            last_error = Some(GithubCopilotError::HttpStatus {
                context: CONTEXT,
                status: 404,
                body: format!("no models endpoint at /{path}"),
            });
            continue;
        }
        return parse_json_response::<ModelsPayload>(response, CONTEXT)
            .await
            .map(normalize);
    }

    Err(last_error.unwrap_or(GithubCopilotError::InvalidResponse {
        context: CONTEXT,
        detail: "no models endpoint responded".to_string(),
    }))
}

/// Codex-tuned first, then newer GPT-5, avoiding mini/preview.
fn model_score(id: &str) -> f64 {
    let value = id.to_ascii_lowercase();
    let mut score = 0.0;
    if value.contains("codex") {
        score += 1000.0;
    }
    if value.contains("gpt-5") {
        score += 500.0;
    }
    if value.contains("mini") {
        score -= 100.0;
    }
    if value.contains("preview") {
        score -= 20.0;
    }

    for (index, number) in numbers_in(&value).enumerate() {
        score += number / 10f64.powi(index as i32);
    }
    score
}

/// If any entry advertises endpoints, require Responses. If none do, the
/// deployment predates the field, so offer everything.
pub fn usable_models(models: &[CopilotModel]) -> Vec<&CopilotModel> {
    let catalog_declares_endpoints = models.iter().any(|model| model.declares_endpoints);
    models
        .iter()
        .filter(|model| !catalog_declares_endpoints || model.supports_responses)
        .collect()
}

fn numbers_in(value: &str) -> impl Iterator<Item = f64> + '_ {
    let bytes = value.as_bytes();
    let mut index = 0;
    std::iter::from_fn(move || {
        while index < bytes.len() {
            if !bytes[index].is_ascii_digit() {
                index += 1;
                continue;
            }
            let start = index;
            while index < bytes.len() && bytes[index].is_ascii_digit() {
                index += 1;
            }
            if index + 1 < bytes.len() && bytes[index] == b'.' && bytes[index + 1].is_ascii_digit()
            {
                index += 1;
                while index < bytes.len() && bytes[index].is_ascii_digit() {
                    index += 1;
                }
            }
            return value.get(start..index).and_then(|run| run.parse().ok());
        }
        None
    })
}

/// `preferred` wins when offered. Non-Responses models are skipped; picking one
/// writes a profile that fails on its first turn.
pub fn choose_model(models: &[CopilotModel], preferred: Option<&str>) -> Option<String> {
    let usable = usable_models(models);
    if let Some(preferred) = preferred
        && usable.iter().any(|model| model.id == preferred)
    {
        return Some(preferred.to_string());
    }

    usable
        .iter()
        .copied()
        .max_by(|left, right| {
            model_score(&left.id)
                .total_cmp(&model_score(&right.id))
                // Ties resolve to the lexicographically smaller id.
                .then_with(|| right.id.cmp(&left.id))
        })
        .map(|model| model.id.clone())
        .or_else(|| preferred.map(str::to_string))
}

#[cfg(test)]
#[path = "models_tests.rs"]
mod tests;
