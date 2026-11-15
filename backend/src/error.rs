use axum::{
    Json,
    http::StatusCode,
    response::{IntoResponse, Response},
};
use serde::Serialize;
use thiserror::Error;

#[derive(Debug, Error)]
pub enum AppError {
    #[error("not authenticated")]
    Unauthenticated,
    #[error("forbidden")]
    Forbidden,
    #[error("required profile fields are missing")]
    IncompleteProfile,
    #[error("invalid input")]
    InvalidInput(&'static str),
    #[error("resource conflict")]
    Conflict(&'static str),
    #[error("resource not found")]
    NotFound,
    #[error("authentication service unavailable")]
    AuthUnavailable,
    #[error("internal service error")]
    Internal,
}

#[derive(Serialize)]
struct ErrorBody {
    error: ErrorDetails,
}
#[derive(Serialize)]
struct ErrorDetails {
    code: &'static str,
    message: &'static str,
}

impl IntoResponse for AppError {
    fn into_response(self) -> Response {
        let (status, code, message) = match self {
            Self::Unauthenticated => (
                StatusCode::UNAUTHORIZED,
                "unauthenticated",
                "Sign in is required.",
            ),
            Self::Forbidden => (
                StatusCode::FORBIDDEN,
                "forbidden",
                "You do not have access to this resource.",
            ),
            Self::IncompleteProfile => (
                StatusCode::UNPROCESSABLE_ENTITY,
                "profile_incomplete",
                "Your employee profile is incomplete; no attendance event was recorded.",
            ),
            Self::InvalidInput(_) => (
                StatusCode::BAD_REQUEST,
                "invalid_input",
                "The request is invalid.",
            ),
            Self::Conflict(_) => (
                StatusCode::CONFLICT,
                "conflict",
                "The requested change conflicts with the current state.",
            ),
            Self::NotFound => (
                StatusCode::NOT_FOUND,
                "not_found",
                "The requested resource was not found.",
            ),
            Self::AuthUnavailable => (
                StatusCode::SERVICE_UNAVAILABLE,
                "auth_unavailable",
                "Identity verification is temporarily unavailable.",
            ),
            Self::Internal => (
                StatusCode::INTERNAL_SERVER_ERROR,
                "internal_error",
                "The service could not complete the request.",
            ),
        };
        (
            status,
            Json(ErrorBody {
                error: ErrorDetails { code, message },
            }),
        )
            .into_response()
    }
}
