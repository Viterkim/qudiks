use super::ChatWidget;
use ratatui::text::Line;

impl ChatWidget {
    pub(super) fn show_api_usage(&mut self) {
        let usage = self.token_usage();
        let provider = if self.config.model_provider_id == "openai-api" {
            "OpenAI API"
        } else {
            "Custom endpoint"
        };
        let mut message = format!(
            "{provider} session usage\nInput: {} tokens ({} cached)\nOutput: {} tokens ({} reasoning)\nTotal: {} tokens",
            usage.input_tokens,
            usage.cached_input_tokens,
            usage.output_tokens,
            usage.reasoning_output_tokens,
            usage.total_tokens,
        );
        if self.config.model_provider_id == "openai-api" {
            message.push_str("\n\nAPI usage: https://platform.openai.com/usage\nCredits and billing: https://platform.openai.com/settings/organization/billing/overview\nRemaining organization credits are shown in Platform billing.");
        }
        self.add_plain_history_lines(
            message
                .lines()
                .map(|line| Line::from(line.to_string()))
                .collect(),
        );
    }
}
