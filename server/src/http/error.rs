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
