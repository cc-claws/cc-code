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
