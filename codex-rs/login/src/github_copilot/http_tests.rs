use super::*;
use core_test_support::skip_if_no_network;
use pretty_assertions::assert_eq;
use wiremock::Mock;
use wiremock::MockServer;
use wiremock::ResponseTemplate;
use wiremock::matchers::method;

#[tokio::test]
async fn truncates_an_oversized_error_body() {
    skip_if_no_network!();

    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .respond_with(ResponseTemplate::new(500).set_body_string("x".repeat(2_000)))
        .mount(&server)
        .await;
    let response = crate::default_client::create_client()
        .get(server.uri())
        .send()
        .await
        .expect("send");

    let error = parse_json_response::<serde_json::Value>(response, "test")
        .await
        .expect_err("should fail");

    let GithubCopilotError::HttpStatus { body, .. } = error else {
        panic!("expected an HTTP status error");
    };
    assert_eq!(body, format!("{}…", "x".repeat(MAX_BODY_SNIPPET)));
}
