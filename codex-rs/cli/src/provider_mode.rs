use anyhow::Context;
use anyhow::Result;
use clap::Args;
use codex_api::ModelsClient;
use codex_api::ReqwestTransport;
use codex_config::ConfigLoadOptions;
use codex_core::config::bootstrap_auth_config;
use codex_core::config::edit::ConfigEdit;
use codex_core::config::edit::ConfigEditsBuilder;
use codex_core::config::find_codex_home;
use codex_core::config::load_config_toml_with_layer_stack;
use codex_core::config::resolve_bootstrap_http_client_factory;
use codex_http_client::ClientRouteClass;
use codex_login::default_client::ClientRedirectPolicy;
use codex_login::default_client::create_client_for_route_async;
use codex_model_provider::BearerAuthProvider;
use codex_models_manager::bundled_models_response;
use codex_models_manager::model_info::model_info_from_slug;
use codex_protocol::config_types::ForcedLoginMethod;
use codex_protocol::config_types::ShellEnvironmentPolicyFilter;
use codex_protocol::openai_models::ApplyPatchToolType;
use codex_protocol::openai_models::ModelVisibility;
use codex_protocol::openai_models::ModelsResponse;
use codex_utils_absolute_path::AbsolutePathBuf;
use codex_utils_cli::CliConfigOverrides;
use codex_utils_cli::ProfileV2Name;
use codex_utils_path::write_atomically;
use crossterm::event::Event;
use crossterm::event::KeyCode;
use crossterm::event::KeyEventKind;
use crossterm::event::KeyModifiers;
use crossterm::terminal;
use serde_json::Value;
use std::hash::DefaultHasher;
use std::hash::Hash;
use std::hash::Hasher;
use std::io::IsTerminal;
use std::io::Write;
use std::path::Path;
use std::sync::Arc;
use std::time::Duration;

#[cfg(test)]
#[path = "provider_mode_tests.rs"]
mod tests;

#[derive(Args, Debug, Default)]
pub(super) struct ProviderMode {
    /// Use GitHub Copilot (the default Qudiks provider).
    #[arg(long, global = true, conflicts_with_all = ["codex", "openai_api", "endpoint"])]
    pub github: bool,

    /// Use Codex with your ChatGPT subscription; sign in on first launch.
    #[arg(long, global = true, conflicts_with_all = ["openai_api", "endpoint"])]
    pub codex: bool,

    /// Use the OpenAI API with your own API key; ask for a key on first launch.
    #[arg(long, global = true, conflicts_with = "endpoint")]
    pub openai_api: bool,

    /// Use an OpenAI-compatible Chat Completions server, e.g. localhost:8080.
    #[arg(long, global = true, value_name = "URL")]
    pub endpoint: Option<String>,
}
impl ProviderMode {
    pub fn selected(&self) -> bool {
        self.github || self.codex || self.openai_api || self.endpoint.is_some()
    }

