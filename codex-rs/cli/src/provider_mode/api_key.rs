use anyhow::Result;
use codex_core::config::edit::ConfigEdit;
use codex_core::config::edit::ConfigEditsBuilder;
use crossterm::event::Event;
use crossterm::event::KeyCode;
use crossterm::event::KeyEventKind;
use crossterm::event::KeyModifiers;
use crossterm::terminal;
use std::io::IsTerminal;
use std::io::Write;
use std::path::Path;

pub(super) enum KeyEntryReason {
    Missing,
    Rejected,
}

struct KeyInputGuard;
impl Drop for KeyInputGuard {
    fn drop(&mut self) {
        let _ = terminal::disable_raw_mode();
        eprintln!();
    }
}

pub(super) fn read_api_key(reason: KeyEntryReason) -> Result<String> {
    let message = match reason {
        KeyEntryReason::Missing => "OpenAI API key missing",
        KeyEntryReason::Rejected => "OpenAI API key is invalid or expired",
    };
    anyhow::ensure!(
        std::io::stdin().is_terminal(),
        "{message}. Set OPENAI_API_KEY or run qudiks --openai-api YOUR_KEY in a terminal."
    );
    eprintln!("Create an API key: https://platform.openai.com/settings/organization/api-keys");
    terminal::enable_raw_mode()?;
    let _guard = KeyInputGuard;
    match reason {
        KeyEntryReason::Missing => {}
        KeyEntryReason::Rejected => {
            eprint!("{message}. Enter another key? [Y/n] ");
            std::io::stderr().flush()?;
            loop {
                if let Event::Key(key) = crossterm::event::read()?
                    && key.kind != KeyEventKind::Release
                {
                    match key.code {
                        KeyCode::Char('c' | 'd')
                            if key.modifiers.contains(KeyModifiers::CONTROL) =>
                        {
                            anyhow::bail!("API key entry cancelled")
                        }
                        KeyCode::Enter | KeyCode::Char('y' | 'Y') => break,
                        KeyCode::Esc | KeyCode::Char('n' | 'N') => {
                            anyhow::bail!("API key entry cancelled")
                        }
                        _ => {}
                    }
                }
            }
            eprint!("\r\n");
        }
    }
    eprint!("OpenAI API key (input hidden): ");
    std::io::stderr().flush()?;
    let mut input = String::new();
    loop {
        match crossterm::event::read()? {
            Event::Key(key) if key.kind != KeyEventKind::Release => match key.code {
                KeyCode::Enter => {
                    if input.trim().is_empty() {
                        continue;
                    }
                    return Ok(input.trim().into());
                }
                KeyCode::Char('c' | 'd') if key.modifiers.contains(KeyModifiers::CONTROL) => {
                    anyhow::bail!("API key entry cancelled")
                }
                KeyCode::Esc => anyhow::bail!("API key entry cancelled"),
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

pub(super) async fn save_api_key(path: &Path, key: &str) -> Result<()> {
    ConfigEditsBuilder::for_config_path(path)
        .with_edits([
            ConfigEdit::SetPath {
                segments: vec!["model_providers".into(), "openai-api".into(), "name".into()],
                value: toml_edit::value("OpenAI API"),
            },
            ConfigEdit::SetPath {
                segments: vec![
                    "model_providers".into(),
                    "openai-api".into(),
                    "experimental_bearer_token".into(),
                ],
                value: toml_edit::value(key),
            },
            ConfigEdit::ClearPath {
                segments: vec![
                    "model_providers".into(),
                    "openai-api".into(),
                    "env_key".into(),
                ],
            },
        ])
        .apply()
        .await?;
    eprintln!("API key saved in {}", path.display());
    Ok(())
}
