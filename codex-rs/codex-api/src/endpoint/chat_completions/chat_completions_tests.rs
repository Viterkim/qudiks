use super::request::translate;
use super::stream::spawn;
use crate::common::ResponseEvent;
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
    let chunks = [
        json!({"id":"test","choices":[{"delta":{"reasoning_text":"Look first."}}]}),
        json!({"choices":[{"delta":{"tool_calls":[
            {"index":0,"id":"a","function":{"name":read,"arguments":"{"}},
            {"index":1,"id":"b","function":{"name":"exec","arguments":"{\"input\":\"text(1)\"}"}}
        ]}}]}),
        json!({"choices":[{"delta":{"tool_calls":[{"index":0,"function":{"arguments":"}"}}]},"finish_reason":"tool_calls"}],"usage":{"prompt_tokens":12,"completion_tokens":9,"total_tokens":21}}),
    ];
    let mut sse = chunks
        .iter()
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
    )
    .collect::<Vec<_>>()
    .await;
    let mut history = Vec::new();
    for event in events {
        match event.unwrap() {
            ResponseEvent::OutputItemDone(item) => {
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
