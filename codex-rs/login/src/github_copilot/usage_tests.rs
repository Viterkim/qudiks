use super::*;
use pretty_assertions::assert_eq;

#[test]
fn formats_premium_credits_like_the_github_ui() {
    let snapshot = CopilotQuotaSnapshot {
        plan: Some("business".to_string()),
        reset_date: Some("2026-10-01".to_string()),
        credits_used: Some(2434),
        entitlement: Some(20_000),
        remaining: Some(17_565),
        unlimited: false,
    };
    assert_eq!(
        format_quota_snapshot(&snapshot),
        "GitHub Copilot usage\nPlan: business\nUsage this cycle: 2,434 / 20,000 AI credits\nResets on 2026-10-01"
    );
}

#[test]
fn formats_unlimited_chat_as_unlimited() {
    let snapshot = CopilotQuotaSnapshot {
        plan: Some("business".to_string()),
        reset_date: Some("2026-10-01".to_string()),
        credits_used: Some(0),
        entitlement: Some(0),
        remaining: Some(0),
        unlimited: true,
    };
    assert_eq!(
        format_quota_snapshot(&snapshot),
        "GitHub Copilot usage\nPlan: business\nUsage this cycle: unlimited\nResets on 2026-10-01"
    );
}

#[test]
fn parses_premium_interactions_from_the_user_payload() {
    let payload: UserPayload = serde_json::from_value(serde_json::json!({
        "copilot_plan": "business",
        "quota_reset_date": "2026-10-01",
        "quota_snapshots": {
            "premium_interactions": {
                "credits_used": 2434,
                "entitlement": 20000,
                "remaining": 17565,
                "unlimited": false
            }
        }
    }))
    .expect("payload parses");
    assert_eq!(
        snapshot_from_payload(payload),
        CopilotQuotaSnapshot {
            plan: Some("business".to_string()),
            reset_date: Some("2026-10-01".to_string()),
            credits_used: Some(2434),
            entitlement: Some(20_000),
            remaining: Some(17_565),
            unlimited: false,
        }
    );
}
