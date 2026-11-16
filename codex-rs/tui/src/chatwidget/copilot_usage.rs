use super::ChatWidget;
use crate::app_event::AppEvent;
use crate::config_panel::ConfigPreference;
use crate::config_panel::ConfigToggle;
use crate::status::format_tokens_compact;
use codex_login::github_copilot::CopilotQuotaSnapshot;
use codex_login::github_copilot::fetch_quota_snapshot;
use codex_login::github_copilot::format_quota_snapshot;
use std::time::Duration;
use std::time::Instant;
use uuid::Uuid;

const REFRESH_INTERVAL: Duration = Duration::from_secs(5 * 60);

#[derive(Default)]
pub(super) struct CopilotUsageState {
    last_attempt: Option<Instant>,
    pending: Option<Uuid>,
    show_result: bool,
    result: Option<Result<CopilotQuotaSnapshot, String>>,
}

impl ChatWidget {
    pub(super) fn handle_copilot_usage_args(&mut self, args: &str) {
        let enabled = match args {
            "toggle" => !self.local_settings.tui.copilot_usage_indicator,
            "on" => true,
            "off" => false,
            _ => {
                self.add_error_message("Usage: /usage [toggle|on|off]".to_string());
                return;
            }
        };
        self.app_event_tx
            .send(AppEvent::SaveConfigPreference(ConfigPreference::Toggle {
                setting: ConfigToggle::CopilotUsageIndicator,
                enabled,
            }));
    }

    pub(super) fn show_github_copilot_usage(&mut self) {
        if self
            .copilot_usage
            .last_attempt
            .is_some_and(|at| at.elapsed() < REFRESH_INTERVAL)
            && let Some(Ok(snapshot)) = self.copilot_usage.result.clone()
        {
            self.add_info_message(format_quota_snapshot(&snapshot), /*hint*/ None);
            return;
        }
        self.copilot_usage.show_result = true;
        self.request_copilot_usage();
    }

    fn request_copilot_usage(&mut self) {
        if self.copilot_usage.pending.is_some() {
            return;
        }
        let request_id = Uuid::new_v4();
        self.copilot_usage.pending = Some(request_id);
        self.copilot_usage.last_attempt = Some(Instant::now());
        let tx = self.app_event_tx.clone();
        let codex_home = self.config.codex_home.clone();
        let auth_route = codex_login::AuthRouteConfig::from_http_client_factory(
            self.config.http_client_factory(),
        );
        tokio::spawn(async move {
            let result = fetch_quota_snapshot(&codex_home, &auth_route)
                .await
                .map_err(|error| error.to_string());
            tx.send(AppEvent::CopilotUsageLoaded { request_id, result });
        });
    }

    pub(crate) fn on_copilot_usage_loaded(
        &mut self,
        request_id: Uuid,
        result: Result<CopilotQuotaSnapshot, String>,
    ) {
        if self.copilot_usage.pending != Some(request_id) {
            return;
        }
        self.copilot_usage.pending = None;
        self.copilot_usage.last_attempt = Some(Instant::now());
        if std::mem::take(&mut self.copilot_usage.show_result) {
            match &result {
                Ok(snapshot) => {
                    self.add_info_message(format_quota_snapshot(snapshot), /*hint*/ None)
                }
                Err(error) => self.add_error_message(error.clone()),
            }
        }
        self.copilot_usage.result = Some(result);
        self.refresh_copilot_usage_indicator();
        self.request_redraw();
    }

    pub(super) fn refresh_copilot_usage_indicator(&mut self) {
        if self.config.model_provider_id != "github-copilot"
            || !self.local_settings.tui.copilot_usage_indicator
        {
            self.bottom_pane.set_copilot_credits(None);
            return;
        }
        let elapsed = self.copilot_usage.last_attempt.map(|at| at.elapsed());
        if elapsed.is_none_or(|elapsed| elapsed >= REFRESH_INTERVAL) {
            self.request_copilot_usage();
        }
        let label = match self.copilot_usage.result.as_ref() {
            Some(Ok(snapshot)) => credits_label(snapshot),
            Some(Err(_)) => "C unavailable".to_string(),
            None => "C …".to_string(),
        };
        self.bottom_pane.set_copilot_credits(Some(label));
        if self.copilot_usage.pending.is_none() {
            self.frame_requester
                .schedule_frame_in(REFRESH_INTERVAL.saturating_sub(elapsed.unwrap_or_default()));
        }
    }
}

fn credits_label(snapshot: &CopilotQuotaSnapshot) -> String {
    if let (Some(used), Some(limit)) = (snapshot.credits_used, snapshot.entitlement)
        && limit > 0
    {
        format!(
            "{} / {} C",
            format_tokens_compact(used),
            format_tokens_compact(limit)
        )
    } else if snapshot.unlimited {
        "∞ C".to_string()
    } else if let Some(remaining) = snapshot.remaining {
        format!("{} C left", format_tokens_compact(remaining))
    } else {
        "C unavailable".to_string()
    }
}

#[cfg(test)]
#[path = "copilot_usage_tests.rs"]
mod tests;
