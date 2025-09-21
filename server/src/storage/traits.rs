use anyhow::Result;
use async_trait::async_trait;
use shared_types::{ConfigData, ConfigKey, VersionInfo};

#[async_trait]
pub trait ConfigStorage: Send + Sync {
    async fn get(&self, key: &ConfigKey) -> Result<ConfigData>;
    async fn put(
        &self,
        key: &ConfigKey,
        data: &ConfigData,
        expected_version: Option<&str>,
    ) -> Result<()>;
    async fn delete_environment(&self, app: &str, env: &str) -> Result<usize>;
    async fn exists(&self, key: &ConfigKey) -> Result<bool>;
    async fn get_version(&self, key: &ConfigKey, version: &str) -> Result<ConfigData>;
    async fn list_versions(&self, key: &ConfigKey) -> Result<Vec<VersionInfo>>;
}
