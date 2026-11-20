//! Copilot remaining-quota snapshot from `copilot_internal/user`.

use super::endpoints::GithubEndpoints;
use super::endpoints::identity_headers;
use super::error::GithubCopilotError;
use super::error::Result;
use super::http::parse_json_response;
use crate::default_client::create_raw_auth_client;
use crate::outbound_proxy::AuthRouteConfig;
use serde::Deserialize;
use std::path::Path;
use std::path::PathBuf;
use std::time::Duration;
use std::time::Instant;
use tokio::sync::Mutex;

const QUOTA_CACHE_TTL: Duration = Duration::from_secs(5 * 60);
static QUOTA_CACHE: Mutex<Option<CachedQuota>> = Mutex::const_new(None);

struct CachedQuota {
    home: PathBuf,
    domain: String,
    github_token: String,
    fetched_at: Instant,
    snapshot: CopilotQuotaSnapshot,
}

#[derive(Debug, Clone, PartialEq)]
pub struct CopilotQuotaSnapshot {
    pub plan: Option<String>,
    pub reset_date: Option<String>,
    pub credits_used: Option<i64>,
    pub entitlement: Option<i64>,
    pub remaining: Option<i64>,
    pub unlimited: bool,
}

#[derive(Deserialize, Default)]
struct UserPayload {
    copilot_plan: Option<String>,
    quota_reset_date: Option<String>,
    #[serde(default)]
    quota_snapshots: QuotaSnapshots,
}

#[derive(Deserialize, Default)]
struct QuotaSnapshots {
    premium_interactions: Option<QuotaWindow>,
}

#[derive(Deserialize, Default)]
struct QuotaWindow {
    credits_used: Option<i64>,
    entitlement: Option<i64>,
    remaining: Option<i64>,
    #[serde(default)]
    unlimited: bool,
}

pub async fn fetch_quota_snapshot(
    codex_home: &Path,
    auth_route_config: &AuthRouteConfig,
) -> Result<CopilotQuotaSnapshot> {
    let credentials = super::storage::load(codex_home)?.ok_or(GithubCopilotError::NotLoggedIn)?;
    let mut cache = QUOTA_CACHE.lock().await;
    if let Some(cached) = cache.as_ref()
        && cached.home == codex_home
        && cached.domain == credentials.domain
        && cached.github_token == credentials.github_token
        && cached.fetched_at.elapsed() < QUOTA_CACHE_TTL
    {
        return Ok(cached.snapshot.clone());
    }
    let endpoints = GithubEndpoints::for_domain(&credentials.domain)?;
    let client =
        create_raw_auth_client(&endpoints.copilot_user_url, auth_route_config).map_err(|err| {
            GithubCopilotError::ClientBuild {
                endpoint: endpoints.copilot_user_url.clone(),
                detail: err.to_string(),
            }
        })?;
    const CONTEXT: &str = "GitHub Copilot usage request";
    let authorization = format!("Bearer {}", credentials.github_token);
    let response = client
        .get(&endpoints.copilot_user_url)
        .headers(identity_headers())
        .header("authorization", authorization)
        .timeout(Duration::from_secs(/*secs*/ 20))
        .send()
        .await
        .map_err(|source| GithubCopilotError::Transport {
            context: CONTEXT,
            source,
        })?;
    let payload: UserPayload = parse_json_response(response, CONTEXT).await?;
    let snapshot = snapshot_from_payload(payload);
    *cache = Some(CachedQuota {
        home: codex_home.to_path_buf(),
        domain: credentials.domain,
        github_token: credentials.github_token,
        fetched_at: Instant::now(),
        snapshot: snapshot.clone(),
    });
    Ok(snapshot)
}

fn snapshot_from_payload(payload: UserPayload) -> CopilotQuotaSnapshot {
    let window = payload
        .quota_snapshots
        .premium_interactions
        .unwrap_or_default();
    CopilotQuotaSnapshot {
        plan: payload.copilot_plan,
        reset_date: payload.quota_reset_date,
        credits_used: window.credits_used.or_else(|| {
            let entitlement = window.entitlement?;
            let remaining = window.remaining?;
            (entitlement > 0).then_some(entitlement.saturating_sub(remaining).max(0))
        }),
        entitlement: window.entitlement,
        remaining: window.remaining,
        unlimited: window.unlimited && window.entitlement.is_none_or(|value| value <= 0),
    }
}

pub fn format_quota_snapshot(snapshot: &CopilotQuotaSnapshot) -> String {
    let mut lines = Vec::new();
    lines.push("GitHub Copilot usage".to_string());
    if let Some(plan) = snapshot.plan.as_deref() {
        lines.push(format!("Plan: {plan}"));
    }
    if snapshot.unlimited {
        lines.push("Usage this cycle: unlimited".to_string());
    } else if let (Some(used), Some(entitlement)) = (snapshot.credits_used, snapshot.entitlement) {
        lines.push(format!(
            "Usage this cycle: {} / {} AI credits",
            format_credits(used),
            format_credits(entitlement)
        ));
    } else if let Some(remaining) = snapshot.remaining {
        lines.push(format!(
            "Remaining this cycle: {} AI credits",
            format_credits(remaining)
        ));
    } else {
        lines.push("Usage this cycle: unavailable".to_string());
    }
    if let Some(reset) = snapshot.reset_date.as_deref() {
        lines.push(format!("Resets on {reset}"));
    }
    lines.join("\n")
}

fn format_credits(value: i64) -> String {
    let digits = value.abs().to_string();
    let mut grouped = String::new();
    for (index, ch) in digits.chars().rev().enumerate() {
        if index > 0 && index % 3 == 0 {
            grouped.push(',');
        }
        grouped.push(ch);
    }
    let grouped: String = grouped.chars().rev().collect();
    if value < 0 {
        format!("-{grouped}")
    } else {
        grouped
    }
}

#[cfg(test)]
#[path = "usage_tests.rs"]
mod tests;
