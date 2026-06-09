use thiserror::Error;

#[derive(Error, Debug)]
pub enum LlmError {
    #[error("HTTP request failed: {0}")]
    HttpError(#[from] reqwest::Error),

    #[error("API error: status {status} - {message}")]
    ApiError { status: u16, message: String },

    #[error("failed to parse response: {0}")]
    ParseError(String),

    #[error("configuration error: {0}")]
    ConfigError(String),

    #[error("unsupported protocol: {0}")]
    UnsupportedProtocol(String),
}
