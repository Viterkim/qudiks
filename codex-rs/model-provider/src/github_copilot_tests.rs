use codex_model_provider_info::ModelProviderInfo;
use codex_protocol::models::ContentItem;
use codex_protocol::models::ResponseItem;
use codex_protocol::protocol::SessionSource;
use codex_protocol::protocol::SubAgentSource;
use codex_tools::ToolSpec;
use pretty_assertions::assert_eq;

use super::prepare_request_items;
use super::prepare_request_tools;
use super::responses_headers;

fn copilot() -> ModelProviderInfo {
    ModelProviderInfo {
        name: "GitHub Copilot".to_string(),
        ..ModelProviderInfo::default()
    }
}

fn message(role: &str, content: Vec<ContentItem>) -> ResponseItem {
    ResponseItem::Message {
        id: None,
        role: role.to_string(),
        content,
        phase: None,
        internal_chat_message_metadata_passthrough: None,
    }
}

fn text(role: &str) -> ResponseItem {
    message(
        role,
        vec![ContentItem::InputText {
            text: "hi".to_string(),
        }],
    )
}

fn header<'a>(headers: &'a http::HeaderMap, name: &str) -> Option<&'a str> {
    headers.get(name).and_then(|value| value.to_str().ok())
}

#[test]
fn marks_the_initiator_from_the_last_item_and_session() {
    let user = responses_headers(&copilot(), &[text("user")], &SessionSource::Cli);
    assert_eq!(header(&user, "Openai-Intent"), Some("conversation-edits"));
    assert_eq!(header(&user, "X-Initiator"), Some("user"));

    let agent = responses_headers(&copilot(), &[text("assistant")], &SessionSource::Cli);
    assert_eq!(header(&agent, "X-Initiator"), Some("agent"));

    let subagent = responses_headers(
        &copilot(),
        &[text("user")],
        &SessionSource::SubAgent(SubAgentSource::Review),
    );
    assert_eq!(header(&subagent, "X-Initiator"), Some("agent"));
}

#[test]
fn flags_vision_requests() {
    let headers = responses_headers(
        &copilot(),
        &[message(
            "user",
            vec![ContentItem::InputImage {
                image_url: "data:image/png;base64,AA".to_string(),
                detail: None,
            }],
        )],
        &SessionSource::Cli,
    );
    assert_eq!(header(&headers, "Copilot-Vision-Request"), Some("true"));
}

#[test]
fn leaves_other_providers_alone() {
    let openai = ModelProviderInfo::default();
    assert!(responses_headers(&openai, &[text("user")], &SessionSource::Cli).is_empty());

    let mut items = vec![reasoning_with_blob()];
    prepare_request_items(&openai, &mut items);
    let ResponseItem::Reasoning {
        encrypted_content, ..
    } = &items[0]
    else {
        panic!("expected reasoning");
    };
    assert!(encrypted_content.is_some());
}

fn reasoning_with_blob() -> ResponseItem {
    ResponseItem::Reasoning {
        id: None,
        summary: Vec::new(),
        content: None,
        encrypted_content: Some("blob".to_string()),
        internal_chat_message_metadata_passthrough: None,
    }
}

#[test]
fn strips_the_fields_copilot_rejects() {
    let mut items = vec![reasoning_with_blob()];
    prepare_request_items(&copilot(), &mut items);
    let ResponseItem::Reasoning {
        encrypted_content, ..
    } = &items[0]
    else {
        panic!("expected reasoning");
    };
    assert_eq!(encrypted_content, &None);

    let mut tools = vec![ToolSpec::WebSearch {
        external_web_access: Some(true),
        indexed_web_access: Some(true),
        filters: None,
        user_location: None,
        search_context_size: None,
        search_content_types: None,
    }];
    prepare_request_tools(&copilot(), &mut tools);
    let ToolSpec::WebSearch {
        external_web_access,
        indexed_web_access,
        ..
    } = &tools[0]
    else {
        panic!("expected web search");
    };
    assert_eq!(external_web_access, &None);
    assert_eq!(indexed_web_access, &None);
}
