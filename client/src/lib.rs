use anyhow::Result;
use reqwest::{Client as ReqwestClient, StatusCode};
use shared_types::{ConfigData, ConfigKey, VersionInfo};
use std::time::Duration;

/// Client for interacting with the Open App Config service
pub struct ConfigClient {
    client: ReqwestClient,
    base_url: String,
}

impl ConfigClient {
    /// Create a new client instance
    pub fn new(base_url: impl Into<String>) -> Result<Self> {
        let client = ReqwestClient::builder()
            .timeout(Duration::from_secs(30))
            .build()?;

        Ok(Self {
            client,
            base_url: base_url.into().trim_end_matches('/').to_string(),
        })
    }

    /// Get the current version of a configuration
    pub async fn get_config(&self, key: &ConfigKey) -> Result<ConfigData> {
        let url = format!(
            "{}/configs/{}/{}/{}",
            self.base_url, key.application, key.environment, key.config_name
        );

        let response = self.client.get(&url).send().await?;

        if response.status() == StatusCode::NOT_FOUND {
            anyhow::bail!("Configuration not found: {}", key);
        }

        response.error_for_status_ref()?;

        let data: serde_json::Value = response.json().await?;

        Ok(ConfigData {
            content: data["content"].clone(),
            schema: data["schema"].clone(),
            version: data["version"].as_str().unwrap_or("").to_string(),
        })
    }

    /// Create or update a configuration
    pub async fn put_config(
        &self,
        key: &ConfigKey,
        content: serde_json::Value,
        schema: Option<serde_json::Value>,
        expected_version: Option<String>,
    ) -> Result<String> {
        let url = format!(
            "{}/configs/{}/{}/{}",
            self.base_url, key.application, key.environment, key.config_name
        );

        let body = serde_json::json!({
            "content": content,
            "schema": schema,
            "expected_version": expected_version,
        });

        let response = self.client.put(&url).json(&body).send().await?;

        response.error_for_status_ref()?;

        let result: serde_json::Value = response.json().await?;

        Ok(result["version"].as_str().unwrap_or("unknown").to_string())
    }

    /// Delete a configuration
    pub async fn delete_config(&self, key: &ConfigKey) -> Result<()> {
        let url = format!(
            "{}/configs/{}/{}/{}",
            self.base_url, key.application, key.environment, key.config_name
        );

        let response = self.client.delete(&url).send().await?;

        response.error_for_status()?;

        Ok(())
    }

    /// List all versions of a configuration
    pub async fn list_versions(&self, key: &ConfigKey) -> Result<Vec<VersionInfo>> {
        let url = format!(
            "{}/configs/{}/{}/{}/versions",
            self.base_url, key.application, key.environment, key.config_name
        );

        let response = self.client.get(&url).send().await?;

        if response.status() == StatusCode::NOT_FOUND {
            anyhow::bail!("Configuration not found: {}", key);
        }

        response.error_for_status_ref()?;

        let data: serde_json::Value = response.json().await?;
        let versions: Vec<VersionInfo> = serde_json::from_value(data["versions"].clone())?;

        Ok(versions)
    }

    /// Get a specific version of a configuration
    pub async fn get_config_version(&self, key: &ConfigKey, version: &str) -> Result<ConfigData> {
        let url = format!(
            "{}/configs/{}/{}/{}/versions/{}",
            self.base_url, key.application, key.environment, key.config_name, version
        );

        let response = self.client.get(&url).send().await?;

        if response.status() == StatusCode::NOT_FOUND {
            anyhow::bail!("Configuration version not found: {} @ {}", key, version);
        }

        response.error_for_status_ref()?;

        let data: serde_json::Value = response.json().await?;

        Ok(ConfigData {
            content: data["content"].clone(),
            schema: data["schema"].clone(),
            version: data["version"].as_str().unwrap_or("").to_string(),
        })
    }

    /// List all configurations with optional prefix filter
    pub async fn list_configs(&self, prefix: Option<&str>) -> Result<Vec<ConfigKey>> {
        let mut url = format!("{}/configs", self.base_url);

        if let Some(p) = prefix {
            url.push_str(&format!("?prefix={}", p));
        }

        let response = self.client.get(&url).send().await?;

        response.error_for_status_ref()?;

        let data: serde_json::Value = response.json().await?;
        let configs = data["configs"]
            .as_array()
            .unwrap_or(&vec![])
            .iter()
            .map(|c| {
                ConfigKey::new(
                    c["application"].as_str().unwrap_or(""),
                    c["environment"].as_str().unwrap_or(""),
                    c["config_name"].as_str().unwrap_or(""),
                )
            })
            .collect();

        Ok(configs)
    }

    /// Check if the service is healthy
    pub async fn health_check(&self) -> Result<bool> {
        let url = format!("{}/health", self.base_url);

        let response = self.client.get(&url).send().await?;

        Ok(response.status() == StatusCode::OK)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_client_creation() {
        let client = ConfigClient::new("http://localhost:3000").unwrap();
        assert_eq!(client.base_url, "http://localhost:3000");

        // Test trailing slash removal
        let client = ConfigClient::new("http://localhost:3000/").unwrap();
        assert_eq!(client.base_url, "http://localhost:3000");
    }

    #[test]
    fn test_client_url_formatting() {
        let client = ConfigClient::new("http://localhost:3000").unwrap();
        assert_eq!(client.base_url, "http://localhost:3000");

        // Test with multiple trailing slashes
        let client = ConfigClient::new("http://localhost:3000///").unwrap();
        assert_eq!(client.base_url, "http://localhost:3000");
    }

    #[test]
    fn test_client_with_different_schemes() {
        let client = ConfigClient::new("https://api.example.com").unwrap();
        assert_eq!(client.base_url, "https://api.example.com");

        let client = ConfigClient::new("http://internal-service:8080").unwrap();
        assert_eq!(client.base_url, "http://internal-service:8080");
    }

    #[test]
    fn test_client_with_paths() {
        let client = ConfigClient::new("https://api.example.com/v1/config/").unwrap();
        assert_eq!(client.base_url, "https://api.example.com/v1/config");
    }
}
