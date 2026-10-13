//! Строки интерфейса фичи «Счета».
use kernel::i18n::Tr;

pub fn dict(key: &str) -> Option<Tr> {
    Some(match key {
        "nav" => ("Счета", "Accounts", "บัญชี"),
        "title" => ("Счета", "Accounts", "บัญชี"),
        "new" => ("Новый счёт", "New account", "บัญชีใหม่"),
        "edit_title" => ("Счёт", "Account", "บัญชี"),
        "account_kind" => ("Тип счёта", "Account type", "ประเภทบัญชี"),
        "CASH" => ("Наличные", "Cash", "เงินสด"),
        "CARD" => ("Карта", "Card", "บัตร"),
        "BANK" => ("Банковский счёт", "Bank account", "บัญชีธนาคาร"),
        "SAVINGS" => ("Накопления", "Savings", "เงินออม"),
        "opening_balance" => ("Начальный остаток", "Opening balance", "ยอดเริ่มต้น"),
        "holder" => ("Держатель", "Holder", "ผู้ถือ"),
        "nobody" => ("общий", "shared", "ใช้ร่วมกัน"),
        "code_hint" => ("Латиница, без пробелов: CARD_KBANK", "Latin, no spaces: CARD_KBANK", "อักษรละติน ไม่มีช่องว่าง: CARD_KBANK"),
        "empty_title" => ("Счетов пока нет", "No accounts yet", "ยังไม่มีบัญชี"),
        "empty_text" => ("Добавьте наличные, карты и банковские счета — с них списываются расходы.", "Add cash, cards and bank accounts — expenses are paid from them.", "เพิ่มเงินสด บัตร และบัญชีธนาคาร รายจ่ายจะตัดจากบัญชีเหล่านี้"),
        "holder_hint" => ("Чья это карта — только для справки, доступ не ограничивает.", "Whose card it is — just for reference, it doesn't restrict access.", "บัตรของใคร — เพื่ออ้างอิงเท่านั้น ไม่จำกัดสิทธิ์"),
        _ => return None,
    })
}
