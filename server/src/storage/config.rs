use serde::{Deserialize, Serialize};
use std::path::PathBuf;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum StorageConfig {
    Local {
        path: PathBuf,
    },
    S3 {
        bucket: String,
        region: Option<String>,
        endpoint: Option<String>,
        access_key_id: Option<String>,
        secret_access_key: Option<String>,
        allow_http: bool,
    },
}

impl StorageConfig {
    pub fn local(path: impl Into<PathBuf>) -> Self {
        Self::Local { path: path.into() }
    }

    pub fn s3(
        bucket: impl Into<String>,
        region: Option<String>,
        endpoint: Option<String>,
        access_key_id: Option<String>,
        secret_access_key: Option<String>,
        allow_http: bool,
    ) -> Self {
        Self::S3 {
            bucket: bucket.into(),
            region,
            endpoint,
            access_key_id,
            secret_access_key,
            allow_http,
        }
    }

    pub fn from_env() -> anyhow::Result<Self> {
        let backend = std::env::var("STORAGE_BACKEND").unwrap_or_else(|_| "local".to_string());

        match backend.as_str() {
            "local" => {
                let path = std::env::var("STORAGE_PATH").unwrap_or_else(|_| "./data".to_string());
                Ok(Self::local(path))
            }
            "s3" => {
                let bucket = std::env::var("AWS_BUCKET")
                    .map_err(|_| anyhow::anyhow!("AWS_BUCKET is required for S3 backend"))?;
                let region = std::env::var("AWS_REGION").ok();
                let endpoint = std::env::var("AWS_ENDPOINT").ok();
                let access_key_id = std::env::var("AWS_ACCESS_KEY_ID").ok();
                let secret_access_key = std::env::var("AWS_SECRET_ACCESS_KEY").ok();
                let allow_http = std::env::var("AWS_ALLOW_HTTP")
                    .unwrap_or_else(|_| "false".to_string())
                    .parse::<bool>()
                    .unwrap_or(false);

                Ok(Self::s3(
                    bucket,
                    region,
                    endpoint,
                    access_key_id,
                    secret_access_key,
                    allow_http,
                ))
            }
            _ => anyhow::bail!(
                "Unknown storage backend: {}. Must be 'local' or 's3'",
                backend
            ),
        }
    }
}
