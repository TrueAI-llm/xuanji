use serde::{Deserialize, Serialize};
use std::fmt;
use std::str::FromStr;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Protocol {
    OpenAI,
    Anthropic,
    Gemini,
}

impl FromStr for Protocol {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s.to_lowercase().as_str() {
            "openai" => Ok(Protocol::OpenAI),
            "anthropic" => Ok(Protocol::Anthropic),
            "gemini" => Ok(Protocol::Gemini),
            _ => Err(format!("unsupported protocol: {s}")),
        }
    }
}

impl fmt::Display for Protocol {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Protocol::OpenAI => write!(f, "openai"),
            Protocol::Anthropic => write!(f, "anthropic"),
            Protocol::Gemini => write!(f, "gemini"),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_from_str() {
        assert_eq!(Protocol::from_str("openai").unwrap(), Protocol::OpenAI);
        assert_eq!(Protocol::from_str("OpenAI").unwrap(), Protocol::OpenAI);
        assert_eq!(Protocol::from_str("anthropic").unwrap(), Protocol::Anthropic);
        assert_eq!(Protocol::from_str("ANTHROPIC").unwrap(), Protocol::Anthropic);
        assert_eq!(Protocol::from_str("gemini").unwrap(), Protocol::Gemini);
        assert!(Protocol::from_str("unknown").is_err());
    }

    #[test]
    fn test_display() {
        assert_eq!(format!("{}", Protocol::OpenAI), "openai");
        assert_eq!(format!("{}", Protocol::Anthropic), "anthropic");
        assert_eq!(format!("{}", Protocol::Gemini), "gemini");
    }

    #[test]
    fn test_serde_roundtrip() {
        let json = serde_json::to_string(&Protocol::OpenAI).unwrap();
        assert_eq!(json, "\"openai\"");
        let parsed: Protocol = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed, Protocol::OpenAI);
    }
}
