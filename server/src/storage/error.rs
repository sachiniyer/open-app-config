use thiserror::Error;

#[derive(Error, Debug)]
pub enum StorageError {
    #[error("Configuration not found: {0}")]
    NotFound(String),

    #[error("Configuration already exists: {0}")]
    AlreadyExists(String),

    #[error("Version conflict: expected {expected}, but found {actual}")]
    VersionConflict { expected: String, actual: String },
}
