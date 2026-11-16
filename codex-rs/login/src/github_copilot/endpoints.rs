//! GitHub/Copilot endpoints and host trust.

use super::error::GithubCopilotError;
use super::error::Result;
use http::HeaderMap;
use http::HeaderValue;
use url::Url;

pub const DEFAULT_GITHUB_DOMAIN: &str = "github.com";
pub const DEFAULT_COPILOT_API_BASE_URL: &str = "https://api.githubcopilot.com";

// Device flow only works with a client id GitHub ties to Copilot.
pub const DEFAULT_COPILOT_CLIENT_ID: &str = "Iv1.b507a08c87ecfe98";

const COPILOT_USER_AGENT: &str = "GitHubCopilotChat/0.35.0";
const COPILOT_EDITOR_VERSION: &str = "vscode/1.107.0";
const COPILOT_EDITOR_PLUGIN_VERSION: &str = "copilot-chat/0.35.0";
const COPILOT_INTEGRATION_ID: &str = "vscode-chat";

const COPILOT_HOSTED_SUFFIX: &str = ".githubcopilot.com";
const COPILOT_HOSTED_HOST: &str = "api.githubcopilot.com";
const PROXY_ENDPOINT_PREFIX: &str = "proxy-ep=";

/// Resolved GitHub endpoints for a single (possibly enterprise) domain.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GithubEndpoints {
    pub domain: String,
    pub device_code_url: String,
    pub access_token_url: String,
    pub copilot_token_url: String,
    pub copilot_user_url: String,
}

impl GithubEndpoints {
    pub fn for_domain(domain: &str) -> Result<Self> {
        let domain = normalize_github_domain(domain)?;
        Ok(Self {
            device_code_url: format!("https://{domain}/login/device/code"),
            access_token_url: format!("https://{domain}/login/oauth/access_token"),
            copilot_token_url: format!("https://api.{domain}/copilot_internal/v2/token"),
            copilot_user_url: format!("https://api.{domain}/copilot_internal/user"),
            domain,
        })
    }

    // Test-only: point at a local mock server.
    #[cfg(test)]
    pub(super) fn for_mock_server(base_url: &str, domain: &str) -> Self {
        let base_url = base_url.trim_end_matches('/');
        Self {
            domain: domain.to_string(),
            device_code_url: format!("{base_url}/login/device/code"),
            access_token_url: format!("{base_url}/login/oauth/access_token"),
            copilot_token_url: format!("{base_url}/copilot_internal/v2/token"),
            copilot_user_url: format!("{base_url}/copilot_internal/user"),
        }
    }

    // Enterprise widens the host allowlist, so github.com must not opt in.
    pub fn enterprise_domain(&self) -> Option<&str> {
        (self.domain != DEFAULT_GITHUB_DOMAIN).then_some(self.domain.as_str())
    }
}

/// Normalizes a user-supplied GitHub domain to a bare lowercase hostname.
pub fn normalize_github_domain(input: &str) -> Result<String> {
    let trimmed = input.trim();
    let invalid = |reason: &str| GithubCopilotError::InvalidDomain {
        domain: trimmed.to_string(),
        reason: reason.to_string(),
    };
    if trimmed.is_empty() {
        return Err(invalid("domain is empty"));
    }

    let candidate = if trimmed.contains("://") {
        trimmed.to_string()
    } else {
        format!("https://{trimmed}")
    };
    let url = Url::parse(&candidate).map_err(|err| invalid(&err.to_string()))?;

    if url.scheme() != "https" {
        return Err(invalid("GitHub endpoints must use https"));
    }
    if !url.username().is_empty() || url.password().is_some() {
        return Err(invalid("domain must not contain credentials"));
    }
    if url.port().is_some() {
        return Err(invalid("domain must not contain a port"));
    }

    let host = url
        .host_str()
        .ok_or_else(|| invalid("domain has no host"))?
        .trim_end_matches('.')
        .to_ascii_lowercase();
    if host.is_empty()
        || host.contains("..")
        || !host
            .chars()
            .all(|ch| ch.is_ascii_alphanumeric() || ch == '.' || ch == '-')
    {
        return Err(invalid("domain is not a valid hostname"));
    }
    Ok(host)
}

