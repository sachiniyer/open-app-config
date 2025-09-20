use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Metadata {
    pub current_version: String,
    pub versions: Vec<VersionMetadata>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VersionMetadata {
    pub version: String,
    pub timestamp: DateTime<Utc>,
}

impl Default for Metadata {
    fn default() -> Self {
        Self {
            current_version: String::new(),
            versions: Vec::new(),
        }
    }
}

impl Metadata {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn add_version(&mut self, version: String) {
        let version_meta = VersionMetadata {
            version: version.clone(),
            timestamp: Utc::now(),
        };
        self.versions.push(version_meta);
        self.current_version = version;
    }

    pub fn next_version_number(&self) -> u32 {
        self.versions
            .iter()
            .filter_map(|v| {
                v.version
                    .strip_prefix('v')
                    .and_then(|n| n.parse::<u32>().ok())
            })
            .max()
            .unwrap_or(0)
            + 1
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_metadata_new() {
        let metadata = Metadata::new();
        assert_eq!(metadata.current_version, "");
        assert!(metadata.versions.is_empty());
    }

    #[test]
    fn test_add_version() {
        let mut metadata = Metadata::new();
        metadata.add_version("v1".to_string());

        assert_eq!(metadata.current_version, "v1");
        assert_eq!(metadata.versions.len(), 1);
        assert_eq!(metadata.versions[0].version, "v1");
    }

    #[test]
    fn test_add_multiple_versions() {
        let mut metadata = Metadata::new();
        metadata.add_version("v1".to_string());
        metadata.add_version("v2".to_string());
        metadata.add_version("v3".to_string());

        assert_eq!(metadata.current_version, "v3");
        assert_eq!(metadata.versions.len(), 3);
        assert_eq!(metadata.versions[0].version, "v1");
        assert_eq!(metadata.versions[1].version, "v2");
        assert_eq!(metadata.versions[2].version, "v3");
    }

    #[test]
    fn test_next_version_number_empty() {
        let metadata = Metadata::new();
        assert_eq!(metadata.next_version_number(), 1);
    }

    #[test]
    fn test_next_version_number_with_versions() {
        let mut metadata = Metadata::new();
        metadata.add_version("v1".to_string());
        assert_eq!(metadata.next_version_number(), 2);

        metadata.add_version("v2".to_string());
        assert_eq!(metadata.next_version_number(), 3);

        metadata.add_version("v5".to_string());
        assert_eq!(metadata.next_version_number(), 6);
    }

    #[test]
    fn test_next_version_number_with_non_sequential() {
        let mut metadata = Metadata::new();
        metadata.versions.push(VersionMetadata {
            version: "v1".to_string(),
            timestamp: Utc::now(),
        });
        metadata.versions.push(VersionMetadata {
            version: "v10".to_string(),
            timestamp: Utc::now(),
        });
        metadata.versions.push(VersionMetadata {
            version: "v5".to_string(),
            timestamp: Utc::now(),
        });

        assert_eq!(metadata.next_version_number(), 11);
    }

    #[test]
    fn test_next_version_number_with_invalid_versions() {
        let mut metadata = Metadata::new();
        metadata.versions.push(VersionMetadata {
            version: "invalid".to_string(),
            timestamp: Utc::now(),
        });
        metadata.versions.push(VersionMetadata {
            version: "v2".to_string(),
            timestamp: Utc::now(),
        });
        metadata.versions.push(VersionMetadata {
            version: "vNaN".to_string(),
            timestamp: Utc::now(),
        });

        assert_eq!(metadata.next_version_number(), 3);
    }

    #[test]
    fn test_version_metadata_timestamp() {
        let before = Utc::now();
        let mut metadata = Metadata::new();
        metadata.add_version("v1".to_string());
        let after = Utc::now();

        assert!(metadata.versions[0].timestamp >= before);
        assert!(metadata.versions[0].timestamp <= after);
    }
}
