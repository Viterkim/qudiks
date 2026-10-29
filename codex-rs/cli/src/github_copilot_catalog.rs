//! Translates Copilot's `/models` for `model_catalog_json`, bypassing Codex's
//! incompatible catalog refresh.

use codex_login::github_copilot::CopilotModel;
use codex_models_manager::bundled_models_response;
use codex_models_manager::model_info::BASE_INSTRUCTIONS;
use codex_protocol::openai_models::ModelInfo;
use codex_protocol::openai_models::ModelsResponse;
use codex_protocol::openai_models::default_input_modalities;
use serde_json::Value;
use serde_json::json;
use std::path::Path;
use std::path::PathBuf;

const CATALOG_FILE: &str = "copilot-models.json";

const TRUNCATION_LIMIT_BYTES: i64 = 10_000;
const QUDIKS_SETUP: &str = r#"You are running in Qudiks, a local fork of the open-source Codex CLI. GitHub Copilot serves the selected model. Qudiks runs the tools and manages sessions.

The Codex instructions below apply to this fork. Use `qudiks` for CLI commands. Configuration, credentials, skills and sessions live in the active Qudiks home, normally ~/.qudiks. Honor runtime overrides and leave a separate ~/.codex installation alone unless the task involves it.

Use this session's tools and schemas. Hosted Codex features, OpenAI account details and ChatGPT usage limits do not describe this Copilot session."#;
const COMPAT_BASE_INSTRUCTIONS: &str = r#"You are a coding agent running in the Qudiks CLI, a terminal-based coding assistant. You are expected to be precise, safe, and helpful.

# How you work

Follow the user's request and applicable AGENTS.md instructions. Keep your tone concise and direct. Give brief progress updates during longer work, then continue with the next action. Do not repeat an update when nothing has changed.

# Task execution

Keep going until the requested work is complete. Read nearby code, make the smallest useful change, and fix errors you find in that change. Do not claim an edit, test, or result before it happens. Use a tool result when it answers the question instead of repeating the same read. If work is authorized, do it without asking for confirmation.

# Tools

Use the tools available in this session to read files, edit code, and run commands. Read a smaller range if output is truncated. Report tool failures plainly.

The exec wrapper takes JavaScript. Run shell commands through tools.exec_command, for example: text(await tools.exec_command({cmd: "pwd"})). A bare await does not display its result. Emit results with text(), never console.log. Forward images with image(result.image_url). Await independent calls together with Promise.all or Promise.allSettled. Inspect returned results before repeating a call. Use tools.apply_patch for file edits and read an unfamiliar tool's description before calling it.

# Validating and presenting work

Run focused checks that cover the change. In the final answer, say what changed, how it was checked, and what remains. Do not present a plan as completed work."#;

pub fn catalog_path(codex_home: &Path) -> PathBuf {
    codex_home.join(CATALOG_FILE)
}

fn reasoning_presets(model: &CopilotModel, bundled: Option<&ModelInfo>) -> Vec<Value> {
    model
        .reasoning_efforts
        .iter()
        .map(|effort| {
            let description = bundled
                .and_then(|info| {
                    info.supported_reasoning_levels
                        .iter()
                        .find(|preset| preset.effort.to_string() == *effort)
                })
                .map_or("", |preset| preset.description.as_str());
            json!({"effort": effort, "description": description})
        })
        .collect()
}

/// Use JSON patches for providers without native freeform tools.
fn apply_patch_tool_type(model: &CopilotModel) -> Option<&'static str> {
    if model.id.starts_with("gpt-5.3-")
        || model.id.starts_with("gpt-5.4-")
        || model.id.starts_with("gpt-5.6-")
        || model.id.starts_with("gpt-6-")
        || model.id.starts_with("gpt-6.")
    {
        Some("freeform")
    } else if model.id.starts_with("grok")
        || model.id.starts_with("kimi-")
        || matches!(model.id.as_str(), "claude-sonnet-5.5" | "claude-opus-5.5")
    {
        Some("function")
    } else {
        None
    }
}

fn base_instructions(model: &CopilotModel, bundled: Option<&ModelInfo>) -> String {
    let template = if model.id.starts_with("grok")
        || model.id.starts_with("kimi-")
        || matches!(model.id.as_str(), "claude-sonnet-5.5" | "claude-opus-5.5")
    {
        COMPAT_BASE_INSTRUCTIONS
    } else if let Some(template) = bundled
        .and_then(|info| info.model_messages.as_ref())
        .and_then(|messages| messages.instructions_template.as_deref())
    {
        template
    } else {
        BASE_INSTRUCTIONS
    };
    let template = template
        .replace("You are Codex,", "You are Qudiks,")
        .replace("As Codex,", "As Qudiks,");

    format!("{QUDIKS_SETUP}\n\n{template}")
}

