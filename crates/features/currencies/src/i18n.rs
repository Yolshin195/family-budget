//! Строки интерфейса фичи «Валюты».
use kernel::i18n::Tr;

pub fn dict(key: &str) -> Option<Tr> {
    Some(match key {
        "nav" => ("Валюты", "Currencies", "สกุลเงิน"),
        "title" => ("Валюты", "Currencies", "สกุลเงิน"),
        "new" => ("Новая валюта", "New currency", "สกุลเงินใหม่"),
        "edit_title" => ("Валюта", "Currency", "สกุลเงิน"),
        "symbol" => ("Символ", "Symbol", "สัญลักษณ์"),
        "minor_units" => ("Знаков после запятой", "Decimal places", "ทศนิยม"),
        "code_hint" => ("ISO 4217: THB, RUB, USD", "ISO 4217: THB, RUB, USD", "ISO 4217: THB, RUB, USD"),
        "base" => ("базовая", "base", "สกุลหลัก"),
        "empty_title" => ("Валют пока нет", "No currencies yet", "ยังไม่มีสกุลเงิน"),
        "empty_text" => (
            "Добавьте валюты ваших счетов — бюджет пересчитает их в базовую по курсу на дату.",
            "Add the currencies of your accounts — the budget converts them to the base one at the date's rate.",
            "เพิ่มสกุลเงินของบัญชี งบจะแปลงเป็นสกุลหลักตามอัตราของวันนั้น",
        ),
        "rates_link" => ("Курсы к базовой валюте →", "Rates to the base currency →", "อัตราแลกเปลี่ยนกับสกุลหลัก →"),
        _ => return None,
    })
}
