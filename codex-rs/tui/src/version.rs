/// The current Codex CLI version as embedded at compile time.
pub const CODEX_CLI_VERSION: &str = env!("CARGO_PKG_VERSION");

/// Local joke branding shown in the TUI header.
pub const CODEX_DISPLAY_NAME: &str = "Åben KI Qudiks";
pub const QUDIKS_RELEASE_VERSION: &str = include_str!("../assets/qudiks-version");

pub fn qudiks_display_version() -> &'static str {
    let version = QUDIKS_RELEASE_VERSION.trim();
    version.strip_prefix("0.0.").unwrap_or(version)
}
