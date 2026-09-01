//! `login github-copilot` commands.

use crate::github_copilot_catalog::write_catalog;
use crate::github_copilot_setup::CopilotProfileSettings;
use crate::github_copilot_setup::existing_model;
use crate::github_copilot_setup::profile_config_path;
use crate::github_copilot_setup::token_command_path;
use crate::github_copilot_setup::write_copilot_profile;
use crate::login::load_config_or_exit;
use chrono::Utc;
use codex_config::ProfileV2Name;
use codex_core::config::Config;
use codex_login::AuthRouteConfig;
use codex_login::github_copilot;
use codex_login::github_copilot::CopilotToken;
use codex_login::github_copilot::GithubCopilotError;
use codex_login::github_copilot::GithubCopilotLoginOptions;
use codex_login::github_copilot::GithubDeviceCode;
use codex_utils_cli::CliConfigOverrides;
use std::path::Path;
use std::path::PathBuf;

#[derive(Debug, Clone)]
pub struct GithubCopilotLoginArgs {
    pub domain: String,
    pub client_id: Option<String>,
    // None writes the base config, so plain `qudiks` uses Copilot.
    pub profile: Option<ProfileV2Name>,
    pub model: Option<String>,
}

fn login_options(config: &Config, args: &GithubCopilotLoginArgs) -> GithubCopilotLoginOptions {
    let mut options = GithubCopilotLoginOptions::new(config.codex_home.to_path_buf());
    options.domain = args.domain.clone();
    if let Some(client_id) = args.client_id.as_deref() {
        options.client_id = client_id.to_string();
    }
    options
}

fn print_device_code(device_code: &GithubDeviceCode) {
    eprintln!(
        "Open {} and enter the code:\n\n    {}\n\nWaiting for approval…",
        device_code.verification_uri, device_code.user_code
    );
}

fn exit_with(error: GithubCopilotError) -> ! {
    eprintln!("Error: {error}");
    std::process::exit(1);
}

/// Discovery failing is not fatal; the user can name a model later.
async fn resolve_model(
    codex_home: &Path,
    profile: Option<&ProfileV2Name>,
    auth_route_config: &AuthRouteConfig,
    token: &CopilotToken,
    requested: Option<&str>,
) -> (Option<String>, Option<PathBuf>) {
    let requested = requested
        .map(str::to_string)
        .or_else(|| existing_model(codex_home, profile));
    let requested = requested.as_deref();
    let models = match github_copilot::discover_models(codex_home, auth_route_config, token).await {
        Ok(models) => models,
        Err(err) => {
            eprintln!("Warning: could not list GitHub Copilot models: {err}");
            return (requested.map(str::to_string), None);
        }
    };

    let chosen = github_copilot::choose_model(&models, requested);
    if let (Some(requested), Some(chosen)) = (requested, chosen.as_deref())
        && requested != chosen
    {
        eprintln!("Warning: `{requested}` cannot be used here, falling back to `{chosen}`.");
    }
    // Codex's /models refresh cannot read Copilot's response.
    let catalog = match write_catalog(codex_home, &github_copilot::usable_models(&models)) {
        Ok(path) => Some(path),
        Err(err) => {
            eprintln!("Warning: could not write the model catalog: {err:#}");
            None
        }
    };
    (chosen, catalog)
}

pub async fn run_login_with_github_copilot(
    cli_config_overrides: CliConfigOverrides,
    args: GithubCopilotLoginArgs,
) -> ! {
    let config = load_config_or_exit(cli_config_overrides).await;
    let options = login_options(&config, &args);
    let auth_route_config = config.auth_route_config();

    let credentials =
        match github_copilot::run_device_login(&options, &auth_route_config, print_device_code)
            .await
        {
            Ok(credentials) => credentials,
            Err(err) => exit_with(err),
        };
    eprintln!("Signed in to GitHub Copilot via {}.", credentials.domain);

    let Some(token) = credentials.copilot_token else {
        eprintln!(
            "Error: login completed without a Copilot token; run `codex login github-copilot` again"
        );
        std::process::exit(1);
    };
    let (model, model_catalog_json) = resolve_model(
        &config.codex_home,
        args.profile.as_ref(),
        &auth_route_config,
        &token,
        args.model.as_deref(),
    )
    .await;
    let settings = CopilotProfileSettings {
        base_url: token.api_base_url,
        model,
        token_command: token_command_path(),
        model_catalog_json,
    };

    match write_copilot_profile(&config.codex_home, args.profile.as_ref(), &settings).await {
        Ok(path) => {
            report_profile(
                args.profile.as_ref(),
                &settings,
                &path.display().to_string(),
            );
            std::process::exit(0);
        }
        Err(err) => {
            eprintln!(
                "Error writing {}: {err:#}",
                path_label(args.profile.as_ref())
            );
            std::process::exit(1);
        }
    }
}

fn invoked_as() -> String {
    std::env::current_exe()
        .ok()
        .and_then(|exe| {
            exe.file_stem()
                .map(|stem| stem.to_string_lossy().into_owned())
        })
        .unwrap_or_else(|| "qudiks".to_string())
}

fn report_profile(profile: Option<&ProfileV2Name>, settings: &CopilotProfileSettings, path: &str) {
    eprintln!("Wrote {path}");
    eprintln!("  Copilot API: {}", settings.base_url);
    match settings.model.as_deref() {
        Some(model) => eprintln!("  Model:       {model}"),
        None => eprintln!("  Model:       not set; add `model = \"...\"` to the profile"),
    }
    let binary = invoked_as();
    match profile {
        Some(profile) => eprintln!("\nStart a session with:\n\n    {binary} -p {profile}\n"),
        None => eprintln!("\nStart a session with:\n\n    {binary}\n"),
    }
}

