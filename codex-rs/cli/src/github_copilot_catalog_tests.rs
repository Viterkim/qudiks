use super::*;
use codex_protocol::config_types::ReasoningSummary;
use codex_protocol::openai_models::ApplyPatchToolType;
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
    assert!(info.supports_reasoning_summary_parameter);
    assert_eq!(info.default_reasoning_summary, ReasoningSummary::None);
    assert_eq!(
        info.apply_patch_tool_type,
        Some(ApplyPatchToolType::Function)
    );
}

#[test]
fn selects_a_compatible_native_apply_patch_shape() {
    let gpt = |id: &str| CopilotModel {
        id: id.to_string(),
        name: id.to_string(),
        supports_responses: true,
        declares_endpoints: true,
        context_window: Some(400_000),
        reasoning_efforts: vec!["low".into(), "high".into()],
    };
    let sol = gpt("gpt-5.6-sol");
    let luna = gpt("gpt-5.6-luna");
    let terra = gpt("gpt-5.6-terra");
    let astra = gpt("gpt-6-astra");
    let grok = CopilotModel {
        id: "grok-4.6".to_string(),
        name: "Grok 4.6".to_string(),
        supports_responses: true,
        declares_endpoints: true,
        context_window: Some(328_000),
        reasoning_efforts: vec!["low".into(), "high".into()],
    };
    let json = build_catalog(&[&sol, &luna, &terra, &astra, &grok]).expect("catalog builds");
    let parsed: ModelsResponse = serde_json::from_str(&json).expect("catalog parses");

    assert!(
        parsed.models[..4]
            .iter()
            .all(|model| { model.apply_patch_tool_type == Some(ApplyPatchToolType::Freeform) })
    );
    assert_eq!(
        parsed.models[4].apply_patch_tool_type,
        Some(ApplyPatchToolType::Function)
    );
}
