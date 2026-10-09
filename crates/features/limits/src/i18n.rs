//! Строки интерфейса фичи «Лимиты».
use kernel::i18n::Tr;

pub fn dict(key: &str) -> Option<Tr> {
    Some(match key {
        "nav" => ("Лимиты", "Limits", "วงเงิน"),
        "title" => ("Лимиты расходов", "Spending limits", "วงเงินรายจ่าย"),
        "period" => ("Период", "Period", "ช่วงเวลา"),
        "valid_from" => ("Действует с", "Valid from", "มีผลตั้งแต่"),
        "hint" => ("Лимит действует на всё поддерево категории, в базовой валюте проекта, до следующего лимита этой категории.",
                   "A limit covers the whole subtree of the category, in the project base currency, until the next limit for that category.",
                   "วงเงินครอบคลุมหมวดหมู่ย่อยทั้งหมด ในสกุลเงินหลักของโปรเจกต์ จนกว่าจะมีวงเงินใหม่ของหมวดนั้น"),
        "superseded" => ("заменён", "superseded", "ถูกแทนที่"),
        _ => return None,
    })
}
