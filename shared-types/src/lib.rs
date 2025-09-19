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

/// Configuration data with optional schema and version
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ConfigData {
    pub content: serde_json::Value,
    pub schema: Option<serde_json::Value>,
    pub version: String,
}

/// Version information for a configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VersionInfo {
    pub version: String,
    pub timestamp: chrono::DateTime<chrono::Utc>,
}