    pub async fn prepare(
        &self,
        overrides: &mut CliConfigOverrides,
        profile: Option<&ProfileV2Name>,
        cwd: Option<&Path>,
    ) -> Result<()> {
        if self.github {
            overrides
                .raw_overrides
                .push("model_provider=github-copilot".into());
            return Ok(());
        }
        if self.codex {
            let home = find_codex_home()?;
            let cwd = AbsolutePathBuf::relative_to_current_dir(cwd.unwrap_or(Path::new(".")))?;
            let cli_overrides = overrides.parse_overrides().map_err(anyhow::Error::msg)?;
            let explicit_model = cli_overrides.iter().any(|(path, _)| path == "model");
            let loaded = load_config_toml_with_layer_stack(
                &home,
                Some(&cwd),
                cli_overrides,
                ConfigLoadOptions {
                    loader_overrides: crate::loader_overrides_for_profile_at_codex_home(
                        profile, &home,
                    ),
                    ..Default::default()
                },
            )
            .await?;
            anyhow::ensure!(
                loaded
                    .config_layer_stack
                    .required_model_provider()
                    .is_none_or(|id| id == "openai"),
                "The selected provider conflicts with the managed model provider requirement"
            );
            anyhow::ensure!(
                codex_login::auth::read_codex_api_key_from_env().is_none(),
                "--codex uses ChatGPT sign-in. Unset CODEX_API_KEY for this launch, or use --openai-api."
            );
            let mut auth_config = bootstrap_auth_config(&home, &loaded)?;
            anyhow::ensure!(
                auth_config.managed_auth_policy.allows_login_method(
                    ForcedLoginMethod::Chatgpt,
                    /*forced_login_method*/ None,
                    auth_config.forced_chatgpt_workspace_id.as_deref(),
                ),
                "ChatGPT sign-in is disabled by managed authentication policy"
            );
            // Inspect the saved login before upstream's forced-login check can remove it.
            auth_config.forced_login_method = None;
            auth_config.managed_auth_policy.allowed_login_methods = None;
            if let Some(auth) = auth_config
                .load_auth(/*enable_codex_api_key_env*/ false)
                .await?
            {
                anyhow::ensure!(
                    auth.uses_codex_backend(),
                    "--codex needs ChatGPT sign-in. Run `qudiks login` first; your saved API login was left unchanged."
                );
            }
            overrides.raw_overrides.extend([
                "model_provider=openai".into(),
                "forced_login_method=chatgpt".into(),
                "model_catalog_json=false".into(),
            ]);
            let bundled = bundled_models_response()?;
            if !explicit_model
                && loaded.config_toml.model_provider.as_deref() != Some("openai")
                && !loaded
                    .config_toml
                    .model
                    .as_ref()
                    .is_some_and(|slug| bundled.models.iter().any(|model| &model.slug == slug))
                && let Some(model) = bundled
                    .models
                    .iter()
                    .filter(|model| model.visibility == ModelVisibility::List)
                    .min_by_key(|model| model.priority)
            {
                overrides
                    .raw_overrides
                    .push(format!("model={}", toml::Value::String(model.slug.clone())));
            }
            return Ok(());
        }
        let (id, name) = if self.openai_api {
            ("openai-api", "OpenAI API")
        } else {
            ("endpoint", "Custom endpoint")
        };
        overrides.raw_overrides.push(format!(
            "model_providers.{id}.name={}",
            toml::Value::String(name.into())
        ));
        let home = find_codex_home()?;
        let cwd = AbsolutePathBuf::relative_to_current_dir(cwd.unwrap_or(Path::new(".")))?;
        let loader_overrides = crate::loader_overrides_for_profile_at_codex_home(profile, &home);
        let loaded = load_config_toml_with_layer_stack(
            &home,
            Some(&cwd),
            overrides.parse_overrides().map_err(anyhow::Error::msg)?,
            ConfigLoadOptions {
                loader_overrides,
                ..Default::default()
            },
        )
        .await?;
        let http_client_factory = resolve_bootstrap_http_client_factory(
            &loaded.config_toml,
            loaded
                .config_layer_stack
                .requirements()
                .feature_requirements
                .as_ref(),
        )?;
        let required_provider = loaded.config_layer_stack.required_model_provider();
        anyhow::ensure!(
            required_provider.is_none_or(|required| required == id),
            "The selected provider conflicts with the managed model provider requirement"
        );
        let mut config = loaded.config_toml;
        let provider = config.model_providers.entry(id.into()).or_default();
        anyhow::ensure!(
            provider.auth.is_none() && provider.aws.is_none() && provider.gateway_oauth.is_none(),
            "Provider mode requires a bearer key, not command-backed, AWS, or gateway authentication"
        );
        provider.name = name.into();
        provider.base_url = Some(match &self.endpoint {
            Some(endpoint) => normalize_endpoint(endpoint)?,
            None => "https://api.openai.com/v1".into(),
        });
        provider.wire_api = serde_json::from_value(Value::String(
            if self.openai_api {
                "responses"
            } else {
                "chat_completions"
            }
            .into(),
        ))?;
        provider.requires_openai_auth = false;
        provider.supports_websockets = false;
        provider.supports_standalone_web_search = false;
        provider.model_catalog_url = None;
        provider.include_internal_metadata = false;

        let mut key = provider
            .api_key()?
            .or_else(|| {
                provider
                    .experimental_bearer_token
                    .as_ref()
                    .map(|token| token.to_string())
            })
            .filter(|key| !key.trim().is_empty());
        if self.openai_api && key.is_none() {
            key = codex_login::read_openai_api_key_from_env();
            if key.is_some() {
                provider.env_key = Some("OPENAI_API_KEY".into());
            }
            if key.is_none() {
                let entered = tokio::task::spawn_blocking(read_api_key).await??;
                let path = match profile {
                    Some(profile) => home.join(format!("{profile}.config.toml")),
                    None => home.join("config.toml"),
                };
                ConfigEditsBuilder::for_config_path(&path)
                    .with_edits([
                        ConfigEdit::SetPath {
                            segments: vec!["model_providers".into(), id.into(), "name".into()],
                            value: toml_edit::value(name),
                        },
                        ConfigEdit::SetPath {
                            segments: vec![
                                "model_providers".into(),
                                id.into(),
                                "experimental_bearer_token".into(),
                            ],
                            value: toml_edit::value(&entered),
                        },
                    ])
                    .apply()
                    .await?;
                eprintln!("API key saved in {}", path.display());
                key = Some(entered);
            }
        }
        let api = provider.to_api_provider(/*auth_mode*/ None)?;
        let request_url = api.url_for_path("models");
        let client = create_client_for_route_async(
            http_client_factory,
            request_url.clone(),
            ClientRouteClass::Api,
            ClientRedirectPolicy::Reject,
        )
        .await?;
        let models = ModelsClient::new(
            ReqwestTransport::from_http_client(client),
            api,
            Arc::new(BearerAuthProvider {
                token: key,
                ..Default::default()
            }),
        );
        let (body, _) = tokio::time::timeout(
            Duration::from_secs(/*secs*/ 15),
            models.list_models_raw(
                request_url,
                http::HeaderMap::new(),
                Some(/*response_body_limit_bytes*/ 1_048_576),
            ),
        )
        .await
        .context("model discovery timed out")??;
        let response: Value = serde_json::from_slice(&body).context("invalid /models response")?;
        let available = response["data"]
            .as_array()
            .context("/models must return a data array")?;
        let bundled = bundled_models_response()?;
        let mut catalog = Vec::new();
        for entry in available.iter().take(512) {
            let Some(slug) = entry["id"].as_str().filter(|slug| slug.len() <= 256) else {
                continue;
            };
            let known = bundled.models.iter().find(|model| model.slug == slug);
            if self.openai_api && known.is_none() {
                continue;
            }
            let mut model = known.cloned().unwrap_or_else(|| model_info_from_slug(slug));
            if self.openai_api && model.visibility != ModelVisibility::List {
                continue;
            }
            model.visibility = ModelVisibility::List;
            model.supported_in_api = true;
            model.use_responses_lite = false;
            model.supports_reasoning_effort_updates = false;
            if !self.openai_api {
                model.default_reasoning_level = None;
                model.supported_reasoning_levels.clear();
                model.supports_reasoning_summary_parameter = false;
                model.apply_patch_tool_type = Some(ApplyPatchToolType::Function);
                if known.is_none() {
                    let context = entry["meta"]["n_ctx_train"].as_i64().unwrap_or(32_768);
                    model.context_window = Some(context.clamp(4096, 1_048_576));
                    model.max_context_window = model.context_window;
                }
            }
            if let Some(messages) = &mut model.model_messages
                && let Some(instructions) = &mut messages.instructions_template
            {
                *instructions = instructions.replace("You are Codex,", "You are Qudiks,");
            }
            catalog.push(model);
        }
        catalog.sort_by_key(|model| model.priority);
        let default = catalog
            .first()
            .context("no supported models found at the endpoint")?;
        let model = config
            .model
            .as_ref()
            .filter(|slug| catalog.iter().any(|model| &model.slug == *slug))
            .unwrap_or(&default.slug)
            .clone();
        let cache_dir = home.join("provider-models");
        std::fs::create_dir_all(&cache_dir)?;
        let mut endpoint_hash = DefaultHasher::new();
        provider.base_url.hash(&mut endpoint_hash);
        let catalog_path = cache_dir.join(format!("{id}-{:016x}.json", endpoint_hash.finish()));
        let catalog = ModelsResponse { models: catalog };
        write_atomically(&catalog_path, &serde_json::to_string(&catalog)?)?;
        let mut provider_override = toml::Value::try_from(&*provider)?;
        // The key is already in config or the environment; don't copy it into launch overrides.
        if let Some(table) = provider_override.as_table_mut() {
            for key in ["experimental_bearer_token", "http_headers", "query_params"] {
                table.remove(key);
            }
        }
        // Shell snapshots otherwise persist inherited API credentials alongside the session.
        let mut filters = config.shell_environment_policy.filters.unwrap_or_default();
        for pattern in config.shell_environment_policy.exclude.unwrap_or_default() {
            filters.insert(pattern, ShellEnvironmentPolicyFilter::Exclude);
        }
        for pattern in config
            .shell_environment_policy
            .include_only
            .unwrap_or_default()
        {
            filters.insert(pattern, ShellEnvironmentPolicyFilter::Include);
        }
        for name in [Some("OPENAI_API_KEY"), provider.env_key.as_deref()]
            .into_iter()
            .flatten()
        {
            filters.retain(|pattern, _| !pattern.eq_ignore_ascii_case(name));
            filters.insert(name.into(), ShellEnvironmentPolicyFilter::Exclude);
        }
        overrides.raw_overrides.push(format!(
            "shell_environment_policy.filters={}",
            toml::Value::try_from(filters)?
        ));
        for (path, value) in [
            (format!("model_providers.{id}"), provider_override),
            ("model_provider".into(), toml::Value::String(id.into())),
            ("model".into(), toml::Value::String(model)),
            (
                "model_catalog_json".into(),
                toml::Value::String(catalog_path.to_string_lossy().into_owned()),
            ),
        ] {
            overrides.raw_overrides.push(format!("{path}={value}"));
        }
        if self.endpoint.is_some() {
            overrides.raw_overrides.extend([
                "web_search=disabled".into(),
                "features.image_generation=false".into(),
            ]);
        }
        Ok(())
    }
}

