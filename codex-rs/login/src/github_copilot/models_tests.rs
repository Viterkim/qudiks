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

fn model(id: &str) -> CopilotModel {
    CopilotModel {
        id: id.to_string(),
        name: id.to_string(),
        supports_responses: true,
        declares_endpoints: true,
        context_window: None,
        reasoning_efforts: Vec::new(),
    }
}

fn chat_only(id: &str) -> CopilotModel {
    CopilotModel {
        supports_responses: false,
        ..model(id)
    }
}

fn undeclared(id: &str) -> CopilotModel {
    CopilotModel {
        supports_responses: false,
        declares_endpoints: false,
        ..model(id)
    }
}

async fn fetch_from(server: &MockServer) -> Result<Vec<CopilotModel>> {
    let base = Url::parse(&server.uri()).expect("mock url");
    fetch_models_from(&create_client(), &base, "tid=abc").await
}

async fn mount_models(server: &MockServer, route: &str, body: serde_json::Value) {
    Mock::given(method("GET"))
        .and(path(route))
        .respond_with(ResponseTemplate::new(200).set_body_json(body))
        .mount(server)
        .await;
}

#[test]
fn ranks_codex_tuned_and_newer_models_first() {
    let models = [
        model("gpt-4o"),
        model("gpt-5.1-codex"),
        model("gpt-5.3-codex"),
        model("gpt-5-codex-mini"),
    ];
    assert_eq!(
        choose_model(&models, /*preferred*/ None),
        Some("gpt-5.3-codex".to_string())
    );
}

#[test]
fn honors_a_preferred_model_only_when_offered() {
    let models = [model("gpt-5.3-codex"), model("gpt-4o")];
    assert_eq!(
        choose_model(&models, Some("gpt-4o")),
        Some("gpt-4o".to_string())
    );
    assert_eq!(
        choose_model(&models, Some("o3")),
        Some("gpt-5.3-codex".to_string())
    );
}

#[test]
fn never_picks_a_model_that_cannot_serve_responses() {
    let models = [
        chat_only("claude-opus-5"),
        undeclared("text-embedding-3-small"),
        model("grok-4.6"),
    ];
    assert_eq!(
        choose_model(&models, /*preferred*/ None),
        Some("grok-4.6".to_string())
    );
    assert_eq!(choose_model(&[chat_only("claude-opus-5")], None), None);
    // An explicit --model must not select a chat-only entry either.
    assert_eq!(
        choose_model(&models, Some("claude-opus-5")),
        Some("grok-4.6".to_string())
    );
}

#[test]
fn offers_everything_when_no_entry_declares_endpoints() {
    let models = [undeclared("a"), undeclared("b")];
    assert_eq!(usable_models(&models).len(), 2);
}

#[tokio::test]
async fn reads_the_real_catalog_shape() {
    skip_if_no_network!();

    let server = MockServer::start().await;
    mount_models(
        &server,
        "/models",
        json!({"data": [
            {
                "id": "grok-4.6",
                "supported_endpoints": ["/responses", "ws:/responses"],
                "capabilities": {
                    "limits": {"max_context_window_tokens": 328000},
                    "supports": {"reasoning_effort": ["low", "high", "xhigh"]}
                }
            },
            {"id": "claude-opus-5", "supported_endpoints": ["/v1/messages"]},
            {"id": "claude-opus-5", "supported_endpoints": ["/v1/messages"]},
            {"id": ""},
        ]}),
    )
    .await;

    let models = fetch_from(&server).await.expect("fetch");

    assert_eq!(models.len(), 2);
    assert_eq!(models[0].context_window, Some(328_000));
    assert_eq!(models[0].reasoning_efforts, ["low", "high", "xhigh"]);
    assert_eq!(
        usable_models(&models)
            .iter()
            .map(|m| m.id.as_str())
            .collect::<Vec<_>>(),
        vec!["grok-4.6"]
    );
}

#[tokio::test]
async fn falls_back_to_the_v1_models_route() {
    skip_if_no_network!();

    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path("/models"))
        .respond_with(ResponseTemplate::new(404))
        .mount(&server)
        .await;
    mount_models(&server, "/v1/models", json!(["gpt-5.3-codex"])).await;

    assert_eq!(
        fetch_from(&server).await.expect("fetch"),
        vec![undeclared("gpt-5.3-codex")]
    );
}

#[tokio::test]
async fn refuses_to_query_an_untrusted_api_host() {
    let token = CopilotToken {
        token: "tid=abc".to_string(),
        expires_at: chrono::DateTime::from_timestamp(1_893_456_000, 0).expect("timestamp"),
        api_base_url: "https://evil.example.com".to_string(),
    };
    let error = fetch_models(&create_client(), &token, /*enterprise_domain*/ None)
        .await
        .expect_err("should fail");
    assert!(matches!(error, GithubCopilotError::UntrustedHost { .. }));
}

#[tokio::test]
async fn accepts_an_enterprise_host_when_the_domain_is_known() {
    let token = CopilotToken {
        token: "tid=abc".to_string(),
        expires_at: chrono::DateTime::from_timestamp(1_893_456_000, 0).expect("timestamp"),
        api_base_url: "https://copilot-api.github.example.com".to_string(),
    };
    // Rejected without the domain, so discovery must pass the signed-in one.
    assert!(
        fetch_models(&create_client(), &token, /*enterprise_domain*/ None)
            .await
            .is_err()
    );
    let error = fetch_models(&create_client(), &token, Some("github.example.com"))
        .await
        .expect_err("host is trusted, so this fails on connection instead");
    assert!(!matches!(error, GithubCopilotError::UntrustedHost { .. }));
}
