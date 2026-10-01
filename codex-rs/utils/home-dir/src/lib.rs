use codex_utils_absolute_path::AbsolutePathBuf;
use dirs::home_dir;
use std::path::PathBuf;

/// Environment variable that selects this fork's configuration directory.
pub const QUDIKS_HOME_ENV: &str = "QUDIKS_HOME";

/// Upstream Codex variable, still honored so existing setups keep working.
pub const CODEX_HOME_ENV: &str = "CODEX_HOME";

/// Directory used when neither environment variable is set.
///
/// Qudiks keeps its own home rather than sharing `~/.codex`, so a Copilot or
/// local-model setup here cannot disturb a real Codex installation, and each
/// can hold its own default model and credentials.
const DEFAULT_HOME_DIR_NAME: &str = ".qudiks";

/// Returns the path to the configuration directory.
///
/// `QUDIKS_HOME` wins, then `CODEX_HOME`, then `~/.qudiks`.
///
/// - When either variable is set, the value must exist and be a directory. The
///   value will be canonicalized and this function will Err otherwise.
/// - When neither is set, this function does not verify that the directory
///   exists.
pub fn find_codex_home() -> std::io::Result<AbsolutePathBuf> {
    for env_var in [QUDIKS_HOME_ENV, CODEX_HOME_ENV] {
        let value = std::env::var(env_var).ok().filter(|val| !val.is_empty());
        if value.is_some() {
            return find_codex_home_from_env(env_var, value.as_deref());
        }
    }
    find_codex_home_from_env(QUDIKS_HOME_ENV, /*codex_home_env*/ None)
}

fn find_codex_home_from_env(
    env_var: &str,
    codex_home_env: Option<&str>,
) -> std::io::Result<AbsolutePathBuf> {
    // Honor the `CODEX_HOME` environment variable when it is set to allow users
    // (and tests) to override the default location.
    match codex_home_env {
        Some(val) => {
            let path = PathBuf::from(val);
            let metadata = std::fs::metadata(&path).map_err(|err| match err.kind() {
                std::io::ErrorKind::NotFound => std::io::Error::new(
                    std::io::ErrorKind::NotFound,
                    format!("{env_var} points to {val:?}, but that path does not exist"),
                ),
                _ => std::io::Error::new(
                    err.kind(),
                    format!("failed to read {env_var} {val:?}: {err}"),
                ),
            })?;

            if !metadata.is_dir() {
                Err(std::io::Error::new(
                    std::io::ErrorKind::InvalidInput,
                    format!("{env_var} points to {val:?}, but that path is not a directory"),
                ))
            } else {
                let canonical = path.canonicalize().map_err(|err| {
                    std::io::Error::new(
                        err.kind(),
                        format!("failed to canonicalize {env_var} {val:?}: {err}"),
                    )
                })?;
                AbsolutePathBuf::from_absolute_path(canonical)
            }
        }
        None => {
            let mut p = home_dir().ok_or_else(|| {
                std::io::Error::new(
                    std::io::ErrorKind::NotFound,
                    "Could not find home directory",
                )
            })?;
            p.push(DEFAULT_HOME_DIR_NAME);
            AbsolutePathBuf::from_absolute_path(p)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::CODEX_HOME_ENV;
    use super::QUDIKS_HOME_ENV;
    use super::find_codex_home_from_env;
    use codex_utils_absolute_path::AbsolutePathBuf;
    use dirs::home_dir;
    use pretty_assertions::assert_eq;
    use std::fs;
    use std::io::ErrorKind;
    use tempfile::TempDir;

    #[test]
    fn find_codex_home_env_missing_path_is_fatal() {
        let temp_home = TempDir::new().expect("temp home");
        let missing = temp_home.path().join("missing-codex-home");
        let missing_str = missing
            .to_str()
            .expect("missing codex home path should be valid utf-8");

        let err = find_codex_home_from_env(CODEX_HOME_ENV, Some(missing_str))
            .expect_err("missing CODEX_HOME");
        assert_eq!(err.kind(), ErrorKind::NotFound);
        assert!(
            err.to_string().contains("CODEX_HOME"),
            "unexpected error: {err}"
        );
    }

    #[test]
    fn find_codex_home_env_file_path_is_fatal() {
        let temp_home = TempDir::new().expect("temp home");
        let file_path = temp_home.path().join("codex-home.txt");
        fs::write(&file_path, "not a directory").expect("write temp file");
        let file_str = file_path
            .to_str()
            .expect("file codex home path should be valid utf-8");

        let err =
            find_codex_home_from_env(CODEX_HOME_ENV, Some(file_str)).expect_err("file CODEX_HOME");
        assert_eq!(err.kind(), ErrorKind::InvalidInput);
        assert!(
            err.to_string().contains("not a directory"),
            "unexpected error: {err}"
        );
    }

    #[test]
    fn find_codex_home_env_valid_directory_canonicalizes() {
        let temp_home = TempDir::new().expect("temp home");
        let temp_str = temp_home
            .path()
            .to_str()
            .expect("temp codex home path should be valid utf-8");

        let resolved =
            find_codex_home_from_env(CODEX_HOME_ENV, Some(temp_str)).expect("valid CODEX_HOME");
        let expected = temp_home
            .path()
            .canonicalize()
            .expect("canonicalize temp home");
        let expected = AbsolutePathBuf::from_absolute_path(expected).expect("absolute home");
        assert_eq!(resolved, expected);
    }

    #[test]
    fn find_codex_home_without_env_uses_default_home_dir() {
        let resolved = find_codex_home_from_env(QUDIKS_HOME_ENV, /*codex_home_env*/ None)
            .expect("default CODEX_HOME");
        let mut expected = home_dir().expect("home dir");
        expected.push(super::DEFAULT_HOME_DIR_NAME);
        let expected = AbsolutePathBuf::from_absolute_path(expected).expect("absolute home");
        assert_eq!(resolved, expected);
    }
}
