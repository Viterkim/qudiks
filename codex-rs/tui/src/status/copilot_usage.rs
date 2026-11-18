use super::format::FieldFormatter;
use super::format::push_label;
use codex_login::github_copilot::CopilotQuotaSnapshot;
use codex_protocol::num_format::format_with_separators;
use ratatui::prelude::Line;
use ratatui::prelude::Span;
use std::collections::BTreeSet;
use std::sync::RwLock;

#[derive(Debug, Default)]
pub(crate) struct StatusCopilotUsage {
    snapshot: RwLock<Option<Option<CopilotQuotaSnapshot>>>,
}
impl StatusCopilotUsage {
    pub(crate) fn set_snapshot(&self, snapshot: Option<CopilotQuotaSnapshot>) {
        #[expect(clippy::expect_used)]
        let mut stored = self
            .snapshot
            .write()
            .expect("status history Copilot usage state poisoned");
        *stored = Some(snapshot);
    }

    pub(crate) fn push_labels(labels: &mut Vec<String>, seen: &mut BTreeSet<String>) {
        push_label(labels, seen, "Copilot usage");
        push_label(labels, seen, "Resets on");
    }

    pub(crate) fn lines(&self, formatter: &FieldFormatter) -> Vec<Line<'static>> {
        #[expect(clippy::expect_used)]
        let stored = self
            .snapshot
            .read()
            .expect("status history Copilot usage state poisoned");
        let (usage, reset_date) = match stored.as_ref() {
            None => ("loading…".to_string(), None),
            Some(None) => ("couldn't fetch Copilot usage".to_string(), None),
            Some(Some(snapshot)) => {
                let usage = if snapshot.unlimited {
                    "unlimited".to_string()
                } else if let (Some(used), Some(entitlement)) =
                    (snapshot.credits_used, snapshot.entitlement)
                {
                    format!(
                        "{} / {} AI credits",
                        format_with_separators(used),
                        format_with_separators(entitlement)
                    )
                } else if let Some(remaining) = snapshot.remaining {
                    format!("{} AI credits remaining", format_with_separators(remaining))
                } else {
                    "unavailable".to_string()
                };
                (usage, snapshot.reset_date.as_deref())
            }
        };
        let mut lines = vec![formatter.line("Copilot usage", vec![Span::from(usage)])];
        if let Some(reset_date) = reset_date {
            lines.push(formatter.line("Resets on", vec![Span::from(reset_date.to_string())]));
        }
        lines
    }
}
