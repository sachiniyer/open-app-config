use thiserror::Error;

#[derive(Error, Debug)]
pub enum StorageError {
    #[error("Configuration not found: {0}")]
    NotFound(String),

    #[error("Configuration already exists: {0}")]
    AlreadyExists(String),

    #[error("IO error: {0}")]
    IoError(#[from] std::io::Error),

    #[error("Serialization error: {0}")]
    SerializationError(#[from] serde_json::Error),

    #[error("Validation error: {0}")]
    ValidationError(String),

    #[error("Version conflict: {0}")]
    VersionConflict(String),

    #[error("Storage error: {0}")]
    Other(String),
}

pub type Result<T> = anyhow::Result<T>;
