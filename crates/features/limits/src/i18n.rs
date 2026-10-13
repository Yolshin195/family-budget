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
        "to_budget" => ("Бюджет по лимитам", "Budget by limits", "งบตามวงเงิน"),
        "empty_title" => ("Лимитов пока нет", "No limits yet", "ยังไม่มีวงเงิน"),
        "empty_text" => ("Задайте лимит на ветку категорий — например, «Еда» на месяц. Бюджет покажет остаток и предупредит при 80 %.", "Set a limit on a category branch, e.g. Food per month. The budget shows what's left and warns at 80%.", "ตั้งวงเงินให้หมวดหมู่ เช่น อาหารต่อเดือน งบจะแสดงยอดคงเหลือและเตือนที่ 80%"),
        _ => return None,
    })
}
