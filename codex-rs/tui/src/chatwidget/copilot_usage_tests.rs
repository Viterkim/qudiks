use super::*;
use crate::chatwidget::tests::make_chatwidget_manual_with_sender;
use crate::slash_command::SlashCommand;
use pretty_assertions::assert_eq;

#[tokio::test]
async fn copilot_usage_uses_cached_quota_and_ignores_unrelated_completions() {
    let (mut chat, _sender, mut events, mut ops) = make_chatwidget_manual_with_sender().await;
    chat.config.model_provider_id = "github-copilot".into();
    let request_id = Uuid::new_v4();
    let snapshot = CopilotQuotaSnapshot {
        plan: Some("business".into()),
        reset_date: None,
        credits_used: Some(11800),
        entitlement: Some(30000),
        remaining: Some(18200),
        unlimited: false,
    };
    chat.copilot_usage.pending = Some(request_id);
    chat.copilot_usage.last_attempt = Some(Instant::now());
    chat.dispatch_command(SlashCommand::Usage);
    chat.dispatch_command(SlashCommand::Usage);
    assert_eq!(chat.copilot_usage.pending, Some(request_id));
    assert!(events.try_recv().is_err());

    chat.on_copilot_usage_loaded(Uuid::new_v4(), Err("old conversation".into()));
    assert!(chat.copilot_usage.result.is_none());
    assert_eq!(chat.copilot_usage.pending, Some(request_id));

    chat.on_copilot_usage_loaded(request_id, Ok(snapshot.clone()));
    let AppEvent::InsertHistoryCell(cell) = events.try_recv().expect("quota message") else {
        panic!("expected quota history cell");
    };
    let text = cell
        .display_lines(100)
        .iter()
        .map(ToString::to_string)
        .collect::<Vec<_>>()
        .join("\n");
    assert!(text.contains("11,800 / 30,000 AI credits"), "{text}");
    assert_eq!(chat.copilot_usage.result, Some(Ok(snapshot)));

    chat.dispatch_command(SlashCommand::Usage);
    assert!(matches!(
        events.try_recv(),
        Ok(AppEvent::InsertHistoryCell(_))
    ));
    assert_eq!(chat.copilot_usage.pending, None);
    assert!(ops.try_recv().is_err());
}