/// GitHub rejects Copilot endpoints without these, even with a valid token.
pub fn identity_headers() -> HeaderMap {
    let mut headers = HeaderMap::new();
    headers.insert("accept", HeaderValue::from_static("application/json"));
    headers.insert("user-agent", HeaderValue::from_static(COPILOT_USER_AGENT));
    headers.insert(
        "editor-version",
        HeaderValue::from_static(COPILOT_EDITOR_VERSION),
    );
    headers.insert(
        "editor-plugin-version",
        HeaderValue::from_static(COPILOT_EDITOR_PLUGIN_VERSION),
    );
    headers.insert(
        "copilot-integration-id",
        HeaderValue::from_static(COPILOT_INTEGRATION_ID),
    );
    headers
}

fn extract_proxy_endpoint(token: &str) -> Option<&str> {
    token
        .split(';')
        .find_map(|part| part.strip_prefix(PROXY_ENDPOINT_PREFIX))
        .map(str::trim)
        .filter(|endpoint| !endpoint.is_empty())
}

/// `proxy-ep=`, then enterprise domain, then the public API. Always validated:
/// the directive comes from the token, so it is attacker-influenced.
pub fn derive_copilot_api_base_url(token: &str, enterprise_domain: Option<&str>) -> Result<String> {
    let Some(proxy_endpoint) = extract_proxy_endpoint(token) else {
        return match enterprise_domain {
            Some(domain) => {
                let domain = normalize_github_domain(domain)?;
                let candidate = format!("https://copilot-api.{domain}");
                assert_trusted_copilot_api_url(&candidate, Some(&domain))?;
                Ok(candidate)
            }
            None => Ok(DEFAULT_COPILOT_API_BASE_URL.to_string()),
        };
    };

    let untrusted = || GithubCopilotError::UntrustedHost {
        host: proxy_endpoint.to_string(),
    };
    let host = if proxy_endpoint.contains("://") {
        let url = Url::parse(proxy_endpoint).map_err(|_| untrusted())?;
        if url.scheme() != "https"
            || !url.username().is_empty()
            || url.password().is_some()
            || url.port().is_some()
            || !matches!(url.path(), "" | "/")
            || url.query().is_some()
            || url.fragment().is_some()
        {
            return Err(untrusted());
        }
        url.host_str().ok_or_else(untrusted)?.to_string()
    } else {
        if proxy_endpoint.contains(['/', '?', '#', '@', ':']) {
            return Err(untrusted());
        }
        proxy_endpoint.to_string()
    };

    let host = host.trim_end_matches('.').to_ascii_lowercase();
    // Copilot advertises `proxy.` but requests must go to `api.`.
    let api_host = match host.strip_prefix("proxy.") {
        Some(rest) => format!("api.{rest}"),
        None if host.starts_with("api.") => host,
        None => format!("api.{host}"),
    };
    let candidate = format!("https://{api_host}");
    assert_trusted_copilot_api_url(&candidate, enterprise_domain)?;
    Ok(candidate)
}

/// Only bare https GitHub-hosted or enterprise-hosted origins pass.
pub fn assert_trusted_copilot_api_url(input: &str, enterprise_domain: Option<&str>) -> Result<Url> {
    let untrusted = || GithubCopilotError::UntrustedHost {
        host: input.to_string(),
    };
    let url = Url::parse(input).map_err(|_| untrusted())?;
    if url.scheme() != "https"
        || !url.username().is_empty()
        || url.password().is_some()
        || url.port().is_some()
        || url.query().is_some()
        || url.fragment().is_some()
        || !matches!(url.path(), "" | "/")
    {
        return Err(untrusted());
    }

    let host = url.host_str().ok_or_else(untrusted)?.to_ascii_lowercase();
    let github_hosted = host == COPILOT_HOSTED_HOST || host.ends_with(COPILOT_HOSTED_SUFFIX);
    let enterprise_hosted = match enterprise_domain {
        Some(domain) => host == format!("copilot-api.{}", normalize_github_domain(domain)?),
        None => false,
    };
    if !github_hosted && !enterprise_hosted {
        return Err(untrusted());
    }
    Ok(url)
}

#[cfg(test)]
#[path = "endpoints_tests.rs"]
mod tests;
