use anyhow::Result;
use core_test_support::responses;
use core_test_support::test_codex::test_codex;
use pretty_assertions::assert_eq;

#[tokio::test]
async fn copilot_provider_adds_required_request_headers() -> Result<()> {
    let server = responses::start_mock_server().await;
    let response = responses::mount_sse_once(&server, responses::sse_completed("response-1")).await;
    let test = test_codex()
        .with_config(|config| {
            config.model_provider.name = "GitHub Copilot".to_string();
        })
        .build_with_auto_env(&server)
        .await?;

    test.submit_turn("hello").await?;

    let request = response.single_request();
    assert_eq!(
        request.header("Openai-Intent").as_deref(),
        Some("conversation-edits")
    );
    assert_eq!(request.header("X-Initiator").as_deref(), Some("user"));
    assert_eq!(request.header("Copilot-Vision-Request"), None);
    Ok(())
}
