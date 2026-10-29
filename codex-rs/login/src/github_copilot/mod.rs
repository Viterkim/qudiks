//! GitHub Copilot auth: device flow, token exchange, storage, model discovery.
//!
//! The bearer token reaches the provider through Codex's command-based auth,
//! which runs `qudiks login github-copilot token` and reads stdout.

mod device_flow;
mod endpoints;
mod error;
mod http;
mod models;
mod storage;
mod token;
mod usage;

use crate::default_client::create_raw_auth_client;
use crate::outbound_proxy::AuthRouteConfig;
use codex_http_client::HttpClient;
use std::path::Path;
use std::path::PathBuf;

pub use device_flow::GithubDeviceCode;
pub use endpoints::DEFAULT_COPILOT_API_BASE_URL;
pub use endpoints::DEFAULT_COPILOT_CLIENT_ID;
pub use endpoints::DEFAULT_GITHUB_DOMAIN;
pub use endpoints::assert_trusted_copilot_api_url;
pub use endpoints::normalize_github_domain;
pub use error::GithubCopilotError;
pub use error::Result;
pub use models::CopilotModel;
pub use models::choose_model;
pub use models::usable_models;
pub use storage::GithubCopilotCredentials;
pub use storage::credentials_path;
pub use token::CopilotToken;
pub use usage::CopilotQuotaSnapshot;
pub use usage::fetch_quota_snapshot;
pub use usage::format_quota_snapshot;

use endpoints::GithubEndpoints;

#[derive(Debug, Clone)]
pub struct GithubCopilotLoginOptions {
    pub codex_home: PathBuf,
    pub domain: String,
    pub client_id: String,
}

impl GithubCopilotLoginOptions {
    pub fn new(codex_home: PathBuf) -> Self {
        Self {
            codex_home,
            domain: DEFAULT_GITHUB_DOMAIN.to_string(),
            client_id: DEFAULT_COPILOT_CLIENT_ID.to_string(),
        }
    }
}

fn client_for(endpoint: &str, auth_route_config: &AuthRouteConfig) -> Result<HttpClient> {
    create_raw_auth_client(endpoint, auth_route_config).map_err(|err| {
        GithubCopilotError::ClientBuild {
            endpoint: endpoint.to_string(),
            detail: err.to_string(),
        }
    })
}

/// `on_code` fires once with the verification URL and user code.
pub async fn run_device_login(
    options: &GithubCopilotLoginOptions,
    auth_route_config: &AuthRouteConfig,
    on_code: impl FnOnce(&GithubDeviceCode),
) -> Result<GithubCopilotCredentials> {
    let endpoints = GithubEndpoints::for_domain(&options.domain)?;

    let login_client = client_for(&endpoints.device_code_url, auth_route_config)?;
    let device_code =
        device_flow::request_device_code(&login_client, &endpoints, &options.client_id).await?;
    on_code(&device_code);
    let github_token = device_flow::poll_for_access_token(
        &login_client,
        &endpoints,
        &options.client_id,
        &device_code,
    )
    .await?;

    let token_client = client_for(&endpoints.copilot_token_url, auth_route_config)?;
    let copilot_token =
        token::exchange_github_token(&token_client, &endpoints, &github_token).await?;

    let credentials = GithubCopilotCredentials {
        github_token,
        domain: endpoints.domain,
        client_id: options.client_id.clone(),
        copilot_token: Some(copilot_token),
    };
    storage::save(&options.codex_home, &credentials)?;
    Ok(credentials)
}

/// Always re-exchanges rather than reusing the stored token; command auth
/// caches in-process.
pub async fn bearer_token(
    codex_home: &Path,
    auth_route_config: &AuthRouteConfig,
) -> Result<CopilotToken> {
    let mut credentials = storage::load(codex_home)?.ok_or(GithubCopilotError::NotLoggedIn)?;
    let endpoints = GithubEndpoints::for_domain(&credentials.domain)?;
    let client = client_for(&endpoints.copilot_token_url, auth_route_config)?;
    let refreshed =
        token::exchange_github_token(&client, &endpoints, &credentials.github_token).await?;
    credentials.copilot_token = Some(refreshed.clone());
    storage::save(codex_home, &credentials)?;
    Ok(refreshed)
}

pub async fn discover_models(
    codex_home: &Path,
    auth_route_config: &AuthRouteConfig,
    token: &CopilotToken,
) -> Result<Vec<CopilotModel>> {
    let enterprise = storage::load(codex_home)?
        .map(|credentials| credentials.domain)
        .filter(|domain| domain != DEFAULT_GITHUB_DOMAIN);
    let client = client_for(&token.api_base_url, auth_route_config)?;
    models::fetch_models(&client, token, enterprise.as_deref()).await
}

pub fn stored_credentials(codex_home: &Path) -> Result<Option<GithubCopilotCredentials>> {
    storage::load(codex_home)
}

pub fn logout(codex_home: &Path) -> Result<bool> {
    storage::delete(codex_home)
}
