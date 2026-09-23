//! Translates Copilot's `/models` into a Codex catalog.
//!
//! Codex wants `{"models":[...]}`, Copilot answers `{"data":[...]}`, so refresh
//! fails and every turn guesses metadata. Pointing `model_catalog_json` at the
//! translation also switches Codex to a static models manager.

use codex_login::github_copilot::CopilotModel;
use codex_models_manager::model_info::BASE_INSTRUCTIONS;
use codex_protocol::openai_models::ModelsResponse;
use serde_json::Value;
use serde_json::json;
use std::path::Path;
use std::path::PathBuf;

const CATALOG_FILE: &str = "copilot-models.json";

const TRUNCATION_LIMIT_BYTES: i64 = 10_000;
const GROK_BASE_INSTRUCTIONS: &str = r#"You are a coding agent running in the Qudiks CLI, a terminal-based coding assistant. You are expected to be precise, safe, and helpful.

# How you work

Follow the user's request and applicable AGENTS.md instructions. Keep your tone concise and direct. Give brief progress updates during longer work, then continue with the next action. Do not repeat an update when nothing has changed.

# Task execution

Keep going until the requested work is complete. Read nearby code, make the smallest useful change, and fix errors you find in that change. Do not claim an edit, test, or result before it happens. Use a tool result when it answers the question instead of repeating the same read. If work is authorized, do it without asking for confirmation.

# Tools

Use the tools available in this session to read files, edit code, and run commands. Read a smaller range if output is truncated. Report tool failures plainly.

# Validating and presenting work

Run focused checks that cover the change. In the final answer, say what changed, how it was checked, and what remains. Do not present a plan as completed work."#;

pub fn catalog_path(codex_home: &Path) -> PathBuf {
    codex_home.join(CATALOG_FILE)
}

fn reasoning_presets(model: &CopilotModel) -> Vec<Value> {
    model
        .reasoning_efforts
        .iter()
        .map(|effort| json!({"effort": effort, "description": ""}))
        .collect()
}

fn apply_patch_tool_type(model: &CopilotModel) -> Option<&'static str> {
    if model.id.starts_with("gpt-5.6-") || model.id.starts_with("gpt-6-") {
        Some("freeform")
    } else if model.id.starts_with("grok") {
        Some("function")
    } else {
        None
    }
}

fn base_instructions(model: &CopilotModel) -> String {
    if model.id.starts_with("grok") {
        GROK_BASE_INSTRUCTIONS.to_string()
    } else {
        BASE_INSTRUCTIONS.to_string()
    }
}

/// Only fields with no serde default, so upstream additions need no change here.
fn model_entry(model: &CopilotModel, priority: i32) -> Value {
    json!({
        "slug": model.id,
        "display_name": model.name,
        "description": null,
        "supported_reasoning_levels": reasoning_presets(model),
        "shell_type": "unified_exec",
        "visibility": "list",
        "supported_in_api": true,
        "priority": priority,
        "availability_nux": null,
        "upgrade": null,
        // Copilot models can emit very large summary_text blocks that Codex
        // renders as transcript reasoning. Keep summaries available as an
        // explicit opt-in, but do not request them by default.
        "supports_reasoning_summary_parameter": true,
        "default_reasoning_summary": "none",
        "support_verbosity": false,
        "default_verbosity": null,
        // Modern GPT models accept Responses custom/freeform tools. Grok rejects
        // that shape with 422, but accepts ordinary JSON function tools.
        "apply_patch_tool_type": apply_patch_tool_type(model),
        "truncation_policy": {"mode": "bytes", "limit": TRUNCATION_LIMIT_BYTES},
        "experimental_supported_tools": [],
        "context_window": model.context_window,
        // Required; Copilot publishes none, so reuse Codex's own prompt.
        "base_instructions": base_instructions(model),
    })
}

/// Validated before returning so an upstream schema change fails loudly
/// instead of silently restoring fallback metadata.
pub fn build_catalog(models: &[&CopilotModel]) -> anyhow::Result<String> {
    let entries: Vec<Value> = models
        .iter()
        .enumerate()
        .map(|(index, model)| model_entry(model, i32::try_from(models.len() - index).unwrap_or(0)))
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
