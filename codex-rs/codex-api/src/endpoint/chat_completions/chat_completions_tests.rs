use super::request::translate;
use super::stream::FinishReasonRequirement;
use super::stream::spawn;
use crate::common::ResponseEvent;
use crate::error::ApiError;
use bytes::Bytes;
use codex_client::StreamResponse;
use codex_protocol::models::ResponseItem;
use futures::StreamExt;
use http::HeaderMap;
use http::StatusCode;
use pretty_assertions::assert_eq;
use serde_json::json;
use std::time::Duration;

#[tokio::test]
async fn tool_round_trip() {
    let (request, names) = translate(json!({"model":"kimi-k3","input":[
        {"type":"message","role":"user","content":[{"type":"input_text","text":"Read and patch."}]}
    ],"tools":[{"type":"namespace","name":"files","tools":[
        {"type":"function","name":"read","parameters":{"type":"object"}}
    ]},{"type":"custom","name":"exec","description":"Run JavaScript."}]}))
    .unwrap();
    let read = request["tools"][0]["function"]["name"].as_str().unwrap();
    let (read_start, read_end) = read.split_at(5);
    let chunks = [
        json!({"id":"test","choices":[{"delta":{"reasoning_text":"Look first."}}]}),
        json!({"choices":[{"delta":{"tool_calls":[
            {"index":0,"id":"","function":{"name":read_start,"arguments":"{"}},
            {"index":1,"id":"b","function":{"name":"exec","arguments":"{\"input\":\"text(1)\"}"}}
        ]}}]}),
        json!({"choices":[{"delta":{"tool_calls":[{"index":0,"id":"a","function":{"name":read_end,"arguments":"}"}}]},"finish_reason":"tool_calls"}],"usage":{"prompt_tokens":12,"completion_tokens":9,"total_tokens":21}}),
    ];
    let mut sse = chunks
        .iter()
        .map(|chunk| format!("data: {chunk}\n\n"))
        .collect::<String>();
    sse.push_str("data: [DONE]\n\n");
    let response = StreamResponse {
        status: StatusCode::OK,
        headers: HeaderMap::new(),
        bytes: Box::pin(futures::stream::iter(
            sse.into_bytes()
                .into_iter()
                .map(|byte| Ok(Bytes::from(vec![byte]))),
        )),
    };
    let events = spawn(
        response,
        Duration::from_secs(1),
        /*telemetry*/ None,
        names,
        FinishReasonRequirement::Required,
    )
    .collect::<Vec<_>>()
    .await;
    let mut history = Vec::new();
    for event in events {
        match event.unwrap() {
            ResponseEvent::OutputItemDone(item) => {
                if let ResponseItem::FunctionCall {
                    encrypted_function_args,
                    ..
                } = &item
                {
                    assert_eq!(encrypted_function_args, &Some(Vec::new()));
                }
                history.push(serde_json::to_value(item).unwrap())
            }
            ResponseEvent::Completed {
                token_usage,
                end_turn,
                ..
            } => {
                assert_eq!(
                    (token_usage.unwrap().total_tokens, end_turn),
                    (21, Some(false))
                );
            }
            _ => {}
        }
    }
    history.push(json!({"type":"function_call_output","call_id":"a","output":[{"type":"input_text","text":"file contents"}]}));
    history.push(json!({"type":"custom_tool_call_output","call_id":"b","output":"1"}));
    let (replay, _) = translate(json!({"model":"kimi-k3","instructions":"Keep the tools working.","input":history,"tools":[
        {"type":"namespace","name":"files","tools":[{"type":"function","name":"read","parameters":{}}]},
        {"type":"custom","name":"exec","description":"Run JavaScript."}
     ]})).unwrap();
    assert_eq!(
        replay["messages"],
        json!([
            {"role":"system","content":"Keep the tools working."},
            {"role":"assistant","content":null,"reasoning_content":"Look first.","tool_calls":[
                {"id":"a","type":"function","function":{"name":read,"arguments":"{}"}},
                {"id":"b","type":"function","function":{"name":"exec","arguments":"{\"input\":\"text(1)\"}"}}
            ]},
            {"role":"tool","tool_call_id":"a","content":"file contents"},
            {"role":"tool","tool_call_id":"b","content":"1"}
        ])
    );
}

