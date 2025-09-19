pub mod backend;
pub mod config;
pub mod error;
pub mod metadata;

#[cfg(test)]
mod tests;

use anyhow::Result;
use async_trait::async_trait;
use shared_types::{ConfigData, ConfigKey, VersionInfo};

pub use backend::ObjectStoreBackend;
pub use config::StorageConfig;
pub use error::StorageError;

#[async_trait]
pub trait ConfigStorage: Send + Sync {
    async fn get(&self, key: &ConfigKey) -> Result<ConfigData>;

    async fn put(&self, key: &ConfigKey, data: &ConfigData) -> Result<()>;

    async fn delete(&self, key: &ConfigKey) -> Result<()>;

    async fn exists(&self, key: &ConfigKey) -> Result<bool>;

    async fn list(&self, prefix: Option<&str>) -> Result<Vec<ConfigKey>>;

    async fn get_version(&self, key: &ConfigKey, version: &str) -> Result<ConfigData>;

    async fn list_versions(&self, key: &ConfigKey) -> Result<Vec<VersionInfo>>;
}

#[async_trait]
pub trait AtomicStorage: ConfigStorage {
    async fn compare_and_swap(
        &self,
        key: &ConfigKey,
        expected_version: Option<&str>,
        data: &ConfigData,
    ) -> Result<bool>;
}
