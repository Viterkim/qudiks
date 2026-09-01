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
        "support_verbosity": false,
        "default_verbosity": null,
        "apply_patch_tool_type": null,
        "truncation_policy": {"mode": "bytes", "limit": TRUNCATION_LIMIT_BYTES},
        "experimental_supported_tools": [],
        "context_window": model.context_window,
        // Required; Copilot publishes none, so reuse Codex's own prompt.
        "base_instructions": BASE_INSTRUCTIONS,
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
