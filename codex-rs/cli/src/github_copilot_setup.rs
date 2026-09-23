//! Writes the Copilot provider and profile into `CODEX_HOME`. Edits are
//! in-place, so re-running touches only these keys.

use codex_config::ProfileV2Name;
use codex_core::config::edit::ConfigEdit;
use codex_core::config::edit::ConfigEditsBuilder;
use codex_model_provider::GITHUB_COPILOT_PROVIDER_NAME;
use std::path::Path;
use std::path::PathBuf;
use toml_edit::Array;
use toml_edit::Item as TomlItem;
use toml_edit::value;

pub const COPILOT_PROVIDER_ID: &str = "github-copilot";

const TOKEN_COMMAND_ARGS: [&str; 3] = ["login", "github-copilot", "token"];

// Copilot answers 400, not 401, for a bad token, so Codex's refresh-on-401
// never fires. Proactive refresh is the only protection (24h token).
const TOKEN_REFRESH_INTERVAL_MS: i64 = 60 * 60 * 1000;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CopilotProfileSettings {
    pub base_url: String,
    pub model: Option<String>,
    pub token_command: PathBuf,
    pub model_catalog_json: Option<PathBuf>,
}

/// Base config when no profile is named, else the file `-p <profile>` layers.
pub fn profile_config_path(codex_home: &Path, profile: Option<&ProfileV2Name>) -> PathBuf {
    match profile {
        Some(profile) => codex_home.join(format!("{profile}.config.toml")),
        None => codex_home.join("config.toml"),
    }
}

fn provider_path(leaf: &str) -> Vec<String> {
    vec![
        "model_providers".to_string(),
        COPILOT_PROVIDER_ID.to_string(),
        leaf.to_string(),
    ]
}

fn auth_path(leaf: &str) -> Vec<String> {
    vec![
        "model_providers".to_string(),
        COPILOT_PROVIDER_ID.to_string(),
        "auth".to_string(),
        leaf.to_string(),
    ]
}

pub fn copilot_profile_edits(settings: &CopilotProfileSettings) -> Vec<ConfigEdit> {
    let token_args: Array = TOKEN_COMMAND_ARGS.iter().copied().collect();
    let mut edits = vec![
        ConfigEdit::SetPath {
            segments: vec!["model_provider".to_string()],
            value: value(COPILOT_PROVIDER_ID),
        },
        ConfigEdit::SetPath {
            segments: provider_path("name"),
            value: value(GITHUB_COPILOT_PROVIDER_NAME),
        },
        ConfigEdit::SetPath {
            segments: provider_path("base_url"),
            value: value(settings.base_url.as_str()),
        },
        ConfigEdit::SetPath {
            segments: provider_path("wire_api"),
            value: value("responses"),
        },
        ConfigEdit::SetPath {
            segments: provider_path("supports_standalone_web_search"),
            value: value(true),
        },
        ConfigEdit::SetPath {
            segments: auth_path("command"),
            value: value(settings.token_command.to_string_lossy().into_owned()),
        },
        ConfigEdit::SetPath {
            segments: auth_path("args"),
            value: TomlItem::Value(token_args.into()),
        },
        ConfigEdit::SetPath {
            segments: auth_path("refresh_interval_ms"),
            value: value(TOKEN_REFRESH_INTERVAL_MS),
        },
        ConfigEdit::SetPath {
            segments: vec!["features".to_string(), "daemon_auto_start".to_string()],
            value: value(false),
        },
    ];

    // Leave an existing `model` alone rather than clearing a working choice.
    if let Some(model) = settings.model.as_deref() {
        edits.push(ConfigEdit::SetPath {
            segments: vec!["model".to_string()],
            value: value(model),
        });
    }
    edits.push(ConfigEdit::SetPath {
        segments: vec!["model_reasoning_effort".to_string()],
        value: value(if settings.model.as_deref() == Some("grok-4.7") {
            "high"
        } else {
            "medium"
        }),
    });
    edits.push(ConfigEdit::SetPath {
        segments: vec!["model_reasoning_summary".to_string()],
        value: value("none"),
    });
    if let Some(catalog) = settings.model_catalog_json.as_ref() {
        edits.push(ConfigEdit::SetPath {
            segments: vec!["model_catalog_json".to_string()],
            value: value(catalog.to_string_lossy().into_owned()),
        });
    }

    edits
}

/// Model already recorded in the target file, so re-running setup does not
/// silently switch models.
pub fn existing_model(codex_home: &Path, profile: Option<&ProfileV2Name>) -> Option<String> {
    let contents = std::fs::read_to_string(profile_config_path(codex_home, profile)).ok()?;
    let parsed: toml::Value = toml::from_str(&contents).ok()?;
    parsed.get("model")?.as_str().map(str::to_string)
}

pub async fn write_copilot_profile(
    codex_home: &Path,
    profile: Option<&ProfileV2Name>,
    settings: &CopilotProfileSettings,
) -> anyhow::Result<PathBuf> {
    let path = profile_config_path(codex_home, profile);
    ConfigEditsBuilder::for_config_path(&path)
        .with_edits(copilot_profile_edits(settings))
        .apply()
        .await?;
    Ok(path)
}

/// The running executable: a locally built fork often is not on `PATH`.
pub fn token_command_path() -> PathBuf {
    std::env::current_exe().unwrap_or_else(|_| PathBuf::from("qudiks"))
}

#[cfg(test)]
#[path = "github_copilot_setup_tests.rs"]
mod tests;
