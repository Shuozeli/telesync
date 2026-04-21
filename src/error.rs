use thiserror::Error;

#[derive(Debug, Error)]
pub enum TelesyncError {
    #[error("config error: {0}")]
    Config(String),

    #[error("telegraph API error: {0}")]
    Api(String),

    #[error("telegraph API error (status {status}): {message}")]
    ApiStatus { status: u16, message: String },

    #[error("network error: {0}")]
    Network(#[from] reqwest::Error),

    #[error("markdown validation failed: {file}: {feature} (line {line})")]
    Validation {
        file: String,
        feature: String,
        line: usize,
    },

    #[error("content safety blocked: {file}: {reason} (line {line})")]
    ContentSafety {
        file: String,
        reason: String,
        line: usize,
    },

    #[error("content too large: {file} ({size_kb}KB, limit 64KB)")]
    ContentTooLarge { file: String, size_kb: usize },

    #[error("state file error: {0}")]
    State(String),

    #[error("lockfile held by PID {pid}")]
    Locked { pid: u32 },

    #[error("io error: {0}")]
    Io(#[from] std::io::Error),

    #[error("json error: {0}")]
    Json(#[from] serde_json::Error),

    #[error("confirmation required: {0}")]
    ConfirmationRequired(String),
}
