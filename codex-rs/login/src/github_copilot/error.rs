use codex_http_client::HttpError;
use std::io;
use std::path::PathBuf;

/// Login and token-exchange failures.
#[derive(Debug, thiserror::Error)]
pub enum GithubCopilotError {
    #[error("invalid GitHub domain `{domain}`: {reason}")]
    InvalidDomain { domain: String, reason: String },

    #[error("refusing untrusted GitHub Copilot host `{host}`")]
    UntrustedHost { host: String },

    #[error("{context} failed with HTTP {status}: {body}")]
    HttpStatus {
        context: &'static str,
        status: u16,
        body: String,
    },

    #[error("{context} could not reach GitHub: {source}")]
    Transport {
        context: &'static str,
        #[source]
        source: HttpError,
    },

    #[error("{context} returned an unexpected response: {detail}")]
    InvalidResponse {
        context: &'static str,
        detail: String,
    },

    #[error("GitHub login was denied")]
    LoginDenied,

    #[error("the GitHub device code expired before login completed")]
    DeviceCodeExpired,

    #[error("GitHub device login failed: {0}")]
    DeviceFlow(String),

    #[error("GitHub device login timed out")]
    DeviceFlowTimeout,

    #[error("could not build an HTTPS client for {endpoint}: {detail}")]
    ClientBuild { endpoint: String, detail: String },

    #[error("not signed in to GitHub Copilot; run `codex login github-copilot`")]
    NotLoggedIn,

    #[error("could not access GitHub Copilot credentials at {path}: {source}")]
    Storage {
        path: PathBuf,
        #[source]
        source: io::Error,
    },
}

impl From<GithubCopilotError> for io::Error {
    fn from(error: GithubCopilotError) -> Self {
        io::Error::other(error.to_string())
    }
}

pub type Result<T> = std::result::Result<T, GithubCopilotError>;
