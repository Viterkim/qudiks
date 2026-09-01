use super::*;
use codex_protocol::openai_models::ModelsResponse;
use pretty_assertions::assert_eq;

#[test]
fn carries_the_real_limits_into_codex_metadata() {
    let grok = CopilotModel {
        id: "grok-4.6".to_string(),
        name: "Grok 4.6".to_string(),
        supports_responses: true,
        declares_endpoints: true,
        context_window: Some(328_000),
        reasoning_efforts: vec!["low".into(), "high".into(), "xhigh".into()],
    };
    let json = build_catalog(&[&grok]).expect("catalog builds");
    let parsed: ModelsResponse = serde_json::from_str(&json).expect("catalog parses");

    let info = &parsed.models[0];
    assert_eq!(info.slug, "grok-4.6");
    assert_eq!(info.context_window, Some(328_000));
    assert_eq!(
        info.supported_reasoning_levels
            .iter()
            .map(|p| p.effort.to_string())
            .collect::<Vec<_>>(),
        vec!["low", "high", "xhigh"]
    );
    assert!(info.auto_compact_token_limit().is_some());
}
