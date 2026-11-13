use super::*;
use codex_protocol::openai_models::ApplyPatchToolType;
use codex_protocol::openai_models::ModelsResponse;
use codex_protocol::openai_models::ReasoningEffort;
use codex_protocol::openai_models::ToolMode;
use pretty_assertions::assert_eq;

#[test]
fn preserves_model_limits() {
    let grok = CopilotModel {
        id: "grok-4.6".to_string(),
        name: "Grok 4.6".to_string(),
        supports_responses: true,
        supports_chat_completions: false,
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

#[test]
fn unbundled_model_gets_agent_instructions() {
    let model = CopilotModel {
        id: "gemini-3.8-flash".into(),
        name: "Gemini 3.8 Flash".into(),
        supports_responses: false,
        supports_chat_completions: true,
        declares_endpoints: true,
        context_window: Some(200_000),
        reasoning_efforts: vec!["low".into(), "medium".into(), "high".into()],
    };
    let catalog: ModelsResponse = serde_json::from_str(&build_catalog(&[&model]).unwrap()).unwrap();
    let info = &catalog.models[0];

    assert_eq!(
        (
            info.default_reasoning_level.clone(),
            info.apply_patch_tool_type.clone(),
            info.tool_mode,
            info.include_skills_usage_instructions,
            info.include_plugin_usage_instructions,
        ),
        (
            Some(ReasoningEffort::High),
            Some(ApplyPatchToolType::Function),
            Some(ToolMode::CodeModeOnly),
            true,
            true,
        )
    );
    assert!(
        info.model_messages
            .as_ref()
            .and_then(|messages| messages.instructions_template.as_deref())
            .is_some_and(|instructions| instructions.contains(COMPAT_BASE_INSTRUCTIONS))
    );
}
