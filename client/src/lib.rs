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
    use mockito::{self, Matcher};

    #[tokio::test]
    async fn test_client_creation() {
        let client = ConfigClient::new("http://localhost:3000").unwrap();
        assert_eq!(client.base_url, "http://localhost:3000");

        // Test trailing slash removal
        let client = ConfigClient::new("http://localhost:3000/").unwrap();
        assert_eq!(client.base_url, "http://localhost:3000");
    }

    #[tokio::test]
    async fn test_health_check() {
        let mut server = mockito::Server::new_async().await;

        let _m = server
            .mock("GET", "/health")
            .with_status(200)
            .with_body(r#"{"status":"healthy"}"#)
            .create();

        let client = ConfigClient::new(server.url()).unwrap();
        let healthy = client.health_check().await.unwrap();
        assert!(healthy);
    }

    #[tokio::test]
    async fn test_get_config() {
        let mut server = mockito::Server::new_async().await;

        let response_body = r#"{
            "application": "myapp",
            "environment": "dev",
            "config_name": "database",
            "version": "v1",
            "content": {"host": "localhost", "port": 5432},
            "schema": {"type": "object"}
        }"#;

        let _m = server
            .mock("GET", "/configs/myapp/dev/database")
            .with_status(200)
            .with_body(response_body)
            .create();

        let client = ConfigClient::new(server.url()).unwrap();
        let key = ConfigKey::new("myapp", "dev", "database");
        let config = client.get_config(&key).await.unwrap();

        assert_eq!(config.version, "v1");
        assert_eq!(config.content["host"], "localhost");
        assert_eq!(config.content["port"], 5432);
    }

    #[tokio::test]
    async fn test_get_config_not_found() {
        let mut server = mockito::Server::new_async().await;

        let _m = server
            .mock("GET", "/configs/myapp/dev/missing")
            .with_status(404)
            .create();

        let client = ConfigClient::new(server.url()).unwrap();
        let key = ConfigKey::new("myapp", "dev", "missing");
        let result = client.get_config(&key).await;

        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("not found"));
    }

    #[tokio::test]
    async fn test_put_config() {
        let mut server = mockito::Server::new_async().await;

        let _m = server
            .mock("PUT", "/configs/myapp/dev/api")
            .match_header("content-type", "application/json")
            .match_body(Matcher::Json(serde_json::json!({
                "content": {"url": "https://api.example.com"},
                "schema": {"type": "object"},
                "expected_version": null
            })))
            .with_status(200)
            .with_body(r#"{"message": "Success", "version": "v1"}"#)
            .create();

        let client = ConfigClient::new(server.url()).unwrap();
        let key = ConfigKey::new("myapp", "dev", "api");
        let content = serde_json::json!({"url": "https://api.example.com"});
        let schema = serde_json::json!({"type": "object"});

        let version = client
            .put_config(&key, content, Some(schema), None)
            .await
            .unwrap();
        assert_eq!(version, "v1");
    }

    #[tokio::test]
    async fn test_delete_config() {
        let mut server = mockito::Server::new_async().await;

        let _m = server
            .mock("DELETE", "/configs/myapp/dev/temp")
            .with_status(200)
            .with_body(r#"{"message": "Deleted successfully"}"#)
            .create();

        let client = ConfigClient::new(server.url()).unwrap();
        let key = ConfigKey::new("myapp", "dev", "temp");

        client.delete_config(&key).await.unwrap();
    }

    #[tokio::test]
    async fn test_list_versions() {
        let mut server = mockito::Server::new_async().await;

        let response_body = r#"{
            "versions": [
                {"version": "v1", "timestamp": "2024-01-01T00:00:00Z"},
                {"version": "v2", "timestamp": "2024-01-02T00:00:00Z"}
            ]
        }"#;

        let _m = server
            .mock("GET", "/configs/myapp/dev/config/versions")
            .with_status(200)
            .with_body(response_body)
            .create();

        let client = ConfigClient::new(server.url()).unwrap();
        let key = ConfigKey::new("myapp", "dev", "config");
        let versions = client.list_versions(&key).await.unwrap();

        assert_eq!(versions.len(), 2);
        assert_eq!(versions[0].version, "v1");
        assert_eq!(versions[1].version, "v2");
    }

    #[tokio::test]
    async fn test_list_configs() {
        let mut server = mockito::Server::new_async().await;

        let response_body = r#"{
            "configs": [
                {"application": "app1", "environment": "dev", "config_name": "db"},
                {"application": "app2", "environment": "prod", "config_name": "api"}
            ]
        }"#;

        let _m = server
            .mock("GET", "/configs")
            .with_status(200)
            .with_body(response_body)
            .create();

        let client = ConfigClient::new(server.url()).unwrap();
        let configs = client.list_configs(None).await.unwrap();

        assert_eq!(configs.len(), 2);
        assert_eq!(configs[0].application, "app1");
        assert_eq!(configs[1].application, "app2");
    }

    #[tokio::test]
    async fn test_list_configs_with_prefix() {
        let mut server = mockito::Server::new_async().await;

        let _m = server
            .mock("GET", "/configs?prefix=app1")
            .with_status(200)
            .with_body(r#"{"configs": []}"#)
            .create();

        let client = ConfigClient::new(server.url()).unwrap();
        let configs = client.list_configs(Some("app1")).await.unwrap();

        assert_eq!(configs.len(), 0);
    }
}
