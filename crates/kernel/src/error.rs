//! @managed rust-hex-api — единая ошибка приложения.
use axum::Json;
use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use serde::Serialize;

use crate::i18n::tr;

/// Коды ошибок [`Problem`] для API и MCP (поле `error` ответа).
pub mod codes {
    pub const VALIDATION: &str = "VALIDATION";
    /// Ссылка на запись, которой нет (счёт, категория, план).
    pub const NOT_FOUND: &str = "NOT_FOUND";
    /// Сумма позиции не сходится с количеством × ценой − скидкой.
    pub const AMOUNT_MISMATCH: &str = "AMOUNT_MISMATCH";
    /// Категория не того типа (доход в расходе и наоборот).
    pub const KIND_MISMATCH: &str = "KIND_MISMATCH";
    /// Внешняя запись уже создала операцию, и та удалена: повторно не создаётся.
    pub const EXTERNAL_REF_DELETED: &str = "EXTERNAL_REF_DELETED";
    /// Внешняя запись уже прикреплена к другой операции.
    pub const EXTERNAL_REF_TAKEN: &str = "EXTERNAL_REF_TAKEN";
}

/// Ошибка входных данных с кодом для API и текстом на языке пользователя (техдолг #9).
/// Текст переводится при создании — на язык текущего запроса ([`crate::i18n::current`]).
#[derive(Debug, Clone, PartialEq)]
pub struct Problem {
    /// Один из [`codes`].
    pub code: &'static str,
    /// Поле формы или запроса (`items[2].amount`); пусто — ошибка не привязана к полю.
    pub field: String,
    pub message: String,
}

impl Problem {
    pub fn new(code: &'static str, field: &str, message: String) -> Self {
        Self { code, field: field.to_string(), message }
    }

    pub fn validation(field: &str, message: String) -> Self {
        Self::new(codes::VALIDATION, field, message)
    }
}

/// `поле: текст` — формат, который разбирают формы (`web::FormError`).
impl std::fmt::Display for Problem {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        if self.field.is_empty() { f.write_str(&self.message) } else { write!(f, "{}: {}", self.field, self.message) }
    }
}

impl From<Problem> for AppError {
    fn from(p: Problem) -> Self {
        AppError::Invalid(p)
    }
}

#[derive(Debug)]
pub enum AppError {
    NotFound(String),
    Validation(String),
    /// Неверные входные данные с кодом и полем (422). Новые ошибки — так, а не `Validation(String)`.
    Invalid(Problem),
    Conflict(String),
    /// Конфликт с кодом и полем (409): тело — как у `Invalid`, код — `p.code` (`EXTERNAL_REF_TAKEN`).
    ConflictWith(Problem),
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
            AppError::Invalid(p) | AppError::ConflictWith(p) => write!(f, "{p}"),
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
            return AppError::NotFound(tr(None, "err.not_found", &[]));
        }
        if let sqlx::Error::Database(db) = &e
            && db.is_unique_violation() {
                return AppError::Conflict(tr(None, "err.duplicate", &[]));
            }
        AppError::Internal(e.into())
    }
}

impl AppError {
    pub fn status(&self) -> StatusCode {
        match self {
            AppError::NotFound(_) => StatusCode::NOT_FOUND,
            AppError::Validation(_) | AppError::Invalid(_) => StatusCode::UNPROCESSABLE_ENTITY,
            AppError::Conflict(_) | AppError::ConflictWith(_) => StatusCode::CONFLICT,
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
            AppError::Validation(_) => codes::VALIDATION,
            AppError::Invalid(p) | AppError::ConflictWith(p) => p.code,
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
                tr(None, "err.unavailable", &[])
            }
            AppError::Internal(e) => {
                eprintln!("internal error: {e:?}");
                tr(None, "err.internal", &[])
            }
            other => other.to_string(),
        }
    }
}

#[derive(Serialize)]
struct ErrorBody {
    error: &'static str,
    message: String,
    /// Поле запроса, к которому относится ошибка (`items[2].amount`).
    #[serde(skip_serializing_if = "Option::is_none")]
    field: Option<String>,
}

impl IntoResponse for AppError {
    fn into_response(self) -> Response {
        let status = self.status();
        let error = self.code();
        let body = match self {
            AppError::Invalid(p) | AppError::ConflictWith(p) => ErrorBody { error, message: p.message, field: Some(p.field).filter(|f| !f.is_empty()) },
            other => ErrorBody { error, message: other.public_message(), field: None },
        };
        (status, Json(body)).into_response()
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

    /// Техдолг #9: ошибка с кодом — на языке запроса, поле отдельно от текста.
    #[tokio::test]
    async fn problem_is_translated_and_keeps_field() {
        let p = crate::i18n::scope(crate::i18n::Lang::En, async { crate::code::code("items[2].category_code", " ") }).await.unwrap_err();
        assert_eq!((p.code, p.field.as_str(), p.message.as_str()), (codes::VALIDATION, "items[2].category_code", "cannot be empty"));
        assert_eq!(AppError::from(p.clone()).to_string(), "items[2].category_code: cannot be empty");
        let th = crate::i18n::scope(crate::i18n::Lang::Th, async { AppError::Internal(anyhow::anyhow!("x")).public_message() }).await;
        assert_eq!(th, "เซิร์ฟเวอร์ขัดข้อง");
        assert_eq!(AppError::from(p).into_response().status(), StatusCode::UNPROCESSABLE_ENTITY);
    }

    /// Конфликт с кодом — 409, код и поле в теле, как у ошибки проверки.
    #[tokio::test]
    async fn conflict_with_code_is_409() {
        let e = AppError::ConflictWith(Problem::new(codes::EXTERNAL_REF_TAKEN, "external_ref", "taken".into()));
        assert_eq!((e.status(), e.code(), e.to_string().as_str()), (StatusCode::CONFLICT, "EXTERNAL_REF_TAKEN", "external_ref: taken"));
        let res = e.into_response();
        assert_eq!(res.status(), StatusCode::CONFLICT);
        let body = axum::body::to_bytes(res.into_body(), 1024).await.unwrap();
        let v: serde_json::Value = serde_json::from_slice(&body).unwrap();
        assert_eq!(v, serde_json::json!({"error": "EXTERNAL_REF_TAKEN", "message": "taken", "field": "external_ref"}));
    }

    #[test]
    fn internal_stays_500() {
        let e = AppError::Internal(anyhow::anyhow!("pool timed out"));
        assert_eq!((e.status(), e.code()), (StatusCode::INTERNAL_SERVER_ERROR, "INTERNAL"));
        assert_eq!(e.public_message(), "внутренняя ошибка сервера");
    }
}
