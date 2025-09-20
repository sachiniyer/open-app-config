use anyhow::Result;
use once_cell::sync::OnceCell;
use reqwest::{Client as ReqwestClient, StatusCode};
use shared_types::{ConfigData, ConfigKey};
use std::collections::HashMap;
use std::sync::Arc;
use std::time::Duration;
use tokio::sync::RwLock;
use tracing::{debug, info};

/// A cached configuration entry
#[derive(Clone, Debug)]
struct CachedConfig {
    data: ConfigData,
}

/// Global singleton instance of the cached client
static INSTANCE: OnceCell<Arc<CachedConfigClient>> = OnceCell::new();

/// A caching singleton client for the Open App Config service
///
/// This client caches configurations on first fetch and returns the cached
/// version for all subsequent requests. It acts as a singleton to ensure
/// consistent configuration across the application.
pub struct CachedConfigClient {
    client: ReqwestClient,
    base_url: String,
    cache: RwLock<HashMap<ConfigKey, CachedConfig>>,
}

impl CachedConfigClient {
    /// Initialize the global singleton instance
    ///
    /// This must be called once at application startup. Subsequent calls
    /// will return an error if already initialized.
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

        info!(
            "CachedConfigClient initialized with base URL: {}",
            INSTANCE.get().unwrap().base_url
        );

        Ok(())
    }

    /// Get the singleton instance
    ///
    /// Returns an error if the client has not been initialized.
    pub fn instance() -> Result<Arc<Self>> {
        INSTANCE.get().cloned().ok_or_else(|| {
            anyhow::anyhow!("CachedConfigClient not initialized. Call initialize() first.")
        })
    }

    /// Get a configuration, using cache if available
    ///
    /// On first call for a given key, fetches from the server and caches.
    /// Subsequent calls return the cached version.
    pub async fn get_config(&self, key: &ConfigKey) -> Result<ConfigData> {
        // Check cache first
        {
            let cache = self.cache.read().await;
            match cache.get(key) {
                Some(cached) => {
                    debug!("Returning cached config for {}", key);
                    return Ok(cached.data.clone());
                }
                None => {}
            }
        }

        // Not in cache, fetch from server
        info!("Fetching config from server for {}", key);
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

        // Cache the result
        {
            let mut cache = self.cache.write().await;
            cache.insert(
                key.clone(),
                CachedConfig {
                    data: config_data.clone(),
                },
            );
            info!(
                "Cached config for {} (version: {})",
                key, config_data.version
            );
        }

        Ok(config_data)
    }

    /// Get a specific version of a configuration, using cache if available
    ///
    /// Versions are cached separately with the version as part of the cache key.
    pub async fn get_config_version(&self, key: &ConfigKey, version: &str) -> Result<ConfigData> {
        // Create a versioned key for caching
        let versioned_key = ConfigKey::new(
            format!("{}@{}", key.application, version),
            key.environment.clone(),
            key.config_name.clone(),
        );

        // Check cache first
        {
            let cache = self.cache.read().await;
            match cache.get(&versioned_key) {
                Some(cached) => {
                    debug!("Returning cached config for {} @ {}", key, version);
                    return Ok(cached.data.clone());
                }
                None => {}
            }
        }

        // Not in cache, fetch from server
        info!(
            "Fetching config version from server for {} @ {}",
            key, version
        );
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

        let config_data = ConfigData {
            content: data["content"].clone(),
            schema: data["schema"].clone(),
            version: data["version"].as_str().unwrap_or("").to_string(),
        };

        // Cache the result
        {
            let mut cache = self.cache.write().await;
            cache.insert(
                versioned_key,
                CachedConfig {
                    data: config_data.clone(),
                },
            );
            info!("Cached config version for {} @ {}", key, version);
        }

        Ok(config_data)
    }

    /// Clear the cache
    ///
    /// This can be useful for testing or when you need to force a refresh.
    pub async fn clear_cache(&self) {
        let mut cache = self.cache.write().await;
        let count = cache.len();
        cache.clear();
        info!("Cleared {} cached configurations", count);
    }

    /// Get the number of cached configurations
    pub async fn cache_size(&self) -> usize {
        self.cache.read().await.len()
    }

    /// Check if a specific configuration is cached
    pub async fn is_cached(&self, key: &ConfigKey) -> bool {
        self.cache.read().await.contains_key(key)
    }

    /// Get all cached keys
    pub async fn cached_keys(&self) -> Vec<ConfigKey> {
        self.cache.read().await.keys().cloned().collect()
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

    #[tokio::test]
    async fn test_singleton_initialization() {
        // Note: This test may fail if run after other tests that initialize the singleton
        // Try to initialize - it might fail if already initialized by another test
        let init_result = CachedConfigClient::initialize("http://localhost:3000");

        if init_result.is_ok() {
            // If we successfully initialized, second attempt should fail
            assert!(CachedConfigClient::initialize("http://localhost:3000").is_err());
        }

        // Getting instance should work either way
        assert!(CachedConfigClient::instance().is_ok());
    }

    #[tokio::test]
    async fn test_cache_operations() {
        // Initialize if not already done
        let _ = CachedConfigClient::initialize("http://localhost:3000");
        let client = CachedConfigClient::instance().unwrap();

        // Clear cache to start fresh
        client.clear_cache().await;

        // Cache should be empty after clearing
        assert_eq!(client.cache_size().await, 0);

        let key = ConfigKey::new("app", "dev", "config");
        assert!(!client.is_cached(&key).await);

        // After clearing again, cache should still be empty
        client.clear_cache().await;
        assert_eq!(client.cache_size().await, 0);
    }

    #[tokio::test]
    async fn test_cached_keys() {
        // Initialize if not already done
        let _ = CachedConfigClient::initialize("http://localhost:3000");
        let client = CachedConfigClient::instance().unwrap();

        // Clear cache to start fresh
        client.clear_cache().await;

        let keys = client.cached_keys().await;
        assert!(keys.is_empty());
    }
}
