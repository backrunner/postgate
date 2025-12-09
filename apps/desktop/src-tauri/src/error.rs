use serde::Serialize;
use thiserror::Error;

#[derive(Error, Debug)]
pub enum PostGateError {
    #[error("Proxy error: {0}")]
    Proxy(String),

    #[error("Certificate error: {0}")]
    Certificate(String),

    #[error("Rule parse error: {0}")]
    RuleParse(String),

    #[error("Storage error: {0}")]
    Storage(String),

    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),

    #[error("JSON error: {0}")]
    Json(#[from] serde_json::Error),

    #[error("Database error: {0}")]
    Database(#[from] sqlx::Error),

    #[error("Invalid state: {0}")]
    InvalidState(String),

    #[error("Not found: {0}")]
    NotFound(String),
}

// Implement Serialize for Tauri command return type
impl serde::Serialize for PostGateError {
    fn serialize<S>(&self, serializer: S) -> std::result::Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        serializer.serialize_str(&self.to_string())
    }
}

pub type Result<T> = std::result::Result<T, PostGateError>;
