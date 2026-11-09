use super::super::endpoints::DEFAULT_COPILOT_API_BASE_URL;
use super::*;
use crate::default_client::create_client;
use chrono::TimeDelta;
use core_test_support::skip_if_no_network;
use pretty_assertions::assert_eq;
use serde_json::json;
use wiremock::Mock;
use wiremock::MockServer;
use wiremock::ResponseTemplate;
use wiremock::matchers::method;
use wiremock::matchers::path;

async fn exchange(body: serde_json::Value) -> Result<CopilotToken> {
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path("/copilot_internal/v2/token"))
        .respond_with(ResponseTemplate::new(200).set_body_json(body))
        .mount(&server)
        .await;
    exchange_github_token(
        &create_client(),
        &GithubEndpoints::for_mock_server(&server.uri(), "github.com"),
        "gho_token",
    )
    .await
}

#[tokio::test]
async fn honors_the_proxy_endpoint_advertised_by_the_token() {
    skip_if_no_network!();

    let token = exchange(json!({
        "token": "tid=abc;proxy-ep=proxy.enterprise.githubcopilot.com",
        "expires_at": 1_893_456_000_i64,
    }))
    .await
    .expect("exchange");

    assert_eq!(
        token.api_base_url,
        "https://api.enterprise.githubcopilot.com"
    );
}

#[tokio::test]
async fn refuses_a_token_that_redirects_to_an_untrusted_host() {
    skip_if_no_network!();

    let error = exchange(json!({
        "token": "tid=abc;proxy-ep=evil.example.com",
        "expires_at": 1_893_456_000_i64,
    }))
    .await
    .expect_err("should fail");

    assert!(matches!(error, GithubCopilotError::UntrustedHost { .. }));
}

#[test]
fn treats_a_token_inside_the_refresh_buffer_as_stale() {
    let now = DateTime::from_timestamp(1_700_000_000, 0).expect("timestamp");
    let at = |offset: i64| CopilotToken {
        token: "tid=abc".to_string(),
        expires_at: now + TimeDelta::seconds(offset),
        api_base_url: DEFAULT_COPILOT_API_BASE_URL.to_string(),
    };

    assert!(at(TOKEN_REFRESH_BUFFER_SECONDS + 1).is_fresh_at(now));
    assert!(!at(TOKEN_REFRESH_BUFFER_SECONDS).is_fresh_at(now));
}
