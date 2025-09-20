use serde::{Deserialize, Serialize};
use shared_types::{ConfigData, ConfigKey, VersionInfo};

/// Request body for creating or updating a configuration
#[derive(Debug, Serialize, Deserialize)]
#[allow(dead_code)] // Will be used when handlers are implemented
pub struct PutConfigRequest {
    /// The configuration content (JSON)
    pub content: serde_json::Value,

    /// Optional JSON schema for validation
    /// If not provided, uses the schema from the previous version
    pub schema: Option<serde_json::Value>,

    /// Expected version for optimistic concurrency control
    /// - None for first creation
    /// - Some("v1") when updating from v1
    pub expected_version: Option<String>,
}

/// Response for a successful configuration retrieval
#[derive(Debug, Serialize, Deserialize)]
pub struct GetConfigResponse {
    pub application: String,
    pub environment: String,
    pub config_name: String,
    pub version: String,
    pub content: serde_json::Value,
    pub schema: serde_json::Value,
}

/// Response for listing configurations
#[derive(Debug, Serialize, Deserialize)]
pub struct ListConfigsResponse {
    pub configs: Vec<ConfigSummary>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct ConfigSummary {
    pub application: String,
    pub environment: String,
    pub config_name: String,
    pub current_version: String,
}

/// Response for listing versions
#[derive(Debug, Serialize, Deserialize)]
pub struct ListVersionsResponse {
    pub versions: Vec<VersionInfo>,
}

/// Response for successful operations that don't return data
#[derive(Debug, Serialize, Deserialize)]
pub struct SuccessResponse {
    pub message: String,
    pub version: Option<String>,
}

/// Error response
#[derive(Debug, Serialize, Deserialize)]
pub struct ErrorResponse {
    pub error: String,
    pub details: Option<String>,
}

// Conversion helpers
impl GetConfigResponse {
    #[allow(dead_code)] // Will be used when handlers are implemented
    pub fn from_data_and_key(data: ConfigData, key: &ConfigKey) -> Self {
        Self {
            application: key.application.clone(),
            environment: key.environment.clone(),
            config_name: key.config_name.clone(),
            version: data.version,
            content: data.content,
            schema: data.schema,
        }
    }
}
