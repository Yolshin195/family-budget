//! @managed rust-hex-api — правила для кодов справочников и ссылок на них.
//! Сообщения — на языке запроса (`i18n::tr`, техдолг #9).
use crate::error::{AppError, AppResult, Problem};
use crate::i18n::tr;

/// Код справочника: непустой, без пробелов, в верхнем регистре (FOOD, USD, CARD_KBANK).
pub fn normalize(field: &str, value: &str) -> Result<String, String> {
    code(field, value).map_err(|p| p.to_string())
}

/// [`normalize`] с ошибкой [`Problem`] — для кода, который отдаёт ошибки с кодом (API, MCP).
pub fn code(field: &str, value: &str) -> Result<String, Problem> {
    let code = value.trim().to_uppercase();
    if code.is_empty() {
        return Err(Problem::validation(field, tr(None, "err.empty", &[])));
    }
    if code.chars().any(char::is_whitespace) {
        return Err(Problem::validation(field, tr(None, "err.no_spaces", &[])));
    }
    Ok(code)
}

/// Обязательный текст: непустой после trim.
pub fn require_text(field: &str, value: &str) -> Result<String, String> {
    let value = value.trim();
    if value.is_empty() {
        return Err(Problem::validation(field, tr(None, "err.empty", &[])).to_string());
    }
    Ok(value.to_string())
}

/// Результат проверки ссылки на запись другого справочника.
pub fn ensure_ref(exists: bool, field: &str, code: &str) -> AppResult<()> {
    if exists {
        Ok(())
    } else {
        Err(AppError::Validation(Problem::validation(field, tr(None, "err.ref_not_found", &[("code", &code)])).to_string()))
    }
}
