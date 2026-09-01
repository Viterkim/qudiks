use super::*;
use pretty_assertions::assert_eq;
use tempfile::TempDir;

fn profile(name: &str) -> ProfileV2Name {
    name.parse().expect("valid profile name")
}

fn settings() -> CopilotProfileSettings {
    CopilotProfileSettings {
        base_url: "https://api.githubcopilot.com".to_string(),
        model: Some("gpt-5.3-codex".to_string()),
        token_command: PathBuf::from("/usr/local/bin/codex"),
        model_catalog_json: None,
    }
}

async fn write_and_read(codex_home: &Path, settings: &CopilotProfileSettings) -> String {
    let path = write_copilot_profile(codex_home, Some(&profile("copilot")), settings)
        .await
        .expect("write profile");
    std::fs::read_to_string(path).expect("read profile")
}

#[tokio::test]
async fn writes_a_profile_codex_can_load() {
    let codex_home = TempDir::new().expect("temp dir");
    let contents = write_and_read(codex_home.path(), &settings()).await;

    let parsed: toml::Value = toml::from_str(&contents).expect("valid toml");
    assert_eq!(
        parsed,
        toml::from_str(
            r#"
model_provider = "github-copilot"
model = "gpt-5.3-codex"
model_reasoning_effort = "medium"

[model_providers.github-copilot]
name = "GitHub Copilot"
base_url = "https://api.githubcopilot.com"
wire_api = "responses"

[model_providers.github-copilot.auth]
command = "/usr/local/bin/codex"
args = ["login", "github-copilot", "token"]
refresh_interval_ms = 3600000
"#
        )
        .expect("valid expected toml")
    );
}

#[tokio::test]
async fn rerunning_setup_is_idempotent() {
    let codex_home = TempDir::new().expect("temp dir");
    let first = write_and_read(codex_home.path(), &settings()).await;
    let second = write_and_read(codex_home.path(), &settings()).await;
    assert_eq!(first, second);
}

#[tokio::test]
async fn updates_only_its_own_keys_and_preserves_the_rest() {
    let codex_home = TempDir::new().expect("temp dir");
    let path = profile_config_path(codex_home.path(), Some(&profile("copilot")));
    std::fs::write(
        &path,
        r#"# hand-written notes
approval_policy = "on-request"

[model_providers.github-copilot]
name = "stale"
base_url = "https://api.githubcopilot.com"

[tui]
theme = "dark"
"#,
    )
    .expect("seed profile");

    let contents = write_and_read(codex_home.path(), &settings()).await;

    assert!(
        contents.contains("# hand-written notes"),
        "comment was dropped: {contents}"
    );
    let parsed: toml::Value = toml::from_str(&contents).expect("valid toml");
    assert_eq!(
        parsed.get("approval_policy"),
        Some(&toml::Value::String("on-request".to_string()))
    );
    assert_eq!(
        parsed
            .get("tui")
            .and_then(|tui| tui.get("theme"))
            .and_then(toml::Value::as_str),
        Some("dark")
    );
    assert_eq!(
        parsed
            .get("model_providers")
            .and_then(|providers| providers.get(COPILOT_PROVIDER_ID))
            .and_then(|provider| provider.get("name"))
            .and_then(toml::Value::as_str),
        Some(GITHUB_COPILOT_PROVIDER_NAME)
    );
}

#[tokio::test]
async fn leaves_an_existing_model_alone_when_none_was_discovered() {
    let codex_home = TempDir::new().expect("temp dir");
    let path = profile_config_path(codex_home.path(), Some(&profile("copilot")));
    std::fs::write(&path, "model = \"already-chosen\"\n").expect("seed profile");

    let contents = write_and_read(
        codex_home.path(),
        &CopilotProfileSettings {
            model: None,
            ..settings()
        },
    )
    .await;

    let parsed: toml::Value = toml::from_str(&contents).expect("valid toml");
    assert_eq!(
        parsed.get("model").and_then(toml::Value::as_str),
        Some("already-chosen")
    );
}

