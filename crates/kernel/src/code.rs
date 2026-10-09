//! @managed rust-hex-api — правила для кодов справочников и ссылок на них.
use crate::error::{AppError, AppResult};

/// Код справочника: непустой, без пробелов, в верхнем регистре (FOOD, USD, CARD_KBANK).
pub fn normalize(field: &str, value: &str) -> Result<String, String> {
    let code = value.trim().to_uppercase();
    if code.is_empty() {
        return Err(format!("{field}: не может быть пустым"));
    }
    if code.chars().any(char::is_whitespace) {
        return Err(format!("{field}: не должен содержать пробелов"));
    }
    Ok(code)
}

/// Обязательный текст: непустой после trim.
pub fn require_text(field: &str, value: &str) -> Result<String, String> {
    let value = value.trim();
    if value.is_empty() {
        return Err(format!("{field}: не может быть пустым"));
    }
    Ok(value.to_string())
}

/// Результат проверки ссылки на запись другого справочника.
pub fn ensure_ref(exists: bool, field: &str, code: &str) -> AppResult<()> {
    if exists {
        Ok(())
    } else {
        Err(AppError::Validation(format!("{field}: запись '{code}' не найдена")))
    }
}
