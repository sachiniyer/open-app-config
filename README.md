# Open App Config

Application Configuration Services are great, but I can't find any independent alternatives. The closest alternative, feature flag managers, often miss the immutable versioning of the configuration data and schema validation.

This package most closely mirrors AWS AppConfig, but with flexible storage backends.

The core feature set:
- Schema validation on write
- Versioning of the configuration data
- Storage backends for local file system, S3/MinIO
- A client with local caching.


### TODO:
- diff the schemas and configs instead of replicating them each time.
- other storage backends (postgres :eyes:)
-

# Alternatives (you should use these instead of this package)
1. AWS AppConfig
2. Azure Application Configuration
3. Kubernetes ConfigMap
4. etcd (kinda)
