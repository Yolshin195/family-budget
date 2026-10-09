//! Строки интерфейса фичи «Транзакции».
use kernel::i18n::Tr;

pub fn dict(key: &str) -> Option<Tr> {
    Some(match key {
        "nav" => ("Операции", "Transactions", "รายการ"),
        "title" => ("Операции", "Transactions", "รายการ"),
        "new" => ("Новая операция", "New transaction", "รายการใหม่"),
        "edit" => ("Операция", "Transaction", "รายการ"),
        "lines" => ("Позиции", "Items", "รายการย่อย"),
        "add_line" => ("+ позиция", "+ item", "+ รายการย่อย"),
        "note" => ("Уточнение", "Note", "หมายเหตุ"),
        "remove" => ("Убрать", "Remove", "ลบ"),
        "author" => ("Кто внёс", "Added by", "ผู้บันทึก"),
        "all_accounts" => ("Все счета", "All accounts", "ทุกบัญชี"),
        "all_members" => ("Все участники", "All members", "สมาชิกทั้งหมด"),
        "untranslated" => ("Без перевода на мой язык", "Missing my language", "ยังไม่มีคำแปลภาษาของฉัน"),
        "translate" => ("Перевод", "Translate", "แปล"),
        "translate_title" => ("Перевод описания", "Translate description", "แปลคำอธิบาย"),
        "original" => ("Оригинал", "Original", "ต้นฉบับ"),
        "from_plan" => ("по плану", "planned", "ตามแผน"),
        "paid_plan" => ("Оплата по плану", "Planned payment", "ชำระตามแผน"),
        "income_total" => ("Доходы", "Income", "รายรับ"),
        "expense_total" => ("Расходы", "Expenses", "รายจ่าย"),
        "net" => ("Итог", "Net", "สุทธิ"),
        "confirm_delete" => ("Удалить операцию?", "Delete this transaction?", "ลบรายการนี้?"),
        "desc_hint" => ("Пишите на любом из трёх языков; переводы можно добавить позже.",
                        "Write in any of the three languages; translations can be added later.",
                        "เขียนได้ทั้งสามภาษา เพิ่มคำแปลภายหลังได้"),
        "no_desc" => ("Без описания", "No description", "ไม่มีคำอธิบาย"),
        _ => return None,
    })
}
