# Open App Config

Application Configuration Services are great, but I can't find any independent alternatives. The closest alternative, feature flag managers, often miss the immutable versioning of the configuration data and schema validation.

The core feature set:
- Schema validation on write
- Versioning of the configuration data
- Storage backends for local file system, S3/MinIO
- A client with local caching.

This package most closely mirrors AWS AppConfig, but with flexible storage backends.


TODO:
- diff the schemas and configs instead of replicating them each time.

# Alternatives
1. AWS AppConfig
2. Azure Application Configuration
3. Kubernetes ConfigMap
4. etcd (kinda)
