use serde::{Deserialize, Serialize};
use std::path::PathBuf;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum StorageConfig {
    Local { path: PathBuf },
    // Future: S3, GCS, Azure backends
}

impl StorageConfig {
    pub fn local(path: impl Into<PathBuf>) -> Self {
        Self::Local { path: path.into() }
    }
}
