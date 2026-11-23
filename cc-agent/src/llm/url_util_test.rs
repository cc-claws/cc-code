use super::apply_opencode_session_header;
use super::url_util::*;

#[test]
fn test_normalize_empty_url() {
    assert_eq!(normalize_base_url("", "anthropic"), "");
    assert_eq!(normalize_base_url("   ", "openai"), "");
}

#[test]
fn test_normalize_anthropic_local_host_without_scheme() {
    assert_eq!(
        normalize_base_url("127.0.0.1:20128", "anthropic"),
        "http://127.0.0.1:20128"
    );
    assert_eq!(
        normalize_base_url("localhost:8000", "anthropic"),
        "http://localhost:8000"
    );
    assert_eq!(
        normalize_base_url("192.168.1.50:20128", "anthropic"),
        "http://192.168.1.50:20128"
    );
}

#[test]
fn test_normalize_anthropic_strip_v1_and_messages() {
    assert_eq!(
        normalize_base_url("127.0.0.1:20128/v1", "anthropic"),
        "http://127.0.0.1:20128"
    );
    assert_eq!(
        normalize_base_url("http://127.0.0.1:20128/v1/", "anthropic"),
        "http://127.0.0.1:20128"
    );
    assert_eq!(
        normalize_base_url("http://127.0.0.1:20128/v1/messages", "anthropic"),
        "http://127.0.0.1:20128"
    );
    assert_eq!(
        normalize_base_url("https://api.anthropic.com/v1", "anthropic"),
        "https://api.anthropic.com"
    );
}

#[test]
fn test_normalize_openai_local_and_public_urls() {
    assert_eq!(
        normalize_base_url("127.0.0.1:20128", "openai"),
        "http://127.0.0.1:20128/v1"
    );
    assert_eq!(
        normalize_base_url("api.deepseek.com", "openai"),
        "https://api.deepseek.com/v1"
    );
    assert_eq!(
        normalize_base_url("https://openrouter.ai/api/v1", "openai"),
        "https://openrouter.ai/api/v1"
    );
    assert_eq!(
        normalize_base_url("https://api.openai.com/v1/chat/completions", "openai"),
        "https://api.openai.com/v1"
    );
}

#[test]
fn test_is_opencode_endpoint_matches_opencode_hosts() {
    assert!(is_opencode_endpoint(
        "https://opencode.ai/inference/go/anthropic/v1/messages"
    ));
    assert!(is_opencode_endpoint(
        "https://opencode.ai/inference/go/openai/v1/chat/completions"
    ));
    assert!(is_opencode_endpoint("https://api.opencode.ai/v1/messages"));
}

#[test]
fn test_is_opencode_endpoint_rejects_other_hosts() {
    assert!(!is_opencode_endpoint(
        "https://api.anthropic.com/v1/messages"
    ));
    assert!(!is_opencode_endpoint(
        "https://opencode.ai.evil.com/v1/messages"
    ));
    assert!(!is_opencode_endpoint(
        "http://127.0.0.1:20128/v1/chat/completions"
    ));
    assert!(!is_opencode_endpoint("not a url"));
}

#[test]
fn test_apply_opencode_session_header_injects_when_session_id_present() {
    let req = apply_opencode_session_header(
        reqwest::Client::new().post("https://opencode.ai/inference/go/anthropic/v1/messages"),
        Some("session-abc"),
    );
    let built = req.build().unwrap();
    assert_eq!(
        built.headers().get("x-opencode-session").unwrap(),
        "session-abc"
    );
}

#[test]
fn test_apply_opencode_session_header_injects_behind_relay_host() {
    let req = apply_opencode_session_header(
        reqwest::Client::new().post("https://relay.example.com/opencode/v1/messages"),
        Some("session-abc"),
    );
    let built = req.build().unwrap();
    assert_eq!(
        built.headers().get("x-opencode-session").unwrap(),
        "session-abc"
    );
}

#[test]
fn test_apply_opencode_session_header_skips_without_session_id() {
    let req = apply_opencode_session_header(
        reqwest::Client::new().post("https://opencode.ai/inference/go/anthropic/v1/messages"),
        None,
    );
    let built = req.build().unwrap();
    assert!(built.headers().get("x-opencode-session").is_none());
}
