use axum::{
    http::StatusCode,
    response::{IntoResponse, Response},
    Json,
};

use super::dto::ErrorResponse;
use crate::storage::StorageError;

#[derive(Debug)]
#[allow(dead_code)] // Will be used when handlers are implemented
pub enum ApiError {
    NotFound(String),
    BadRequest(String),
    VersionConflict { expected: String, actual: String },
    InternalError(String),
    StorageError(anyhow::Error),
}

impl IntoResponse for ApiError {
    fn into_response(self) -> Response {
        let (status, error, details) = match self {
            ApiError::NotFound(msg) => (StatusCode::NOT_FOUND, "Not Found", Some(msg)),
            ApiError::BadRequest(msg) => (StatusCode::BAD_REQUEST, "Bad Request", Some(msg)),
            ApiError::VersionConflict { expected, actual } => (
                StatusCode::CONFLICT,
                "Version Conflict",
                Some(format!(
                    "Expected version {}, but found {}",
                    expected, actual
                )),
            ),
            ApiError::InternalError(msg) => (
                StatusCode::INTERNAL_SERVER_ERROR,
                "Internal Server Error",
                Some(msg),
            ),
            ApiError::StorageError(err) => {
                // Check if it's a version conflict from storage
                if let Some(storage_err) = err.downcast_ref::<StorageError>() {
                    match storage_err {
                        StorageError::VersionConflict { expected, actual } => {
                            return ApiError::VersionConflict {
                                expected: expected.clone(),
                                actual: actual.clone(),
                            }
                            .into_response();
                        }
                        StorageError::NotFound(key) => {
                            return ApiError::NotFound(format!("Configuration not found: {}", key))
                                .into_response();
                        }
                        _ => {}
                    }
                }

                (
                    StatusCode::INTERNAL_SERVER_ERROR,
                    "Storage Error",
                    Some(err.to_string()),
                )
            }
        };

        let body = Json(ErrorResponse {
            error: error.to_string(),
            details,
        });

        (status, body).into_response()
    }
}

// Convenience conversion from anyhow::Error
impl From<anyhow::Error> for ApiError {
    fn from(err: anyhow::Error) -> Self {
        ApiError::StorageError(err)
    }
}

pub type ApiResult<T> = Result<T, ApiError>;

#[cfg(test)]
mod tests {
    use super::*;
    use axum::body::to_bytes;

    #[tokio::test]
    async fn test_api_error_not_found() {
        let error = ApiError::NotFound("Config not found".to_string());
        let response = error.into_response();

        assert_eq!(response.status(), StatusCode::NOT_FOUND);

        let body = to_bytes(response.into_body(), 1024).await.unwrap();
        let error_response: ErrorResponse = serde_json::from_slice(&body).unwrap();

        assert_eq!(error_response.error, "Not Found");
        assert_eq!(error_response.details, Some("Config not found".to_string()));
    }

    #[tokio::test]
    async fn test_api_error_bad_request() {
        let error = ApiError::BadRequest("Invalid input".to_string());
        let response = error.into_response();

        assert_eq!(response.status(), StatusCode::BAD_REQUEST);

        let body = to_bytes(response.into_body(), 1024).await.unwrap();
        let error_response: ErrorResponse = serde_json::from_slice(&body).unwrap();

        assert_eq!(error_response.error, "Bad Request");
        assert_eq!(error_response.details, Some("Invalid input".to_string()));
    }

    #[tokio::test]
    async fn test_api_error_version_conflict() {
        let error = ApiError::VersionConflict {
            expected: "v1".to_string(),
            actual: "v2".to_string(),
        };
        let response = error.into_response();

        assert_eq!(response.status(), StatusCode::CONFLICT);

        let body = to_bytes(response.into_body(), 1024).await.unwrap();
        let error_response: ErrorResponse = serde_json::from_slice(&body).unwrap();

        assert_eq!(error_response.error, "Version Conflict");
        assert!(error_response
            .details
            .unwrap()
            .contains("Expected version v1, but found v2"));
    }

    #[tokio::test]
    async fn test_api_error_internal() {
        let error = ApiError::InternalError("Database connection failed".to_string());
        let response = error.into_response();

        assert_eq!(response.status(), StatusCode::INTERNAL_SERVER_ERROR);

        let body = to_bytes(response.into_body(), 1024).await.unwrap();
        let error_response: ErrorResponse = serde_json::from_slice(&body).unwrap();

        assert_eq!(error_response.error, "Internal Server Error");
        assert_eq!(
            error_response.details,
            Some("Database connection failed".to_string())
        );
    }

    #[tokio::test]
    async fn test_storage_error_conversion() {
        let storage_err = StorageError::VersionConflict {
            expected: "v1".to_string(),
            actual: "v2".to_string(),
        };
        let error = ApiError::StorageError(anyhow::Error::new(storage_err));
        let response = error.into_response();

        assert_eq!(response.status(), StatusCode::CONFLICT);
    }

    #[test]
    fn test_from_anyhow_error() {
        let anyhow_err = anyhow::anyhow!("Some error");
        let api_err: ApiError = anyhow_err.into();

        match api_err {
            ApiError::StorageError(_) => {} // Expected
            _ => panic!("Expected StorageError variant"),
        }
    }
}
