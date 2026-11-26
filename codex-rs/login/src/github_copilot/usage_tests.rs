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
            "chat": {"unlimited": true},
            "completions": {"unlimited": true},
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

#[test]
fn finite_premium_allowance_takes_precedence_over_unlimited_flag() {
    let payload: UserPayload = serde_json::from_value(serde_json::json!({
        "quota_snapshots": {"premium_interactions": {
            "credits_used": 12004,
            "entitlement": 30000,
            "remaining": 17990,
            "unlimited": true
        }}
    }))
    .unwrap();
    assert_eq!(
        snapshot_from_payload(payload),
        CopilotQuotaSnapshot {
            plan: None,
            reset_date: None,
            credits_used: Some(12004),
            entitlement: Some(30000),
            remaining: Some(17990),
            unlimited: false,
        }
    );
}

#[test]
fn legacy_remaining_quota_can_supply_missing_usage() {
    for (window, used, entitlement, remaining, unlimited) in [
        (
            serde_json::json!({"entitlement":30000,"remaining":18000}),
            Some(12000),
            Some(30000),
            Some(18000),
            false,
        ),
        (
            serde_json::json!({"entitlement":0,"remaining":0,"unlimited":true}),
            None,
            Some(0),
            Some(0),
            true,
        ),
        (serde_json::json!({}), None, None, None, false),
    ] {
        let payload = serde_json::from_value(
            serde_json::json!({"quota_snapshots":{"premium_interactions":window}}),
        )
        .unwrap();
        assert_eq!(
            snapshot_from_payload(payload),
            CopilotQuotaSnapshot {
                plan: None,
                reset_date: None,
                credits_used: used,
                entitlement,
                remaining,
                unlimited,
            }
        );
    }
}
