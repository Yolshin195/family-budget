//! @managed rust-hex-api — единая ошибка приложения.
use axum::Json;
use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use serde::Serialize;

#[derive(Debug)]
pub enum AppError {
    NotFound(String),
    Validation(String),
    Conflict(String),
    Internal(anyhow::Error),
}

pub type AppResult<T> = Result<T, AppError>;

impl std::fmt::Display for AppError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            AppError::NotFound(m) | AppError::Validation(m) | AppError::Conflict(m) => f.write_str(m),
            AppError::Internal(e) => write!(f, "{e}"),
        }
    }
}

impl From<anyhow::Error> for AppError {
    fn from(e: anyhow::Error) -> Self {
        AppError::Internal(e)
    }
}

impl From<sqlx::Error> for AppError {
    fn from(e: sqlx::Error) -> Self {
        if let sqlx::Error::RowNotFound = e {
            return AppError::NotFound("запись не найдена".into());
        }
        if let sqlx::Error::Database(db) = &e {
            if db.is_unique_violation() {
                return AppError::Conflict("запись с таким ключом уже существует".into());
            }
        }
        AppError::Internal(e.into())
    }
}

#[derive(Serialize)]
struct ErrorBody {
    error: &'static str,
    message: String,
}

impl IntoResponse for AppError {
    fn into_response(self) -> Response {
        let (status, error) = match &self {
            AppError::NotFound(_) => (StatusCode::NOT_FOUND, "NOT_FOUND"),
            AppError::Validation(_) => (StatusCode::UNPROCESSABLE_ENTITY, "VALIDATION"),
            AppError::Conflict(_) => (StatusCode::CONFLICT, "CONFLICT"),
            AppError::Internal(e) => {
                eprintln!("internal error: {e:?}");
                (StatusCode::INTERNAL_SERVER_ERROR, "INTERNAL")
            }
        };
        let message = match &self {
            AppError::Internal(_) => "внутренняя ошибка сервера".to_string(),
            other => other.to_string(),
        };
        (status, Json(ErrorBody { error, message })).into_response()
    }
}
