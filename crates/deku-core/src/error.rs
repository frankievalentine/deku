use thiserror::Error;

#[derive(Debug, Error)]
pub enum DekuError {
    #[error("app not found: {0}")]
    AppNotFound(String),

    #[error("app already exists: {0}")]
    AppAlreadyExists(String),

    #[error("app is locked: {0}")]
    AppLocked(String),

    #[error("deploy failed: {0}")]
    DeployFailed(String),

    #[error("build failed: {0}")]
    BuildFailed(String),

    #[error("database error: {0}")]
    Database(#[from] sqlx::Error),

    #[error("io error: {0}")]
    Io(#[from] std::io::Error),

    #[error("serialization error: {0}")]
    Serialization(#[from] serde_json::Error),

    #[error("plugin error: {0}")]
    Plugin(String),

    #[error("unauthorized")]
    Unauthorized,

    #[error("internal error: {0}")]
    Internal(String),
}

pub type Result<T> = std::result::Result<T, DekuError>;
