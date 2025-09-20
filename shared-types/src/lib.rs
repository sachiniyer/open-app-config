use serde::{Deserialize, Serialize};
use std::fmt;

/// Structured key for identifying configurations
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Hash)]
pub struct ConfigKey {
    pub application: String,
    pub environment: String,
    pub config_name: String,
}

impl ConfigKey {
    pub fn new(
        application: impl Into<String>,
        environment: impl Into<String>,
        config_name: impl Into<String>,
    ) -> Self {
        Self {
            application: application.into(),
            environment: environment.into(),
            config_name: config_name.into(),
        }
    }

    /// Generate a path-like string representation
    pub fn to_path(&self) -> String {
        format!(
            "{}/{}/{}",
            self.application, self.environment, self.config_name
        )
    }
}

impl fmt::Display for ConfigKey {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.to_path())
    }
}

/// Configuration data with required schema and version
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ConfigData {
    pub content: serde_json::Value,
    pub schema: serde_json::Value, // Schema is now required
    pub version: String,
}

/// Version information for a configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VersionInfo {
    pub version: String,
    pub timestamp: chrono::DateTime<chrono::Utc>,
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn test_config_key_new() {
        let key = ConfigKey::new("app", "dev", "config");
        assert_eq!(key.application, "app");
        assert_eq!(key.environment, "dev");
        assert_eq!(key.config_name, "config");
    }

    #[test]
    fn test_config_key_to_path() {
        let key = ConfigKey::new("myapp", "production", "database");
        assert_eq!(key.to_path(), "myapp/production/database");
    }

    #[test]
    fn test_config_key_display() {
        let key = ConfigKey::new("app", "staging", "api");
        assert_eq!(format!("{}", key), "app/staging/api");
    }

    #[test]
    fn test_config_key_serialization() {
        let key = ConfigKey::new("app", "dev", "config");
        let json = serde_json::to_string(&key).unwrap();
        let deserialized: ConfigKey = serde_json::from_str(&json).unwrap();
        assert_eq!(key, deserialized);
    }

    #[test]
    fn test_config_data_serialization() {
        let data = ConfigData {
            content: json!({"host": "localhost", "port": 5432}),
            schema: json!({"type": "object"}),
            version: "v1".to_string(),
        };

        let json = serde_json::to_string(&data).unwrap();
        let deserialized: ConfigData = serde_json::from_str(&json).unwrap();

        assert_eq!(data.content, deserialized.content);
        assert_eq!(data.schema, deserialized.schema);
        assert_eq!(data.version, deserialized.version);
    }

    #[test]
    fn test_version_info_serialization() {
        let now = chrono::Utc::now();
        let version = VersionInfo {
            version: "v2".to_string(),
            timestamp: now,
        };

        let json = serde_json::to_string(&version).unwrap();
        let deserialized: VersionInfo = serde_json::from_str(&json).unwrap();

        assert_eq!(version.version, deserialized.version);
        assert_eq!(version.timestamp, deserialized.timestamp);
    }
}
