use super::normalize_endpoint;
use crate::MultitoolCli;
use clap::Parser;
use pretty_assertions::assert_eq;

#[test]
fn endpoint_urls() {
    for (input, expected) in [
        ("127.0.0.1:8080", "http://127.0.0.1:8080/v1"),
        ("http://localhost:8080/", "http://localhost:8080/v1"),
        ("https://example.com/api/v1/", "https://example.com/api/v1"),
    ] {
        assert_eq!(normalize_endpoint(input).unwrap(), expected);
    }
    for input in [
        "file:///tmp/server",
        "https://user:secret@example.com",
        "https://example.com?key=secret",
        "https://example.com#fragment",
    ] {
        assert!(normalize_endpoint(input).is_err(), "{input}");
    }
}

#[test]
fn provider_flags() {
    assert!(
        !MultitoolCli::try_parse_from(["qudiks"])
            .unwrap()
            .provider_mode
            .selected()
    );
    for arguments in [
        vec!["qudiks", "--github"],
        vec!["qudiks", "--codex"],
        vec!["qudiks", "resume", "--last", "--codex"],
        vec!["qudiks", "exec", "--codex", "hello"],
        vec!["qudiks", "--openai-api", "resume", "--last"],
        vec!["qudiks", "fork", "--endpoint", "localhost:8080"],
        vec!["qudiks", "exec", "--openai-api", "hello"],
    ] {
        assert!(
            MultitoolCli::try_parse_from(arguments)
                .unwrap()
                .provider_mode
                .selected()
        );
    }
    for arguments in [
        vec!["qudiks", "--github", "--openai-api"],
        vec!["qudiks", "--github", "--codex"],
        vec!["qudiks", "--codex", "--openai-api"],
        vec!["qudiks", "--codex", "--endpoint", "localhost:8080"],
        vec!["qudiks", "--github", "--endpoint", "localhost:8080"],
        vec![
            "qudiks",
            "exec",
            "--openai-api",
            "--endpoint",
            "localhost:8080",
        ],
    ] {
        assert!(MultitoolCli::try_parse_from(arguments).is_err());
    }
}