/// Loads the written profile the way `codex -p <profile>` does.
async fn load_profile_config(
    codex_home: &Path,
    profile: &ProfileV2Name,
) -> codex_core::config::Config {
    let profile_path = profile_config_path(codex_home, Some(profile));
    codex_core::config::ConfigBuilder::default()
        .codex_home(codex_home.to_path_buf())
        .loader_overrides(codex_config::LoaderOverrides {
            user_config_path: Some(
                codex_utils_absolute_path::AbsolutePathBuf::from_absolute_path(&profile_path)
                    .expect("absolute profile path"),
            ),
            user_config_profile: Some(profile.clone()),
            ignore_project_config: true,
            ..codex_config::LoaderOverrides::without_managed_config_for_tests()
        })
        .build()
        .await
        .expect("load profile config")
}

#[tokio::test]
async fn codex_resolves_the_written_profile_to_the_copilot_provider() {
    let codex_home = TempDir::new().expect("temp dir");
    let profile = profile("copilot");
    write_copilot_profile(codex_home.path(), Some(&profile), &settings())
        .await
        .expect("write profile");

    let config = load_profile_config(codex_home.path(), &profile).await;

    assert_eq!(config.model_provider_id, COPILOT_PROVIDER_ID);
    assert_eq!(config.model.as_deref(), Some("gpt-5.3-codex"));
    assert_eq!(config.model_provider.name, GITHUB_COPILOT_PROVIDER_NAME);
    assert_eq!(
        config.model_provider.base_url.as_deref(),
        Some("https://api.githubcopilot.com")
    );

    let auth = config
        .model_provider
        .auth
        .as_ref()
        .expect("provider auth is configured");
    assert_eq!(auth.command, "/usr/local/bin/codex");
    assert_eq!(
        auth.args
            .iter()
            .map(|arg| arg.as_str())
            .collect::<Vec<&str>>(),
        vec!["login", "github-copilot", "token"]
    );
    // Copilot 400s on a bad token, so the 401 refresh path never fires and this
    // must be non-zero or a >24h session can never recover.
    assert!(auth.refresh_interval_ms > 0);
}

#[tokio::test]
async fn disables_namespaced_tools_for_grok_but_keeps_web_search() {
    let codex_home = TempDir::new().expect("temp dir");
    let contents = write_and_read(
        codex_home.path(),
        &CopilotProfileSettings {
            model: Some("grok-4.6".to_string()),
            ..settings()
        },
    )
    .await;

    let parsed: toml::Value = toml::from_str(&contents).expect("valid toml");
    assert_eq!(parsed.get("web_search"), None);
    assert_eq!(
        parsed
            .get("features")
            .and_then(|f| f.get("multi_agent"))
            .and_then(toml::Value::as_bool),
        Some(false)
    );
}

#[tokio::test]
async fn restores_namespaced_tools_for_a_model_that_supports_them() {
    let codex_home = TempDir::new().expect("temp dir");
    write_and_read(
        codex_home.path(),
        &CopilotProfileSettings {
            model: Some("grok-4.6".to_string()),
            ..settings()
        },
    )
    .await;
    let contents = write_and_read(codex_home.path(), &settings()).await;

    let parsed: toml::Value = toml::from_str(&contents).expect("valid toml");
    assert_eq!(parsed.get("web_search"), None);
    assert_eq!(
        parsed.get("features").and_then(|f| f.get("multi_agent")),
        None
    );
}

#[tokio::test]
async fn reads_back_the_model_so_a_rerun_does_not_switch_it() {
    let codex_home = TempDir::new().expect("temp dir");
    assert_eq!(existing_model(codex_home.path(), /*profile*/ None), None);

    write_copilot_profile(
        codex_home.path(),
        /*profile*/ None,
        &CopilotProfileSettings {
            model: Some("grok-4.6".to_string()),
            ..settings()
        },
    )
    .await
    .expect("write");

    assert_eq!(
        existing_model(codex_home.path(), /*profile*/ None),
        Some("grok-4.6".to_string())
    );
}
