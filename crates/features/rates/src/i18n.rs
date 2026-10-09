//! Строки интерфейса фичи «Курсы валют».
use kernel::i18n::Tr;

pub fn dict(key: &str) -> Option<Tr> {
    Some(match key {
        "nav" => ("Курсы", "Rates", "อัตราแลกเปลี่ยน"),
        "title" => ("Курсы валют", "Exchange rates", "อัตราแลกเปลี่ยน"),
        "rate" => ("Курс", "Rate", "อัตรา"),
        "hint" => ("Сколько единиц базовой валюты за 1 единицу валюты. Операция пересчитывается по курсу на её дату (ближайший предыдущий).",
                   "Units of the base currency per 1 unit. An operation uses the rate on its date (latest before it).",
                   "จำนวนสกุลเงินหลักต่อ 1 หน่วย รายการจะใช้อัตรา ณ วันที่ทำรายการ (ล่าสุดก่อนหน้า)"),
        _ => return None,
    })
}
