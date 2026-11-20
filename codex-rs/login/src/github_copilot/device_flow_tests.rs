use super::*;
use crate::default_client::create_client;
use core_test_support::skip_if_no_network;
use pretty_assertions::assert_eq;
use serde_json::json;
use wiremock::Mock;
use wiremock::MockServer;
use wiremock::ResponseTemplate;
use wiremock::matchers::method;
use wiremock::matchers::path;

const CLIENT_ID: &str = "Iv1.test";

fn endpoints_for(server: &MockServer) -> GithubEndpoints {
    GithubEndpoints::for_mock_server(&server.uri(), "github.com")
}

#[tokio::test]
async fn clamps_the_poll_interval_and_wait_window() {
    skip_if_no_network!();

    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/login/device/code"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "device_code": "dc",
            "user_code": "ABCD-1234",
            "verification_uri": "https://github.com/login/device",
            "expires_in": 86_400,
            "interval": 0,
        })))
        .mount(&server)
        .await;

    let code = request_device_code(&create_client(), &endpoints_for(&server), CLIENT_ID)
        .await
        .expect("device code");

    assert_eq!(code.interval, MIN_POLL_INTERVAL);
    assert_eq!(code.expires_in, MAX_DEVICE_FLOW_WAIT);
}

fn pending() -> GithubDeviceCode {
    GithubDeviceCode {
        device_code: "dc".to_string(),
        user_code: "ABCD-1234".to_string(),
        verification_uri: "https://github.com/login/device".to_string(),
        interval: MIN_POLL_INTERVAL,
        expires_in: MAX_DEVICE_FLOW_WAIT,
    }
}

async fn mount_replies(server: &MockServer, replies: Vec<serde_json::Value>) {
    for reply in replies {
        Mock::given(method("POST"))
            .and(path("/login/oauth/access_token"))
            .respond_with(ResponseTemplate::new(200).set_body_json(reply))
            .up_to_n_times(1)
            .mount(server)
            .await;
    }
}

#[tokio::test(start_paused = true)]
async fn polls_through_pending_and_slow_down_until_approved() {
    skip_if_no_network!();

    let server = MockServer::start().await;
    mount_replies(
        &server,
        vec![
            json!({"error": "authorization_pending"}),
            json!({"error": "slow_down", "interval": 10}),
            json!({"access_token": "gho_token"}),
        ],
    )
    .await;

    let token = poll_for_access_token(
        &create_client(),
        &endpoints_for(&server),
        CLIENT_ID,
        &pending(),
    )
    .await
    .expect("poll");

    assert_eq!(token, "gho_token");
}

#[tokio::test(start_paused = true)]
async fn reports_a_denied_login() {
    skip_if_no_network!();

    let server = MockServer::start().await;
    mount_replies(&server, vec![json!({"error": "access_denied"})]).await;

    let error = poll_for_access_token(
        &create_client(),
        &endpoints_for(&server),
        CLIENT_ID,
        &pending(),
    )
    .await
    .expect_err("should fail");

    assert!(matches!(error, GithubCopilotError::LoginDenied));
}
