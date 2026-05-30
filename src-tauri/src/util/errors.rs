use thiserror::Error;

#[derive(Debug, Error)]
pub enum AppError {
    #[error("invalid URL: {0}")]
    InvalidUrl(String),

    #[error("network error: {0}")]
    Network(String),

    #[error("decode error: {0}")]
    Decode(String),

    #[error("not implemented: {0}")]
    NotImplemented(&'static str),
}

impl From<reqwest::Error> for AppError {
    fn from(e: reqwest::Error) -> Self {
        AppError::Network(e.to_string())
    }
}

impl From<url::ParseError> for AppError {
    fn from(e: url::ParseError) -> Self {
        AppError::InvalidUrl(e.to_string())
    }
}

/// Serializable form returned across the Tauri IPC boundary.
#[derive(Debug, serde::Serialize)]
pub struct IpcError {
    pub code: &'static str,
    pub message: String,
}

impl From<AppError> for IpcError {
    fn from(e: AppError) -> Self {
        let code = match e {
            AppError::InvalidUrl(_) => "invalid_url",
            AppError::Network(_) => "network",
            AppError::Decode(_) => "decode",
            AppError::NotImplemented(_) => "not_implemented",
        };
        IpcError { code, message: e.to_string() }
    }
}

pub type AppResult<T> = Result<T, AppError>;
