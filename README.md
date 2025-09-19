# Open App Config

Application Configuration Services are great, but I can't find any independent alternatives. The closest alternative, feature flag managers, often miss the immutable versioning of the configuration data and schema validation.

The core feature set:
- Schema validation on write
- Versioning of the configuration data

This package most closely mirrors AWS AppConfig, but with flexible storage backends

# Alternative
1. AWS AppConfig
2. Azure Application Configuration
3. Kubernetes ConfigMap
4. etcd (kinda)