pub async fn run_github_copilot_setup(
    cli_config_overrides: CliConfigOverrides,
    args: GithubCopilotLoginArgs,
) -> ! {
    let config = load_config_or_exit(cli_config_overrides).await;
    let auth_route_config = config.auth_route_config();

    let token = match github_copilot::bearer_token(&config.codex_home, &auth_route_config).await {
        Ok(token) => token,
        Err(err) => exit_with(err),
    };
    let (model, model_catalog_json) = resolve_model(
        &config.codex_home,
        args.profile.as_ref(),
        &auth_route_config,
        &token,
        args.model.as_deref(),
    )
    .await;
    let settings = CopilotProfileSettings {
        base_url: token.api_base_url,
        model,
        token_command: token_command_path(),
        model_catalog_json,
    };

    match write_copilot_profile(&config.codex_home, args.profile.as_ref(), &settings).await {
        Ok(path) => {
            report_profile(
                args.profile.as_ref(),
                &settings,
                &path.display().to_string(),
            );
            std::process::exit(0);
        }
        Err(err) => {
            eprintln!(
                "Error writing {}: {err:#}",
                path_label(args.profile.as_ref())
            );
            std::process::exit(1);
        }
    }
}

/// Only the token goes to stdout; provider auth reads it verbatim.
pub async fn run_github_copilot_token(cli_config_overrides: CliConfigOverrides) -> ! {
    let config = load_config_or_exit(cli_config_overrides).await;
    let auth_route_config = config.auth_route_config();

    match github_copilot::bearer_token(&config.codex_home, &auth_route_config).await {
        Ok(token) => {
            println!("{}", token.token);
            std::process::exit(0);
        }
        Err(err) => exit_with(err),
    }
}

fn path_label(profile: Option<&ProfileV2Name>) -> String {
    match profile {
        Some(profile) => format!("the {profile} profile"),
        None => "the base config".to_string(),
    }
}

pub async fn run_github_copilot_status(
    cli_config_overrides: CliConfigOverrides,
    profile: Option<&ProfileV2Name>,
) -> ! {
    let config = load_config_or_exit(cli_config_overrides).await;

    match github_copilot::stored_credentials(&config.codex_home) {
        Ok(Some(credentials)) => {
            eprintln!("Signed in to GitHub Copilot via {}", credentials.domain);
            match credentials.copilot_token {
                Some(token) if token.is_fresh_at(Utc::now()) => eprintln!(
                    "Copilot API: {}\nToken valid until {}",
                    token.api_base_url, token.expires_at
                ),
                Some(token) => eprintln!(
                    "Copilot API: {}\nToken expired at {}; it will be refreshed on next use",
                    token.api_base_url, token.expires_at
                ),
                None => eprintln!("No Copilot token issued yet; one will be fetched on next use"),
            }

            let profile_path = profile_config_path(&config.codex_home, profile);
            if profile_path.exists() {
                eprintln!("Config: {}", profile_path.display());
            } else {
                eprintln!(
                    "Config: {} is missing; run `{} login github-copilot setup`",
                    profile_path.display(),
                    invoked_as()
                );
            }
            std::process::exit(0);
        }
        Ok(None) => {
            eprintln!("Not signed in to GitHub Copilot");
            std::process::exit(1);
        }
        Err(err) => exit_with(err),
    }
}

/// Leaves the profile alone: no secrets, possibly hand-edited.
pub async fn run_github_copilot_logout(cli_config_overrides: CliConfigOverrides) -> ! {
    let config = load_config_or_exit(cli_config_overrides).await;

    match github_copilot::logout(&config.codex_home) {
        Ok(true) => {
            eprintln!("Removed stored GitHub Copilot credentials");
            std::process::exit(0);
        }
        Ok(false) => {
            eprintln!("No stored GitHub Copilot credentials");
            std::process::exit(0);
        }
        Err(err) => exit_with(err),
    }
}

/// Chat-only models are listed separately, not offered as choices.
pub async fn run_github_copilot_models(cli_config_overrides: CliConfigOverrides) -> ! {
    let config = load_config_or_exit(cli_config_overrides).await;
    let auth_route_config = config.auth_route_config();

    let token = match github_copilot::bearer_token(&config.codex_home, &auth_route_config).await {
        Ok(token) => token,
        Err(err) => exit_with(err),
    };
    let models =
        match github_copilot::discover_models(&config.codex_home, &auth_route_config, &token).await
        {
            Ok(models) => models,
            Err(err) => exit_with(err),
        };

    let usable = github_copilot::usable_models(&models);
    let unusable = models
        .iter()
        .filter(|model| !usable.iter().any(|kept| kept.id == model.id))
        .collect::<Vec<_>>();

    println!("Usable models ({}):", usable.len());
    for model in &usable {
        println!("  {}", model.id);
    }
    if !unusable.is_empty() {
        println!(
            "\n{} more in the catalog do not serve the Responses API and cannot be used:",
            unusable.len()
        );
        for model in &unusable {
            println!("  {}", model.id);
        }
    }
    println!(
        "\nSelect one with:\n\n    {} login github-copilot setup --model <MODEL>\n",
        invoked_as()
    );
    std::process::exit(0);
}
