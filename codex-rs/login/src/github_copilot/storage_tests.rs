use super::*;
use chrono::DateTime;
use pretty_assertions::assert_eq;
use tempfile::TempDir;

fn sample() -> GithubCopilotCredentials {
    GithubCopilotCredentials {
        github_token: "gho_token".to_string(),
        domain: "github.com".to_string(),
        client_id: "Iv1.test".to_string(),
        copilot_token: Some(CopilotToken {
            token: "tid=abc".to_string(),
            expires_at: DateTime::from_timestamp(1_893_456_000, 0).expect("timestamp"),
            api_base_url: "https://api.githubcopilot.com".to_string(),
        }),
    }
}

#[test]
fn round_trips_credentials() {
    let home = TempDir::new().expect("temp dir");
    assert_eq!(load(home.path()).expect("load empty"), None);
    save(home.path(), &sample()).expect("save");
    assert_eq!(load(home.path()).expect("load"), Some(sample()));
    assert!(delete(home.path()).expect("delete"));
    assert_eq!(load(home.path()).expect("load"), None);
}

#[cfg(unix)]
#[test]
fn stores_the_github_token_with_owner_only_permissions() {
    use std::os::unix::fs::PermissionsExt;

    let home = TempDir::new().expect("temp dir");
    save(home.path(), &sample()).expect("save");
    let mode = std::fs::metadata(credentials_path(home.path()))
        .expect("metadata")
        .permissions()
        .mode();
    assert_eq!(mode & 0o777, 0o600);
}
