use anyhow::Result;
use reqwest::{Client as ReqwestClient, StatusCode};
use shared_types::{ConfigData, ConfigKey, VersionInfo};
use std::collections::HashMap;
use std::sync::Arc;
use std::time::Duration;
use tokio::sync::RwLock;

pub struct ConfigClient {
    client: ReqwestClient,
    base_url: String,
    cache: Arc<RwLock<HashMap<String, ConfigData>>>,
}

impl ConfigClient {
    pub fn new(base_url: impl Into<String>) -> Result<Self> {
        let client = ReqwestClient::builder()
            .timeout(Duration::from_secs(30))
            .build()?;

        Ok(Self {
            client,
            base_url: base_url.into().trim_end_matches('/').to_string(),
            cache: Arc::new(RwLock::new(HashMap::new())),
        })
    }

    pub async fn get_config(&self, key: &ConfigKey) -> Result<ConfigData> {
        let cache_key = key.to_string();

        // Check cache first
        {
            let cache = self.cache.read().await;
            let cached = cache.get(&cache_key);
            if let Some(cached) = cached {
                return Ok(cached.clone());
            }
        }

        // Fetch from remote and cache
        let data = self.fetch_config(key).await?;

        {
            let mut cache = self.cache.write().await;
            cache.insert(cache_key, data.clone());
        }

        Ok(data)
    }

    pub async fn refresh(&self, key: &ConfigKey) -> Result<ConfigData> {
        let cache_key = key.to_string();
        let data = self.fetch_config(key).await?;

        {
            let mut cache = self.cache.write().await;
            cache.insert(cache_key, data.clone());
        }

        Ok(data)
    }

    async fn fetch_config(&self, key: &ConfigKey) -> Result<ConfigData> {
        let url = format!(
            "{}/configs/{}/{}/{}",
            self.base_url, key.application, key.environment, key.config_name
        );

        let response = self.client.get(&url).send().await?;

        if response.status() == StatusCode::NOT_FOUND {
            anyhow::bail!("Configuration not found: {key}");
        }

        response.error_for_status_ref()?;

        let data: serde_json::Value = response.json().await?;

        Ok(ConfigData {
            content: data["content"].clone(),
            schema: data["schema"].clone(),
            version: data["version"].as_str().unwrap_or("").to_string(),
        })
    }

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

        // Invalidate cache for this key
        {
            let mut cache = self.cache.write().await;
            cache.remove(&key.to_string());
        }

        Ok(result["version"].as_str().unwrap_or("unknown").to_string())
    }

    pub async fn delete_environment(&self, app: &str, env: &str) -> Result<()> {
        let url = format!("{}/configs/{}/{}", self.base_url, app, env);

        let response = self.client.delete(&url).send().await?;
        response.error_for_status()?;

        // Clear entire cache since we don't know which configs were deleted
        {
            let mut cache = self.cache.write().await;
            cache.clear();
        }

        Ok(())
    }

    pub async fn list_versions(&self, key: &ConfigKey) -> Result<Vec<VersionInfo>> {
        let url = format!(
            "{}/configs/{}/{}/{}/versions",
            self.base_url, key.application, key.environment, key.config_name
        );

        let response = self.client.get(&url).send().await?;

        if response.status() == StatusCode::NOT_FOUND {
            anyhow::bail!("Configuration not found: {key}");
        }

        response.error_for_status_ref()?;

        let data: serde_json::Value = response.json().await?;
        let versions: Vec<VersionInfo> = serde_json::from_value(data["versions"].clone())?;

        Ok(versions)
    }

    pub async fn get_config_version(&self, key: &ConfigKey, version: &str) -> Result<ConfigData> {
        let url = format!(
            "{}/configs/{}/{}/{}/versions/{}",
            self.base_url, key.application, key.environment, key.config_name, version
        );

        let response = self.client.get(&url).send().await?;

        if response.status() == StatusCode::NOT_FOUND {
            anyhow::bail!("Configuration version not found: {key} @ {version}");
        }

        response.error_for_status_ref()?;

        let data: serde_json::Value = response.json().await?;

        Ok(ConfigData {
            content: data["content"].clone(),
            schema: data["schema"].clone(),
            version: data["version"].as_str().unwrap_or("").to_string(),
        })
    }

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
    fn test_client_creation() -> Result<()> {
        let client = ConfigClient::new("http://localhost:3000")?;
        assert_eq!(client.base_url, "http://localhost:3000");

        let client = ConfigClient::new("http://localhost:3000/")?;
        assert_eq!(client.base_url, "http://localhost:3000");
        Ok(())
    }

    #[test]
    fn test_client_url_formatting() -> Result<()> {
        let client = ConfigClient::new("http://localhost:3000")?;
        assert_eq!(client.base_url, "http://localhost:3000");

        let client = ConfigClient::new("http://localhost:3000///")?;
        assert_eq!(client.base_url, "http://localhost:3000");
        Ok(())
    }
}
