use super::dto::ErrorResponse;
use crate::storage::StorageError;
use axum::{
    Json,
    http::StatusCode,
    response::{IntoResponse, Response},
};

#[derive(Debug)]
pub enum ApiError {
    NotFound(String),
    BadRequest(String),
    InternalError(String),
}

impl IntoResponse for ApiError {
    fn into_response(self) -> Response {
        let (status, error, details) = match self {
            ApiError::NotFound(msg) => (StatusCode::NOT_FOUND, "Not Found", msg),
            ApiError::BadRequest(msg) => (StatusCode::BAD_REQUEST, "Bad Request", msg),
            ApiError::InternalError(msg) => (
                StatusCode::INTERNAL_SERVER_ERROR,
                "Internal Server Error",
                msg,
            ),
        };

        (
            status,
            Json(ErrorResponse {
                error: error.to_string(),
                details: Some(details),
            }),
        )
            .into_response()
    }
}

impl From<anyhow::Error> for ApiError {
    fn from(err: anyhow::Error) -> Self {
        match err.downcast_ref::<StorageError>() {
            Some(storage_err) => match storage_err {
                StorageError::VersionConflict { .. } => ApiError::BadRequest(err.to_string()),
                StorageError::NotFound(_) => ApiError::NotFound(err.to_string()),
                StorageError::AlreadyExists(_) => ApiError::InternalError(err.to_string()),
            },
            None => ApiError::InternalError(err.to_string()),
        }
    }
}

pub type ApiResult<T> = Result<T, ApiError>;
