use serde::{Deserialize, Serialize};
use shared_types::{ConfigData, ConfigKey, VersionInfo};

/// Request body for creating or updating a configuration
#[derive(Debug, Serialize, Deserialize)]
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

#[derive(Debug, Clone, Serialize, Deserialize)]
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

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn test_put_config_request_serialization() {
        let request = PutConfigRequest {
            content: json!({"key": "value"}),
            schema: Some(json!({"type": "object"})),
            expected_version: Some("v1".to_string()),
        };

        let json = serde_json::to_string(&request).unwrap();
        let deserialized: PutConfigRequest = serde_json::from_str(&json).unwrap();

        assert_eq!(deserialized.content, request.content);
        assert_eq!(deserialized.schema, request.schema);
        assert_eq!(deserialized.expected_version, request.expected_version);
    }

    #[test]
    fn test_put_config_request_with_null_schema() {
        let json = r#"{
            "content": {"key": "value"},
            "schema": null,
            "expected_version": "v2"
        }"#;

        let request: PutConfigRequest = serde_json::from_str(json).unwrap();
        assert_eq!(request.content, json!({"key": "value"}));
        assert_eq!(request.schema, None);
        assert_eq!(request.expected_version, Some("v2".to_string()));
    }

    #[test]
    fn test_get_config_response_from_data_and_key() {
        let key = ConfigKey::new("app", "dev", "config");
        let data = ConfigData {
            content: json!({"setting": "value"}),
            schema: json!({"type": "object"}),
            version: "v1".to_string(),
        };

        let response = GetConfigResponse::from_data_and_key(data.clone(), &key);

        assert_eq!(response.application, "app");
        assert_eq!(response.environment, "dev");
        assert_eq!(response.config_name, "config");
        assert_eq!(response.version, "v1");
        assert_eq!(response.content, data.content);
        assert_eq!(response.schema, data.schema);
    }

    #[test]
    fn test_config_summary_serialization() {
        let summary = ConfigSummary {
            application: "myapp".to_string(),
            environment: "prod".to_string(),
            config_name: "database".to_string(),
            current_version: "v3".to_string(),
        };

        let json = serde_json::to_string(&summary).unwrap();
        let deserialized: ConfigSummary = serde_json::from_str(&json).unwrap();

        assert_eq!(deserialized.application, summary.application);
        assert_eq!(deserialized.environment, summary.environment);
        assert_eq!(deserialized.config_name, summary.config_name);
        assert_eq!(deserialized.current_version, summary.current_version);
    }

    #[test]
    fn test_list_configs_response() {
        let configs = vec![
            ConfigSummary {
                application: "app1".to_string(),
                environment: "dev".to_string(),
                config_name: "config1".to_string(),
                current_version: "v1".to_string(),
            },
            ConfigSummary {
                application: "app2".to_string(),
                environment: "prod".to_string(),
                config_name: "config2".to_string(),
                current_version: "v2".to_string(),
            },
        ];

        let response = ListConfigsResponse {
            configs: configs.clone(),
        };

        let json = serde_json::to_string(&response).unwrap();
        let deserialized: ListConfigsResponse = serde_json::from_str(&json).unwrap();

        assert_eq!(deserialized.configs.len(), 2);
        assert_eq!(deserialized.configs[0].application, "app1");
        assert_eq!(deserialized.configs[1].application, "app2");
    }

    #[test]
    fn test_success_response() {
        let response = SuccessResponse {
            message: "Operation successful".to_string(),
            version: Some("v5".to_string()),
        };

        let json = serde_json::to_string(&response).unwrap();
        let deserialized: SuccessResponse = serde_json::from_str(&json).unwrap();

        assert_eq!(deserialized.message, response.message);
        assert_eq!(deserialized.version, response.version);
    }

    #[test]
    fn test_error_response() {
        let response = ErrorResponse {
            error: "Not Found".to_string(),
            details: Some("Configuration not found".to_string()),
        };

        let json = serde_json::to_string(&response).unwrap();
        let deserialized: ErrorResponse = serde_json::from_str(&json).unwrap();

        assert_eq!(deserialized.error, response.error);
        assert_eq!(deserialized.details, response.details);
    }
}
