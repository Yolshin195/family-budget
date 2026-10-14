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
    /// Нет прав (например, роль VIEWER пытается писать).
    Forbidden(String),
    /// Нет входа: сессия или API-токен не предъявлены, истекли или отозваны.
    Unauthorized(String),
    /// Превышен лимит запросов.
    TooManyRequests(String),
    /// Внешний сервис, без которого запрос не выполнить (провайдер входа Rauthy), временно недоступен.
    /// Это сбой окружения, а не ошибка приложения: 503, подробности — только в журнале сервера.
    Unavailable(anyhow::Error),
    Internal(anyhow::Error),
}

pub type AppResult<T> = Result<T, AppError>;

impl std::fmt::Display for AppError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            AppError::NotFound(m)
            | AppError::Validation(m)
            | AppError::Conflict(m)
            | AppError::Forbidden(m)
            | AppError::Unauthorized(m)
            | AppError::TooManyRequests(m) => f.write_str(m),
            AppError::Unavailable(e) | AppError::Internal(e) => write!(f, "{e}"),
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

impl AppError {
    pub fn status(&self) -> StatusCode {
        match self {
            AppError::NotFound(_) => StatusCode::NOT_FOUND,
            AppError::Validation(_) => StatusCode::UNPROCESSABLE_ENTITY,
            AppError::Conflict(_) => StatusCode::CONFLICT,
            AppError::Forbidden(_) => StatusCode::FORBIDDEN,
            AppError::Unauthorized(_) => StatusCode::UNAUTHORIZED,
            AppError::TooManyRequests(_) => StatusCode::TOO_MANY_REQUESTS,
            AppError::Unavailable(_) => StatusCode::SERVICE_UNAVAILABLE,
            AppError::Internal(_) => StatusCode::INTERNAL_SERVER_ERROR,
        }
    }

    pub fn code(&self) -> &'static str {
        match self {
            AppError::NotFound(_) => "NOT_FOUND",
            AppError::Validation(_) => "VALIDATION",
            AppError::Conflict(_) => "CONFLICT",
            AppError::Forbidden(_) => "FORBIDDEN",
            AppError::Unauthorized(_) => "UNAUTHORIZED",
            AppError::TooManyRequests(_) => "TOO_MANY_REQUESTS",
            AppError::Unavailable(_) => "UNAVAILABLE",
            AppError::Internal(_) => "INTERNAL",
        }
    }

    /// Текст для пользователя: у внутренних ошибок и недоступных сервисов подробности не раскрываются
    /// (они — в журнале сервера).
    pub fn public_message(&self) -> String {
        match self {
            AppError::Unavailable(e) => {
                eprintln!("service unavailable: {e:?}");
                "сервис временно недоступен, попробуйте позже".to_string()
            }
            AppError::Internal(e) => {
                eprintln!("internal error: {e:?}");
                "внутренняя ошибка сервера".to_string()
            }
            other => other.to_string(),
        }
    }
}

#[derive(Serialize)]
struct ErrorBody {
    error: &'static str,
    message: String,
}

impl IntoResponse for AppError {
    fn into_response(self) -> Response {
        let status = self.status();
        let error = self.code();
        let message = self.public_message();
        (status, Json(ErrorBody { error, message })).into_response()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn unavailable_is_503_without_details() {
        let e = AppError::Unavailable(anyhow::anyhow!("Rauthy недоступен: connection refused (rauthy:8443)"));
        assert_eq!(e.status(), StatusCode::SERVICE_UNAVAILABLE);
        assert_eq!(e.code(), "UNAVAILABLE");
        let message = e.public_message();
        assert!(!message.contains("Rauthy") && !message.contains("rauthy:8443"), "подробности не уходят клиенту: {message}");
        assert_eq!(e.into_response().status(), StatusCode::SERVICE_UNAVAILABLE);
    }

    #[test]
    fn internal_stays_500() {
        let e = AppError::Internal(anyhow::anyhow!("pool timed out"));
        assert_eq!((e.status(), e.code()), (StatusCode::INTERNAL_SERVER_ERROR, "INTERNAL"));
        assert_eq!(e.public_message(), "внутренняя ошибка сервера");
    }
}
