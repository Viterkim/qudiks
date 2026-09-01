use super::*;
use pretty_assertions::assert_eq;

#[test]
fn normalizes_domains() {
    assert_eq!(
        normalize_github_domain("GitHub.COM.").ok(),
        Some("github.com".to_string())
    );
    assert_eq!(
        normalize_github_domain(" https://github.example.com ").ok(),
        Some("github.example.com".to_string())
    );
}

#[test]
fn rejects_domains_that_could_redirect_credentials() {
    for input in [
        "",
        "http://github.com",
        "user:pass@github.com",
        "github.com:8443",
        "git..hub.com",
    ] {
        assert!(
            normalize_github_domain(input).is_err(),
            "expected `{input}` to be rejected"
        );
    }
}

#[test]
fn rewrites_a_proxy_directive_to_the_api_host() {
    for (token, expected) in [
        ("tid=abc", DEFAULT_COPILOT_API_BASE_URL),
        (
            "tid=abc;proxy-ep=proxy.enterprise.githubcopilot.com;exp=1",
            "https://api.enterprise.githubcopilot.com",
        ),
        (
            "proxy-ep=https://api.enterprise.githubcopilot.com",
            "https://api.enterprise.githubcopilot.com",
        ),
    ] {
        assert_eq!(
            derive_copilot_api_base_url(token, /*enterprise_domain*/ None).ok(),
            Some(expected.to_string()),
            "token: {token}"
        );
    }
}

#[test]
fn refuses_proxy_directives_pointing_off_the_allowlist() {
    for token in [
        "proxy-ep=evil.example.com",
        "proxy-ep=http://api.githubcopilot.com",
        "proxy-ep=https://api.githubcopilot.com.evil.example.com",
        "proxy-ep=https://api.githubcopilot.com:8443",
        "proxy-ep=https://user:pass@api.githubcopilot.com",
        "proxy-ep=api.githubcopilot.com/../evil",
    ] {
        assert!(
            derive_copilot_api_base_url(token, /*enterprise_domain*/ None).is_err(),
            "expected `{token}` to be rejected"
        );
    }
}

#[test]
fn trusts_an_enterprise_host_only_for_its_own_domain() {
    let url = "https://copilot-api.github.example.com";
    assert!(assert_trusted_copilot_api_url(url, Some("github.example.com")).is_ok());
    assert!(assert_trusted_copilot_api_url(url, Some("github.other.com")).is_err());
    assert!(assert_trusted_copilot_api_url(url, /*enterprise_domain*/ None).is_err());
}

#[test]
fn refuses_copilot_urls_carrying_a_path_or_query() {
    for url in [
        "https://api.githubcopilot.com/v1",
        "https://api.githubcopilot.com/?a=b",
        "https://api.githubcopilot.com/#frag",
    ] {
        assert!(
            assert_trusted_copilot_api_url(url, /*enterprise_domain*/ None).is_err(),
            "expected `{url}` to be rejected"
        );
    }
}