#[tokio::test]
async fn truncated_stream_never_dispatches_tools() {
    let (_, names) = translate(json!({"model":"kimi-k3","input":[],"tools":[{"type":"function","name":"remove","parameters":{}}]})).unwrap();
    let chunk = json!({"id":"test","choices":[{"delta":{"tool_calls":[{"index":0,"id":"a","function":{"name":"remove","arguments":"{}"}}]}}]});
    let response = StreamResponse {
        status: StatusCode::OK,
        headers: HeaderMap::new(),
        bytes: Box::pin(futures::stream::iter([Ok(Bytes::from(format!(
            "data: {chunk}\n\n"
        )))])),
    };
    let events = spawn(
        response,
        Duration::from_secs(1),
        /*telemetry*/ None,
        names,
        FinishReasonRequirement::Required,
    )
    .collect::<Vec<_>>()
    .await;
    assert!(events.last().unwrap().is_err());
    assert!(!events.iter().any(|event| matches!(
        event,
        Ok(ResponseEvent::OutputItemDone(
            ResponseItem::FunctionCall { .. }
        ))
    )));
}

#[tokio::test]
async fn duplicate_tool_call_ids_never_dispatch() {
    let (_, names) = translate(json!({"model":"gemini-3.8-flash","input":[],"tools":[{"type":"function","name":"remove","parameters":{}}]})).unwrap();
    let call = json!({"index":0,"id":"a","function":{"name":"remove","arguments":"{}"}});
    let duplicate = json!({"index":1,"id":"a","function":{"name":"remove","arguments":"{}"}});
    let chunk = json!({"id":"test","choices":[{"delta":{"tool_calls":[call,duplicate]},"finish_reason":"tool_calls"}]});
    let response = StreamResponse {
        status: StatusCode::OK,
        headers: HeaderMap::new(),
        bytes: Box::pin(futures::stream::iter([Ok(Bytes::from(format!(
            "data: {chunk}\n\ndata: [DONE]\n\n"
        )))])),
    };
    let events = spawn(
        response,
        Duration::from_secs(1),
        /*telemetry*/ None,
        names,
        FinishReasonRequirement::Required,
    )
    .collect::<Vec<_>>()
    .await;

    assert!(!events.iter().any(|event| matches!(
        event,
        Ok(ResponseEvent::OutputItemDone(
            ResponseItem::FunctionCall { .. }
        ))
    )));
}

