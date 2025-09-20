use anyhow::Result;
use once_cell::sync::OnceCell;
use reqwest::{Client as ReqwestClient, StatusCode};
use shared_types::{ConfigData, ConfigKey};
use std::collections::HashMap;
use std::sync::Arc;
use std::time::Duration;
use tokio::sync::RwLock;

static INSTANCE: OnceCell<Arc<CachedConfigClient>> = OnceCell::new();

pub struct CachedConfigClient {
    client: ReqwestClient,
    base_url: String,
    cache: RwLock<HashMap<String, ConfigData>>,
}

impl CachedConfigClient {
    pub fn initialize(base_url: impl Into<String>) -> Result<()> {
        let client = ReqwestClient::builder()
            .timeout(Duration::from_secs(30))
            .build()?;

        let instance = Arc::new(Self {
            client,
            base_url: base_url.into().trim_end_matches('/').to_string(),
            cache: RwLock::new(HashMap::new()),
        });

        INSTANCE
            .set(instance)
            .map_err(|_| anyhow::anyhow!("CachedConfigClient already initialized"))?;
        Ok(())
    }

    pub fn instance() -> Result<Arc<Self>> {
        INSTANCE.get().cloned().ok_or_else(|| {
            anyhow::anyhow!("CachedConfigClient not initialized. Call initialize() first.")
        })
    }

    pub async fn get_config(&self, key: &ConfigKey) -> Result<ConfigData> {
        let cache_key = key.to_string();

        {
            let cache = self.cache.read().await;
            match cache.get(&cache_key) {
                Some(cached) => return Ok(cached.clone()),
                None => {}
            }
        }

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
        let config_data = ConfigData {
            content: data["content"].clone(),
            schema: data["schema"].clone(),
            version: data["version"].as_str().unwrap_or("").to_string(),
        };

        {
            let mut cache = self.cache.write().await;
            cache.insert(cache_key, config_data.clone());
        }

        Ok(config_data)
    }

    pub async fn clear_cache(&self) {
        self.cache.write().await.clear();
    }

    pub async fn health_check(&self) -> Result<bool> {
        let url = format!("{}/health", self.base_url);
        let response = self.client.get(&url).send().await?;
        Ok(response.status() == StatusCode::OK)
    }
}
