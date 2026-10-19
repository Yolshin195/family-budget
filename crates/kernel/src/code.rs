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

/// Самый длинный код новой записи.
pub const MAX_CODE: usize = 40;

/// Код новой записи справочника: латиница, цифры и `_`, до [`MAX_CODE`] знаков (CARD_KBANK, SEVEN_ELEVEN).
/// Код подставляется в URL, поэтому `/ ? # &` и кириллица в нём не нужны (техдолг #7). Ссылки на
/// записи разбирает [`normalize`]: коды, созданные до этого правила, остаются рабочими.
pub fn new_code(field: &str, value: &str) -> Result<String, String> {
    fresh_code(field, value).map_err(|p| p.to_string())
}

/// [`new_code`] с ошибкой [`Problem`].
pub fn fresh_code(field: &str, value: &str) -> Result<String, Problem> {
    let code = code(field, value)?;
    if code.len() > MAX_CODE || !code.bytes().all(|b| b.is_ascii_uppercase() || b.is_ascii_digit() || b == b'_') {
        return Err(Problem::validation(field, tr(None, "err.code_chars", &[("max", &MAX_CODE)])));
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

#[cfg(test)]
mod tests {
    use super::*;

    /// Техдолг #7: новый код — только латиница, цифры и `_`; ссылки по старым кодам работают.
    #[tokio::test]
    async fn new_codes_are_url_safe() {
        assert_eq!(new_code("code", " card_kbank ").unwrap(), "CARD_KBANK");
        assert_eq!(new_code("code", "7_eleven").unwrap(), "7_ELEVEN");
        for bad in ["FOOD/CAFE", "a?b", "x#1", "a&b", "ЕДА", "café", "a-b", &"A".repeat(MAX_CODE + 1)] {
            assert!(new_code("code", bad).is_err(), "{bad}");
        }
        assert_eq!(new_code("code", &"A".repeat(MAX_CODE)).unwrap().len(), MAX_CODE);
        let en = crate::i18n::scope(crate::i18n::Lang::En, async { fresh_code("code", "ЕДА") }).await.unwrap_err();
        assert_eq!(en.message, "only Latin letters, digits and _, up to 40 characters");
        assert_eq!(normalize("code", "food/cafe").unwrap(), "FOOD/CAFE", "старый код по ссылке находится");
    }
}