fn model_entry(model: &CopilotModel, priority: i32, bundled: Option<&ModelInfo>) -> Value {
    let instructions = base_instructions(model, bundled);
    let model_messages =
        bundled
            .and_then(|info| info.model_messages.clone())
            .map(|mut messages| {
                messages.instructions_template = Some(instructions.clone());
                messages
            });
    let default_reasoning = if model.id == "gpt-6.1-sol" || model.id.starts_with("kimi-") {
        Some("high".to_string())
    } else {
        bundled
            .and_then(|info| info.default_reasoning_level.as_ref())
            .map(ToString::to_string)
    }
    .filter(|effort| model.reasoning_efforts.contains(effort))
    .or_else(|| {
        ["medium", "low", "none"]
            .into_iter()
            .find(|effort| {
                model
                    .reasoning_efforts
                    .iter()
                    .any(|supported| supported == effort)
            })
            .map(str::to_string)
    })
    .or_else(|| model.reasoning_efforts.first().cloned());

    json!({
        "slug": model.id,
        "display_name": model.name,
        "description": null,
        "default_reasoning_level": default_reasoning,
        "supported_reasoning_levels": reasoning_presets(model, bundled),
        "shell_type": "unified_exec",
        "visibility": "list",
        "supported_in_api": true,
        "priority": priority,
        "availability_nux": null,
        "upgrade": null,
        // Summaries can flood the transcript, so leave them opt-in.
        "supports_reasoning_summary_parameter": true,
        "default_reasoning_summary": "none",
        "support_verbosity": false,
        "default_verbosity": null,
        "apply_patch_tool_type": apply_patch_tool_type(model),
        "truncation_policy": bundled.map_or_else(
            || json!({"mode": "bytes", "limit": TRUNCATION_LIMIT_BYTES}),
            |info| json!(info.truncation_policy),
        ),
        "tool_mode": if model.id.starts_with("kimi-") || matches!(model.id.as_str(), "claude-sonnet-5.5" | "claude-opus-5.5") { Some(codex_protocol::openai_models::ToolMode::CodeModeOnly) } else { bundled.and_then(|info| info.tool_mode) },
        "multi_agent_version": bundled.and_then(|info| info.multi_agent_version),
        "multi_agent_reasoning_effort": bundled.and_then(|info| info.multi_agent_reasoning_effort.clone()),
        "experimental_supported_tools": bundled
            .map(|info| info.experimental_supported_tools.clone())
            .unwrap_or_default(),
        "input_modalities": bundled
            .map(|info| info.input_modalities.clone())
            .unwrap_or_else(default_input_modalities),
        "include_skills_usage_instructions": bundled.is_some_and(|info| info.include_skills_usage_instructions),
        "include_apps_usage_instructions": bundled.is_none_or(|info| info.include_apps_usage_instructions),
        "include_plugin_usage_instructions": bundled.is_some_and(|info| info.include_plugin_usage_instructions),
        "context_window": model.context_window,
        "max_context_window": model.context_window,
        "model_messages": model_messages,
        "base_instructions": instructions,
    })
}

/// Validates against Codex's schema so a mismatch cannot silently restore fallback metadata.
pub fn build_catalog(models: &[&CopilotModel]) -> anyhow::Result<String> {
    let bundled = bundled_models_response()?;
    let entries: Vec<Value> = models
        .iter()
        .enumerate()
        .map(|(index, model)| {
            let template = bundled.models.iter().find(|info| info.slug == model.id);
            model_entry(
                model,
                i32::try_from(models.len() - index).unwrap_or(0),
                template,
            )
        })
        .collect();
    let catalog = json!({"models": entries});

    serde_json::from_value::<ModelsResponse>(catalog.clone())
        .map_err(|err| anyhow::anyhow!("built an invalid model catalog: {err}"))?;
    Ok(serde_json::to_string_pretty(&catalog)?)
}

pub fn write_catalog(codex_home: &Path, models: &[&CopilotModel]) -> anyhow::Result<PathBuf> {
    let path = catalog_path(codex_home);
    std::fs::write(&path, build_catalog(models)?)?;
    Ok(path)
}

#[cfg(test)]
#[path = "github_copilot_catalog_tests.rs"]
mod tests;
