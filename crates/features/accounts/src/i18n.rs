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
        "opening_balance" => ("Остаток", "Balance", "ยอดคงเหลือ"),
        "opening_date" => ("на дату", "as of", "ณ วันที่"),
        "opening_hint" => (
            "Сколько было на счёте на эту дату. Операции после неё изменят остаток.",
            "What the account held on that date. Later transactions change the balance.",
            "ยอดในบัญชี ณ วันนั้น รายการหลังจากนั้นจะเปลี่ยนยอด",
        ),
        "balance" => ("Остаток", "Balance", "ยอดคงเหลือ"),
        "statement" => ("Выписка", "Statement", "รายการเดินบัญชี"),
        "transfer" => ("Перевести", "Transfer", "โอน"),
        "reconcile" => ("Сверить", "Reconcile", "กระทบยอด"),
        // Остаток по данным банка — из последнего SMS банка с остатком.
        "bank_balance" => ("В банке", "At the bank", "ในธนาคาร"),
        "bank_hint" => (
            "Остаток из последнего SMS банка; сверяется с операциями на то же время",
            "Balance from the latest bank SMS, compared with the transactions up to the same time",
            "ยอดคงเหลือจาก SMS ล่าสุดของธนาคาร เทียบกับรายการ ณ เวลาเดียวกัน",
        ),
        "bank_ok" => ("Сходится", "Matches", "ตรงกัน"),
        "bank_more" => (
            "В операциях на {amount} больше, чем в банке",
            "Transactions show {amount} more than the bank",
            "ในรายการมากกว่าธนาคาร {amount}",
        ),
        "bank_less" => (
            "В операциях на {amount} меньше, чем в банке",
            "Transactions show {amount} less than the bank",
            "ในรายการน้อยกว่าธนาคาร {amount}",
        ),
        "bank_reconcile" => ("Сверить остаток", "Reconcile balance", "กระทบยอดคงเหลือ"),
        "holder" => ("Держатель", "Holder", "ผู้ถือ"),
        "nobody" => ("общий", "shared", "ใช้ร่วมกัน"),
        "code_hint" => (
            "Латиница, цифры и _: CARD_KBANK",
            "Latin letters, digits and _: CARD_KBANK",
            "อักษรละติน ตัวเลข และ _: CARD_KBANK",
        ),
        "empty_title" => ("Счетов пока нет", "No accounts yet", "ยังไม่มีบัญชี"),
        "empty_text" => (
            "Добавьте наличные, карты и банковские счета — с них списываются расходы.",
            "Add cash, cards and bank accounts — expenses are paid from them.",
            "เพิ่มเงินสด บัตร และบัญชีธนาคาร รายจ่ายจะตัดจากบัญชีเหล่านี้",
        ),
        "holder_hint" => (
            "Чья это карта — только для справки, доступ не ограничивает.",
            "Whose card it is — just for reference, it doesn't restrict access.",
            "บัตรของใคร — เพื่ออ้างอิงเท่านั้น ไม่จำกัดสิทธิ์",
        ),
        _ => return None,
    })
}