#[tokio::test]
async fn gemini_done_without_finish_reason() {
    let response_text = json!({"choices":[{"delta":{"content":"All done."},"finish_reason":null}]});
    let reasoning_only = json!({"choices":[{"delta":{"reasoning_text":"Thinking."}}]});
    let length_limit = json!({"choices":[{"delta":{},"finish_reason":"length"}]});
    let tool_chunk = json!({"choices":[{"delta":{"tool_calls":[
        {"index":0,"id":"a","function":{"name":"lookup","arguments":"{}"}}
    ]}}]});
    let error = json!({"choices":null,"copilot_errors":[{"message":"Provider failed"}]});
    for (chunks, finish_reason, expected_error) in [
        (
            vec![&response_text],
            FinishReasonRequirement::OptionalForText,
            None,
        ),
        (
            vec![&response_text],
            FinishReasonRequirement::Required,
            Some("Chat Completions sent [DONE] without a finish reason"),
        ),
        (
            vec![&reasoning_only],
            FinishReasonRequirement::OptionalForText,
            Some("Chat Completions sent [DONE] without a finish reason"),
        ),
        (
            vec![&response_text, &tool_chunk],
            FinishReasonRequirement::OptionalForText,
            Some("Chat Completions sent [DONE] without a finish reason"),
        ),
        (
            vec![&response_text, &error],
            FinishReasonRequirement::OptionalForText,
            Some("Copilot stream error: Provider failed"),
        ),
        (
            vec![&response_text, &length_limit],
            FinishReasonRequirement::OptionalForText,
            Some("incomplete Chat Completions response: length"),
        ),
    ] {
        let (_, names) = translate(json!({"model":"gemini-4-neon","input":[],"tools":[
            {"type":"function","name":"lookup","parameters":{}}
        ]}))
        .unwrap();
        let mut sse = chunks
            .into_iter()
            .map(|chunk| format!("data: {chunk}\n\n"))
            .collect::<String>();
        sse.push_str("data: [DONE]\n\n");
        let response = StreamResponse {
            status: StatusCode::OK,
            headers: HeaderMap::new(),
            bytes: Box::pin(futures::stream::iter([Ok(Bytes::from(sse))])),
        };
        let events = spawn(
            response,
            Duration::from_secs(1),
            /*telemetry*/ None,
            names,
            finish_reason,
        )
        .collect::<Vec<_>>()
        .await;

        assert!(!events.iter().any(|event| matches!(
            event,
            Ok(ResponseEvent::OutputItemDone(
                ResponseItem::FunctionCall { .. }
            ))
        )));
        match expected_error {
            Some(expected) => {
                let Err(ApiError::Stream(message)) = events.last().unwrap() else {
                    panic!("expected stream error, got {events:?}");
                };
                assert_eq!(message, expected);
            }
            None => assert!(matches!(
                events.as_slice(),
                [
                    Ok(ResponseEvent::Created { .. }),
                    Ok(ResponseEvent::OutputItemAdded(ResponseItem::Message { .. })),
                    Ok(ResponseEvent::OutputTextDelta(_)),
                    Ok(ResponseEvent::OutputItemDone(ResponseItem::Message { .. })),
                    Ok(ResponseEvent::Completed {
                        end_turn: Some(true),
                        ..
                    })
                ]
            )),
        }
    }
}

#[test]
fn replay_parallel_results_with_images() {
    let (request, _) = translate(json!({"model":"gemini-3.8-flash","input":[
        {"type":"function_call","call_id":"a","name":"read","arguments":"{}"},
        {"type":"custom_tool_call","call_id":"b","name":"exec","input":"text(1)"},
        {"type":"function_call_output","call_id":"a","output":[
            {"type":"input_text","text":"image result"},
            {"type":"input_image","image_url":"data:image/png;base64,AAA","detail":"high"}
        ]},
        {"type":"custom_tool_call_output","call_id":"b","output":"{\"isError\":true,\"content\":[{\"type\":\"text\",\"text\":\"permission denied\"}]}"},
        {"type":"message","role":"user","content":[{"type":"input_text","text":"Try again."}]}
    ],"tools":[
        {"type":"function","name":"read","parameters":{"type":"object"}},
        {"type":"custom","name":"exec"}
    ]})).unwrap();
    assert_eq!(
        request["messages"],
        json!([
            {"role":"assistant","content":null,"tool_calls":[
                {"id":"a","type":"function","function":{"name":"read","arguments":"{}"}},
                {"id":"b","type":"function","function":{"name":"exec","arguments":"{\"input\":\"text(1)\"}"}}
            ]},
            {"role":"tool","tool_call_id":"a","content":"image result"},
            {"role":"tool","tool_call_id":"b","content":"{\"isError\":true,\"content\":[{\"type\":\"text\",\"text\":\"permission denied\"}]}"},
            {"role":"user","content":[{"type":"image_url","image_url":{"url":"data:image/png;base64,AAA","detail":"high"}}]},
            {"role":"user","content":[{"type":"text","text":"Try again."}]}
        ])
    );
}
