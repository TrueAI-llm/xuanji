use std::str::FromStr;
use xuanji_llm::{Protocol, ProviderConfig};

#[test]
fn test_protocol_from_str_openai() {
    assert_eq!(Protocol::from_str("openai").unwrap(), Protocol::OpenAI);
}

#[test]
fn test_protocol_from_str_case_insensitive() {
    assert_eq!(Protocol::from_str("OpenAI").unwrap(), Protocol::OpenAI);
    assert_eq!(Protocol::from_str("ANTHROPIC").unwrap(), Protocol::Anthropic);
    assert_eq!(Protocol::from_str("Gemini").unwrap(), Protocol::Gemini);
}

#[test]
fn test_protocol_from_str_unknown() {
    assert!(Protocol::from_str("unknown").is_err());
    assert!(Protocol::from_str("").is_err());
}

#[test]
fn test_protocol_display() {
    assert_eq!(format!("{}", Protocol::OpenAI), "openai");
    assert_eq!(format!("{}", Protocol::Anthropic), "anthropic");
    assert_eq!(format!("{}", Protocol::Gemini), "gemini");
}

#[test]
fn test_provider_config_toml_parsing() {
    let toml_str = r#"
        protocol = "openai"
        model = "gpt-4"
        api_key = "sk-test-key"
        base_url = "https://api.openai.com"
        max_tokens = 4096
        temperature = 0.7
    "#;
    let config: ProviderConfig = toml::from_str(toml_str).unwrap();
    assert_eq!(config.protocol, Protocol::OpenAI);
    assert_eq!(config.model, "gpt-4");
    assert_eq!(config.api_key, "sk-test-key");
    assert_eq!(config.base_url.as_deref(), Some("https://api.openai.com"));
    assert_eq!(config.max_tokens, Some(4096));
    assert!((config.temperature.unwrap() - 0.7).abs() < f64::EPSILON);
}

#[test]
fn test_provider_config_minimal() {
    let toml_str = r#"
        protocol = "anthropic"
        model = "claude-sonnet-4-20250514"
        api_key = "sk-ant-test"
    "#;
    let config: ProviderConfig = toml::from_str(toml_str).unwrap();
    assert_eq!(config.protocol, Protocol::Anthropic);
    assert_eq!(config.model, "claude-sonnet-4-20250514");
    assert!(config.base_url.is_none());
    assert!(config.max_tokens.is_none());
    assert!(config.temperature.is_none());
}
