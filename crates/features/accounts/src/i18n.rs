//! Строки интерфейса фичи «Счета».
use kernel::i18n::Tr;

pub fn dict(key: &str) -> Option<Tr> {
    Some(match key {
        "nav" => ("Счета", "Accounts", "บัญชี"),
        "title" => ("Счета", "Accounts", "บัญชี"),
        "new" => ("Новый счёт", "New account", "บัญชีใหม่"),
        "edit" => ("Счёт", "Account", "บัญชี"),
        "account_kind" => ("Тип счёта", "Account type", "ประเภทบัญชี"),
        "CASH" => ("Наличные", "Cash", "เงินสด"),
        "CARD" => ("Карта", "Card", "บัตร"),
        "BANK" => ("Банковский счёт", "Bank account", "บัญชีธนาคาร"),
        "SAVINGS" => ("Накопления", "Savings", "เงินออม"),
        "opening_balance" => ("Начальный остаток", "Opening balance", "ยอดเริ่มต้น"),
        "holder" => ("Держатель", "Holder", "ผู้ถือ"),
        "nobody" => ("общий", "shared", "ใช้ร่วมกัน"),
        "code_hint" => ("Латиница, без пробелов: CARD_KBANK", "Latin, no spaces: CARD_KBANK", "อักษรละติน ไม่มีช่องว่าง: CARD_KBANK"),
        _ => return None,
    })
}
