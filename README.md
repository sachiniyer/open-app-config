# Open App Config

Application Configuration Services are great, but I can't find any independent alternatives. The closest alternative, feature flag managers, often miss the immutable versioning of the configuration data and schema validation.

 This package most closely mirrors AWS AppConfig, The core feature set is:
- Schema validation on write (with [jsonschema](https://crates.io/crates/jsonschema))
- Versioning of the configuration data
- Storage backends for local file system, S3/MinIO (with [object store](https://crates.io/crates/object_store))
- A client with local caching.

### TODO:
- diff the schemas and configs instead of replicating them each time.
- other storage backends (postgres :eyes:)
- client libs in other languages (nodejs :eyes:)

# Alternatives (you should use these instead of this package)
1. AWS AppConfig
2. Azure Application Configuration
3. Kubernetes ConfigMap

If you do want to use this package, [contact me](https://sachiniyer.com/contact), and I am happy put more effort into maintaing it.
