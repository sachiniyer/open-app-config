use thiserror::Error;

#[derive(Error, Debug)]
#[allow(dead_code)]
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

    #[error("Version conflict: expected {expected}, but found {actual}")]
    VersionConflict { expected: String, actual: String },

    #[error("Storage error: {0}")]
    Other(String),
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_error_display() {
        let err = StorageError::NotFound("test-key".to_string());
        assert_eq!(err.to_string(), "Configuration not found: test-key");

        let err = StorageError::AlreadyExists("config".to_string());
        assert_eq!(err.to_string(), "Configuration already exists: config");

        let err = StorageError::ValidationError("Invalid schema".to_string());
        assert_eq!(err.to_string(), "Validation error: Invalid schema");

        let err = StorageError::VersionConflict {
            expected: "v1".to_string(),
            actual: "v2".to_string(),
        };
        assert_eq!(
            err.to_string(),
            "Version conflict: expected v1, but found v2"
        );

        let err = StorageError::Other("Custom error".to_string());
        assert_eq!(err.to_string(), "Storage error: Custom error");
    }

    #[test]
    fn test_error_from_io() {
        let io_err = std::io::Error::new(std::io::ErrorKind::NotFound, "file not found");
        let storage_err: StorageError = io_err.into();
        assert!(storage_err.to_string().contains("IO error"));
    }

    #[test]
    fn test_error_from_serde() {
        let json = "{ invalid json }";
        let serde_err = serde_json::from_str::<serde_json::Value>(json).unwrap_err();
        let storage_err: StorageError = serde_err.into();
        assert!(storage_err.to_string().contains("Serialization error"));
    }
}
