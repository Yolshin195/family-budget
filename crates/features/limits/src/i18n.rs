//! Строки интерфейса фичи «Лимиты».
use kernel::i18n::Tr;

pub fn dict(key: &str) -> Option<Tr> {
    Some(match key {
        "nav" => ("Лимиты", "Limits", "วงเงิน"),
        "title" => ("Лимиты расходов", "Spending limits", "วงเงินรายจ่าย"),
        "period" => ("Период", "Period", "ช่วงเวลา"),
        "valid_from" => ("Действует с", "Valid from", "มีผลตั้งแต่"),
        "hint" => (
            "Лимит действует на всё поддерево категории, в базовой валюте проекта, с начала периода, в который попала дата, и до следующего лимита этой категории. 0 — тратить нельзя совсем; «Снять лимит» — с этой даты лимита нет, прошлые периоды его сохраняют.",
            "A limit covers the whole subtree of the category, in the project base currency, from the start of the period containing the date until the next limit for that category. 0 means no spending at all; “Remove limit” means no limit from that date, past periods keep theirs.",
            "วงเงินครอบคลุมหมวดหมู่ย่อยทั้งหมด ในสกุลเงินหลักของโปรเจกต์ ตั้งแต่ต้นช่วงเวลาที่มีวันที่นั้น จนกว่าจะมีวงเงินใหม่ของหมวดนั้น 0 คือห้ามใช้จ่ายเลย ส่วน «ยกเลิกวงเงิน» คือไม่มีวงเงินตั้งแต่วันนั้น ช่วงที่ผ่านมายังคงวงเงินเดิม",
        ),
        "remove" => ("Снять лимит с даты", "Remove limit from date", "ยกเลิกวงเงินตั้งแต่วันที่"),
        "remove_hint" => (
            "С этой даты у категории нет лимита, сумму вводить не нужно",
            "From this date the category has no limit, no amount needed",
            "ตั้งแต่วันนี้หมวดนี้ไม่มีวงเงิน ไม่ต้องใส่จำนวน",
        ),
        "removed" => ("лимит снят", "limit removed", "ยกเลิกวงเงินแล้ว"),
        "effective_from" => ("действует с", "in force from", "มีผลตั้งแต่"),
        "superseded" => ("заменён", "superseded", "ถูกแทนที่"),
        "to_budget" => ("Бюджет по лимитам", "Budget by limits", "งบตามวงเงิน"),
        "empty_title" => ("Лимитов пока нет", "No limits yet", "ยังไม่มีวงเงิน"),
        "empty_text" => (
            "Задайте лимит на ветку категорий — например, «Еда» на месяц. Бюджет покажет остаток и предупредит при 80 %.",
            "Set a limit on a category branch, e.g. Food per month. The budget shows what's left and warns at 80%.",
            "ตั้งวงเงินให้หมวดหมู่ เช่น อาหารต่อเดือน งบจะแสดงยอดคงเหลือและเตือนที่ 80%",
        ),
        _ => return None,
    })
}
