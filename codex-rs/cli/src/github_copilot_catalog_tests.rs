use super::*;
use codex_protocol::openai_models::ModelsResponse;
use pretty_assertions::assert_eq;

#[test]
fn preserves_model_limits() {
    let grok = CopilotModel {
        id: "grok-4.6".to_string(),
        name: "Grok 4.6".to_string(),
        supports_responses: true,
        declares_endpoints: true,
        context_window: Some(200_000),
        reasoning_efforts: vec!["low".into(), "high".into(), "xhigh".into()],
    };
    let json = build_catalog(&[&grok]).expect("catalog builds");
    let parsed: ModelsResponse = serde_json::from_str(&json).expect("catalog parses");

    let info = &parsed.models[0];
    assert_eq!(info.context_window, Some(200_000));
    assert_eq!(
        info.supported_reasoning_levels
            .iter()
            .map(|preset| preset.effort.to_string())
            .collect::<Vec<_>>(),
        vec!["low", "high", "xhigh"]
    );
}