fn normalize_endpoint(endpoint: &str) -> Result<String> {
    let endpoint = if endpoint.contains("://") {
        endpoint.to_string()
    } else {
        format!("http://{endpoint}")
    };
    let mut url = url::Url::parse(&endpoint).context("invalid endpoint URL")?;
    anyhow::ensure!(
        matches!(url.scheme(), "http" | "https")
            && url.host().is_some()
            && url.username().is_empty()
            && url.password().is_none()
            && url.query().is_none()
            && url.fragment().is_none(),
        "endpoint must be an HTTP(S) base URL without credentials, query, or fragment"
    );
    if url.path().trim_matches('/').is_empty() {
        url.set_path("/v1");
    }
    Ok(url.to_string().trim_end_matches('/').into())
}

struct KeyInputGuard;
impl Drop for KeyInputGuard {
    fn drop(&mut self) {
        let _ = terminal::disable_raw_mode();
        eprintln!();
    }
}

fn read_api_key() -> Result<String> {
    anyhow::ensure!(
        std::io::stdin().is_terminal(),
        "OpenAI API key missing. Set OPENAI_API_KEY or launch qudiks --openai-api in a terminal."
    );
    eprintln!("Create an API key: https://platform.openai.com/settings/organization/api-keys");
    eprint!("OpenAI API key (input hidden): ");
    std::io::stderr().flush()?;
    terminal::enable_raw_mode()?;
    let _guard = KeyInputGuard;
    let mut input = String::new();
    loop {
        match crossterm::event::read()? {
            Event::Key(key) if key.kind != KeyEventKind::Release => match key.code {
                KeyCode::Enter => {
                    anyhow::ensure!(!input.trim().is_empty(), "No API key entered");
                    return Ok(input.trim().into());
                }
                KeyCode::Char('c' | 'd') if key.modifiers.contains(KeyModifiers::CONTROL) => {
                    anyhow::bail!("API key entry cancelled")
                }
                KeyCode::Backspace => {
                    input.pop();
                }
                KeyCode::Char(character)
                    if !character.is_control()
                        && !key.modifiers.contains(KeyModifiers::CONTROL) =>
                {
                    input.push(character);
                }
                _ => {}
            },
            Event::Paste(paste) => input.push_str(paste.trim()),
            _ => {}
        }
        anyhow::ensure!(input.len() <= 4096, "API key is too long");
    }
}
