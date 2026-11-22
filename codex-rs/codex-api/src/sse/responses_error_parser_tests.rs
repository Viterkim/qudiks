//! Covers retry-delay parsing for provider error messages.

use super::retry_after_from_message;
use codex_http_client::RetryAfter;
use pretty_assertions::assert_eq;
use std::time::Duration;

#[tokio::test(start_paused = true)]
async fn test_try_parse_retry_delay() {
    let message = "Rate limit reached for gpt-5.1 in organization org- on tokens per min (TPM): Limit 1, Used 1, Requested 19304. Please try again in 28ms. Visit https://platform.openai.com/account/rate-limits to learn more.";
    assert_eq!(
        retry_after_from_message(message).map(RetryAfter::remaining_delay),
        Some(Duration::from_millis(28)),
    );
}

#[tokio::test(start_paused = true)]
async fn test_try_parse_retry_delay_no_delay() {
    let message = "Rate limit reached for gpt-5.1 in organization <ORG> on tokens per min (TPM): Limit 30000, Used 6899, Requested 24050. Please try again in 1.898s. Visit https://platform.openai.com/account/rate-limits to learn more.";
    assert_eq!(
        retry_after_from_message(message).map(RetryAfter::remaining_delay),
        Some(Duration::from_secs_f64(1.898)),
    );
}

#[tokio::test(start_paused = true)]
async fn test_try_parse_retry_delay_azure() {
    assert_eq!(
        retry_after_from_message("Rate limit exceeded. Try again in 35 seconds.")
            .map(RetryAfter::remaining_delay),
        Some(Duration::from_secs(35)),
    );
}
