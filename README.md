# Open App Config

AWS AppConfig (and Azure App Configuration) is great, but I can't find any alternatives. The closest alternative, feature flag managers, often miss the immutable versioning of the configuration data.

The core feature set:
- Schema validation on write
- Versioning of the configuration data
- Storage backend into s3
