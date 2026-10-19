//! Строки интерфейса фичи «Курсы валют».
use kernel::i18n::Tr;

pub fn dict(key: &str) -> Option<Tr> {
    Some(match key {
        "nav" => ("Курсы", "Rates", "อัตราแลกเปลี่ยน"),
        "title" => ("Курсы валют", "Exchange rates", "อัตราแลกเปลี่ยน"),
        "rate" => ("Курс", "Rate", "อัตรา"),
        "hint" => ("Курс можно ввести в любую сторону — кнопкой ⇄. Операция пересчитывается по курсу на её дату (ближайший предыдущий).",
                   "Enter a rate in either direction — use the ⇄ button. An operation uses the rate on its date (latest before it).",
                   "ป้อนอัตราได้ทั้งสองทิศทาง — กดปุ่ม ⇄ รายการจะใช้อัตรา ณ วันที่ทำรายการ (ล่าสุดก่อนหน้า)"),
        "swap" => ("Поменять направление", "Swap direction", "สลับทิศทาง"),
        "last" => ("последний", "last", "ล่าสุด"),
        "per" => ("за", "per", "ต่อ"),
        "warn_less" => ("В {k} раз меньше прошлого курса", "{k} times lower than the previous rate", "ต่ำกว่าอัตราก่อนหน้า {k} เท่า"),
        "warn_more" => ("В {k} раз больше прошлого курса", "{k} times higher than the previous rate", "สูงกว่าอัตราก่อนหน้า {k} เท่า"),
        "warn_check" => ("Может, перепутано направление?", "Is the direction mixed up?", "อาจสลับทิศทางผิดหรือไม่?"),
        "base_is" => ("Базовая валюта проекта —", "The project's base currency is", "สกุลเงินหลักของโปรเจกต์คือ"),
        "empty_title" => ("Курсов пока нет", "No rates yet", "ยังไม่มีอัตราแลกเปลี่ยน"),
        "empty_text" => (
            "Добавьте курс для каждой валюты счетов, кроме базовой. Без курса суммы не входят в итоги бюджета.",
            "Add a rate for each account currency except the base one. Without a rate, amounts are left out of the budget totals.",
            "เพิ่มอัตราสำหรับทุกสกุลเงินของบัญชี ยกเว้นสกุลหลัก หากไม่มีอัตรา ยอดจะไม่รวมในยอดรวมของงบ",
        ),
        _ => return None,
    })
}
