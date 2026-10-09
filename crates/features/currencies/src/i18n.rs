//! Строки интерфейса фичи «Валюты».
use kernel::i18n::Tr;

pub fn dict(key: &str) -> Option<Tr> {
    Some(match key {
        "nav" => ("Валюты", "Currencies", "สกุลเงิน"),
        "title" => ("Валюты", "Currencies", "สกุลเงิน"),
        "new" => ("Новая валюта", "New currency", "สกุลเงินใหม่"),
        "edit" => ("Валюта", "Currency", "สกุลเงิน"),
        "symbol" => ("Символ", "Symbol", "สัญลักษณ์"),
        "minor_units" => ("Знаков после запятой", "Decimal places", "ทศนิยม"),
        "code_hint" => ("ISO 4217: THB, RUB, USD", "ISO 4217: THB, RUB, USD", "ISO 4217: THB, RUB, USD"),
        "base" => ("базовая", "base", "สกุลหลัก"),
        _ => return None,
    })
}
