use codex_model_provider_info::ModelProviderInfo;
use codex_protocol::models::ContentItem;
use codex_protocol::models::FunctionCallOutputContentItem;
use codex_protocol::models::ResponseItem;
use codex_protocol::openai_models::ModelInfo;
use codex_protocol::protocol::SessionSource;
use codex_tools::ToolSpec;
use http::HeaderMap;
use http::HeaderValue;
use std::collections::HashSet;

/// Setup writes this into `model_providers.<id>.name`; they must not drift.
pub const GITHUB_COPILOT_PROVIDER_NAME: &str = "GitHub Copilot";
const INTENT_HEADER: &str = "Openai-Intent";
const INITIATOR_HEADER: &str = "X-Initiator";
const VISION_HEADER: &str = "Copilot-Vision-Request";

pub(super) fn supports_namespace_tools(
    provider: &ModelProviderInfo,
    model_info: &ModelInfo,
) -> bool {
    provider.name != GITHUB_COPILOT_PROVIDER_NAME || !model_info.slug.starts_with("grok")
}

/// Echoing back `reasoning.encrypted_content` 400s with "Could not decode the
/// compaction blob". Safe to drop: opaque cache token, `summary` still goes.
pub(super) fn prepare_request_items(provider: &ModelProviderInfo, input: &mut [ResponseItem]) {
    if provider.name != GITHUB_COPILOT_PROVIDER_NAME {
        return;
    }
    for item in input {
        if let ResponseItem::Reasoning {
            encrypted_content, ..
        } = item
        {
            *encrypted_content = None;
        }
    }
}

pub(super) fn prepare_model_request_items(
    provider: &ModelProviderInfo,
    model_info: &ModelInfo,
    input: &mut [ResponseItem],
) {
    if provider.name != GITHUB_COPILOT_PROVIDER_NAME || !model_info.slug.starts_with("grok") {
        return;
    }
    let patch_call_ids: HashSet<String> = input
        .iter()
        .filter_map(|item| match item {
            ResponseItem::CustomToolCall { name, call_id, .. } if name == "apply_patch" => {
                Some(call_id.clone())
            }
            _ => None,
        })
        .collect();
    for item in input {
        match item {
            ResponseItem::CustomToolCall {
                name,
                namespace,
                call_id,
                input,
                internal_chat_message_metadata_passthrough,
                ..
            } if name == "apply_patch" => {
                *item = ResponseItem::FunctionCall {
                    id: None,
                    name: name.clone(),
                    namespace: namespace.clone(),
                    arguments: serde_json::json!({"patch": input}).to_string(),
                    encrypted_function_args: None,
                    call_id: call_id.clone(),
                    internal_chat_message_metadata_passthrough:
                        internal_chat_message_metadata_passthrough.clone(),
                };
            }
            ResponseItem::CustomToolCallOutput {
                call_id,
                name,
                output,
                internal_chat_message_metadata_passthrough,
                ..
            } if patch_call_ids.contains(call_id) => {
                *item = ResponseItem::FunctionCallOutput {
                    id: None,
                    call_id: Some(call_id.clone()),
                    name: name.clone(),
                    namespace: None,
                    output: output.clone(),
                    internal_chat_message_metadata_passthrough:
                        internal_chat_message_metadata_passthrough.clone(),
                };
            }
            _ => {}
        }
    }
}

/// Copilot 400s on `external_web_access`, killing the request. Dropping it
/// leaves web search working.
pub(super) fn prepare_request_tools(provider: &ModelProviderInfo, tools: &mut [ToolSpec]) {
    if provider.name != GITHUB_COPILOT_PROVIDER_NAME {
        return;
    }
    for tool in tools {
        if let ToolSpec::WebSearch {
            external_web_access,
            indexed_web_access,
            ..
        } = tool
        {
            *external_web_access = None;
            *indexed_web_access = None;
        }
    }
}

pub(super) fn responses_headers(
    provider: &ModelProviderInfo,
    input: &[ResponseItem],
    session_source: &SessionSource,
) -> HeaderMap {
    if provider.name != GITHUB_COPILOT_PROVIDER_NAME {
        return HeaderMap::new();
    }

    let mut headers = HeaderMap::new();
    headers.insert(
        INTENT_HEADER,
        HeaderValue::from_static("conversation-edits"),
    );
    headers.insert(
        INITIATOR_HEADER,
        HeaderValue::from_static(request_initiator(input, session_source)),
    );
    if contains_image_input(input) {
        headers.insert(VISION_HEADER, HeaderValue::from_static("true"));
    }
    headers
}

fn request_initiator(input: &[ResponseItem], session_source: &SessionSource) -> &'static str {
    if matches!(session_source, SessionSource::SubAgent(_)) {
        return "agent";
    }

    match input.last() {
        Some(ResponseItem::Message { role, .. }) if role == "user" => "user",
        Some(_) => "agent",
        None => "user",
    }
}

fn contains_image_input(input: &[ResponseItem]) -> bool {
    input.iter().any(|item| match item {
        ResponseItem::Message { content, .. } => content
            .iter()
            .any(|item| matches!(item, ContentItem::InputImage { .. })),
        ResponseItem::FunctionCallOutput { output, .. }
        | ResponseItem::CustomToolCallOutput { output, .. } => {
            output.content_items().is_some_and(|items| {
                items
                    .iter()
                    .any(|item| matches!(item, FunctionCallOutputContentItem::InputImage { .. }))
            })
        }
        ResponseItem::AdditionalTools { .. }
        | ResponseItem::AgentMessage { .. }
        | ResponseItem::Reasoning { .. }
        | ResponseItem::LocalShellCall { .. }
        | ResponseItem::FunctionCall { .. }
        | ResponseItem::ToolSearchCall { .. }
        | ResponseItem::CustomToolCall { .. }
        | ResponseItem::ToolSearchOutput { .. }
        | ResponseItem::WebSearchCall { .. }
        | ResponseItem::ImageGenerationCall { .. }
        | ResponseItem::Compaction { .. }
        | ResponseItem::CompactionTrigger { .. }
        | ResponseItem::ContextCompaction { .. }
        | ResponseItem::ConfigurationUpdate { .. }
        | ResponseItem::Other => false,
    })
}

#[cfg(test)]
#[path = "github_copilot_tests.rs"]
mod tests;
