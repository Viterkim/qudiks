use super::*;
use core_test_support::responses;
use core_test_support::test_codex::run_test_with_large_stack;
use core_test_support::test_codex::test_codex;
use pretty_assertions::assert_eq;
use tempfile::TempDir;

fn profile(name: &str) -> ProfileV2Name {
    name.parse().expect("valid profile name")
}

fn settings() -> CopilotProfileSettings {
    CopilotProfileSettings {
        base_url: "https://api.githubcopilot.com".to_string(),
        model: Some("gpt-6.1-sol".to_string()),
        token_command: PathBuf::from("/usr/local/bin/qudiks-bin"),
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
async fn setup_is_idempotent() {
    let codex_home = TempDir::new().expect("temp dir");
    let first = write_and_read(codex_home.path(), &settings()).await;
    let second = write_and_read(codex_home.path(), &settings()).await;

    assert_eq!(first, second);
}

#[tokio::test]
async fn preserves_other_settings() {
    let codex_home = TempDir::new().expect("temp dir");
    let path = profile_config_path(codex_home.path(), Some(&profile("copilot")));
    std::fs::write(
        &path,
        r#"# hand-written notes
approval_policy = "on-request"

[model_providers.github-copilot]
name = "stale"
base_url = "https://api.githubcopilot.com"
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
}

#[tokio::test]
async fn preserves_model_when_discovery_fails() {
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

#[test]
fn loads_copilot_profile() -> anyhow::Result<()> {
    run_test_with_large_stack("copilot_profile", || async {
        let codex_home = TempDir::new().expect("temp dir");
        let profile = profile("copilot");
        write_copilot_profile(codex_home.path(), Some(&profile), &settings())
            .await
            .expect("write profile");

        let config = load_profile_config(codex_home.path(), &profile).await;

        assert_eq!(config.model_provider_id, COPILOT_PROVIDER_ID);

        let server = responses::start_mock_server().await;
        let response =
            responses::mount_sse_once(&server, responses::sse_completed("response-1")).await;
        let mut builder = test_codex().with_config(move |mock_config| {
            mock_config.model = config.model.clone();
            mock_config.model_reasoning_effort = config.model_reasoning_effort.clone();
            mock_config.model_reasoning_summary = config.model_reasoning_summary;
            mock_config.model_provider.name = config.model_provider.name.clone();
        });
        let test = Box::pin(builder.build_with_auto_env(&server))
            .await
            .expect("build mock session");
        test.submit_text_turn("hello")
            .await
            .expect("submit mock turn");

        let request = response.single_request();
        let body = request.body_json();
        assert_eq!(body["model"], "gpt-6.1-sol");
        assert_eq!(body["reasoning"]["effort"], "high");
        Ok(())
    })
}

#[tokio::test]
async fn preserves_explicit_tool_settings() {
    let codex_home = TempDir::new().expect("temp dir");
    let path = profile_config_path(codex_home.path(), Some(&profile("copilot")));
    std::fs::write(
        path,
        "web_search = \"disabled\"\n\n[features]\nmulti_agent = false\n",
    )
    .expect("seed profile");
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
    assert_eq!(
        parsed.get("web_search").and_then(toml::Value::as_str),
        Some("disabled")
    );
    assert_eq!(
        parsed
            .get("features")
            .and_then(|features| features.get("multi_agent"))
            .and_then(toml::Value::as_bool),
        Some(false)
    );
}

#[tokio::test]
async fn reads_saved_model() {
    let codex_home = TempDir::new().expect("temp dir");

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
