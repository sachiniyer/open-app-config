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

    /// Store configuration with optimistic concurrency control.
    ///
    /// # Arguments
    /// * `key` - The configuration key
    /// * `data` - The configuration data to store
    /// * `expected_version` - The version that must currently exist:
    ///   - `None`: Config must not exist (used for first creation)
    ///   - `Some("v1")`: Current version must be "v1" (used for updates)
    ///
    /// # Errors
    /// Returns `StorageError::VersionConflict` if the expected version doesn't match
    async fn put(
        &self,
        key: &ConfigKey,
        data: &ConfigData,
        expected_version: Option<&str>,
    ) -> Result<()>;

    async fn delete(&self, key: &ConfigKey) -> Result<()>;

    async fn exists(&self, key: &ConfigKey) -> Result<bool>;

    async fn list(&self, prefix: Option<&str>) -> Result<Vec<ConfigKey>>;

    async fn get_version(&self, key: &ConfigKey, version: &str) -> Result<ConfigData>;

    async fn list_versions(&self, key: &ConfigKey) -> Result<Vec<VersionInfo>>;
}
