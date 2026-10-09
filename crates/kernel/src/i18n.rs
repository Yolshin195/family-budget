//! Три языка интерфейса и данных: ru, en, th.
//!
//! - Строки интерфейса — словари в коде: ядро (`dict`) + каждая фича свой `i18n::dict`.
//! - Переводимые поля данных хранятся как оригинал `X` + язык `X_lang` + переводы `X_ru/X_en/X_th`;
//!   показ — [`pick`].

/// Язык. В БД и API — `RU`/`EN`/`TH`, в cookie и URL — `ru`/`en`/`th`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub enum Lang {
    #[default]
    Ru,
    En,
    Th,
}

impl Lang {
    pub const ALL: [Lang; 3] = [Lang::Ru, Lang::En, Lang::Th];

    /// `ru` / `en` / `th`.
    pub fn code(self) -> &'static str {
        match self {
            Lang::Ru => "ru",
            Lang::En => "en",
            Lang::Th => "th",
        }
    }

    /// `RU` / `EN` / `TH` — значение enum-полей `*_lang` в БД.
    pub fn upper(self) -> &'static str {
        match self {
            Lang::Ru => "RU",
            Lang::En => "EN",
            Lang::Th => "TH",
        }
    }

    /// Разбор без учёта регистра; `ru-RU`, `th_TH` тоже понимаются.
    pub fn parse(value: &str) -> Option<Lang> {
        let v = value.trim().to_ascii_lowercase();
        match v.get(..2)? {
            "ru" => Some(Lang::Ru),
            "en" => Some(Lang::En),
            "th" => Some(Lang::Th),
            _ => None,
        }
    }

    /// Первый поддерживаемый язык из заголовка `Accept-Language`.
    pub fn from_accept_language(header: &str) -> Option<Lang> {
        header.split(',').find_map(|part| Lang::parse(part.split(';').next().unwrap_or("")))
    }

    /// Выбрать перевод строки интерфейса.
    pub fn pick(self, t: Tr) -> &'static str {
        match self {
            Lang::Ru => t.0,
            Lang::En => t.1,
            Lang::Th => t.2,
        }
    }
}

/// Строка интерфейса на трёх языках: (ru, en, th).
pub type Tr = (&'static str, &'static str, &'static str);

/// Словарь фичи: ключ → перевод. `None` — ключа нет, искать дальше (в ядре).
pub type Dict = fn(&str) -> Option<Tr>;

/// Переводимое поле записи: оригинал, его язык и переводы.
#[derive(Debug, Clone, Copy)]
pub struct Localized<'a> {
    pub original: &'a str,
    pub lang: Lang,
    pub ru: Option<&'a str>,
    pub en: Option<&'a str>,
    pub th: Option<&'a str>,
}

impl<'a> Localized<'a> {
    pub fn translation(&self, lang: Lang) -> Option<&'a str> {
        let t = match lang {
            Lang::Ru => self.ru,
            Lang::En => self.en,
            Lang::Th => self.th,
        };
        t.map(str::trim).filter(|s| !s.is_empty())
    }

    /// Текст на языке `lang`: оригинал, если язык совпадает; иначе перевод; иначе оригинал.
    pub fn show(&self, lang: Lang) -> &'a str {
        if lang == self.lang {
            return self.original;
        }
        self.translation(lang).unwrap_or(self.original)
    }

    /// Нет перевода на `lang` (и `lang` — не язык оригинала).
    pub fn missing(&self, lang: Lang) -> bool {
        lang != self.lang && self.translation(lang).is_none()
    }
}

/// Короткая форма [`Localized::show`] для мест, где поля уже под рукой.
pub fn pick<'a>(lang: Lang, original: &'a str, original_lang: Lang, ru: Option<&'a str>, en: Option<&'a str>, th: Option<&'a str>) -> &'a str {
    Localized { original, lang: original_lang, ru, en, th }.show(lang)
}

/// Перевод по ключу: словарь фичи → словарь ядра → сам ключ.
pub fn t(lang: Lang, feature: Option<Dict>, key: &str) -> String {
    feature
        .and_then(|d| d(key))
        .or_else(|| dict(key))
        .map(|tr| lang.pick(tr).to_string())
        .unwrap_or_else(|| key.to_string())
}

/// Общие строки интерфейса.
pub fn dict(key: &str) -> Option<Tr> {
    Some(match key {
        "app.title" => ("Семейный бюджет", "Family budget", "งบประมาณครอบครัว"),
        "nav.logout" => ("Выйти", "Log out", "ออกจากระบบ"),
        "nav.projects_manage" => ("Проекты и участники", "Projects & members", "โปรเจกต์และสมาชิก"),
        "nav.project" => ("Проект", "Project", "โปรเจกต์"),
        "save" => ("Сохранить", "Save", "บันทึก"),
        "cancel" => ("Отмена", "Cancel", "ยกเลิก"),
        "create" => ("Создать", "Create", "สร้าง"),
        "add" => ("Добавить", "Add", "เพิ่ม"),
        "edit" => ("Изменить", "Edit", "แก้ไข"),
        "delete" => ("Удалить", "Delete", "ลบ"),
        "archive" => ("В архив", "Archive", "เก็บถาวร"),
        "archived" => ("в архиве", "archived", "เก็บถาวรแล้ว"),
        "back" => ("Назад", "Back", "กลับ"),
        "code" => ("Код", "Code", "รหัส"),
        "name" => ("Название", "Name", "ชื่อ"),
        "date" => ("Дата", "Date", "วันที่"),
        "amount" => ("Сумма", "Amount", "จำนวนเงิน"),
        "total" => ("Итого", "Total", "รวม"),
        "account" => ("Счёт", "Account", "บัญชี"),
        "category" => ("Категория", "Category", "หมวดหมู่"),
        "currency" => ("Валюта", "Currency", "สกุลเงิน"),
        "description" => ("Описание", "Description", "คำอธิบาย"),
        "kind" => ("Тип", "Type", "ประเภท"),
        "expense" => ("Расход", "Expense", "รายจ่าย"),
        "income" => ("Доход", "Income", "รายรับ"),
        "EXPENSE" => ("Расход", "Expense", "รายจ่าย"),
        "INCOME" => ("Доход", "Income", "รายรับ"),
        "lang.original" => ("Язык оригинала", "Original language", "ภาษาต้นฉบับ"),
        "lang.translations" => ("Переводы", "Translations", "คำแปล"),
        "lang.no_translation" => ("нет перевода", "no translation", "ยังไม่มีคำแปล"),
        "RU" => ("Русский", "Russian", "รัสเซีย"),
        "EN" => ("Английский", "English", "อังกฤษ"),
        "TH" => ("Тайский", "Thai", "ไทย"),
        "empty" => ("Пока ничего нет", "Nothing here yet", "ยังไม่มีข้อมูล"),
        "readonly" => ("Только просмотр", "Read only", "ดูอย่างเดียว"),
        "error.title" => ("Ошибка", "Error", "ข้อผิดพลาด"),
        "today" => ("Сегодня", "Today", "วันนี้"),
        "week" => ("Неделя", "Week", "สัปดาห์"),
        "month" => ("Месяц", "Month", "เดือน"),
        "year" => ("Год", "Year", "ปี"),
        "WEEK" => ("Неделя", "Week", "สัปดาห์"),
        "MONTH" => ("Месяц", "Month", "เดือน"),
        "YEAR" => ("Год", "Year", "ปี"),
        "yes" => ("да", "yes", "ใช่"),
        "no" => ("нет", "no", "ไม่"),
        _ => return None,
    })
}

/// Названия месяцев для календаря (1..=12).
pub fn month_name(lang: Lang, month: u32) -> &'static str {
    const RU: [&str; 12] = ["Январь", "Февраль", "Март", "Апрель", "Май", "Июнь", "Июль", "Август", "Сентябрь", "Октябрь", "Ноябрь", "Декабрь"];
    const EN: [&str; 12] = ["January", "February", "March", "April", "May", "June", "July", "August", "September", "October", "November", "December"];
    const TH: [&str; 12] = ["มกราคม", "กุมภาพันธ์", "มีนาคม", "เมษายน", "พฤษภาคม", "มิถุนายน", "กรกฎาคม", "สิงหาคม", "กันยายน", "ตุลาคม", "พฤศจิกายน", "ธันวาคม"];
    let i = (month.clamp(1, 12) - 1) as usize;
    match lang {
        Lang::Ru => RU[i],
        Lang::En => EN[i],
        Lang::Th => TH[i],
    }
}

/// Короткие названия дней недели, с понедельника.
pub fn weekdays(lang: Lang) -> [&'static str; 7] {
    match lang {
        Lang::Ru => ["Пн", "Вт", "Ср", "Чт", "Пт", "Сб", "Вс"],
        Lang::En => ["Mon", "Tue", "Wed", "Thu", "Fri", "Sat", "Sun"],
        Lang::Th => ["จ.", "อ.", "พ.", "พฤ.", "ศ.", "ส.", "อา."],
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn show_prefers_original_then_translation() {
        let x = Localized { original: "Аренда", lang: Lang::Ru, ru: None, en: Some("Rent"), th: None };
        assert_eq!(x.show(Lang::Ru), "Аренда");
        assert_eq!(x.show(Lang::En), "Rent");
        assert_eq!(x.show(Lang::Th), "Аренда");
        assert!(x.missing(Lang::Th));
        assert!(!x.missing(Lang::Ru));
    }

    #[test]
    fn accept_language() {
        assert_eq!(Lang::from_accept_language("de-DE,th;q=0.9,en;q=0.8"), Some(Lang::Th));
        assert_eq!(Lang::from_accept_language("fr"), None);
    }
}

/// Нормализация необязательного перевода: пустая строка → `None`, пробелы по краям убраны.
pub fn clean(value: Option<String>) -> Option<String> {
    value.map(|v| v.trim().to_string()).filter(|v| !v.is_empty())
}

/// Askama передаёт поля шаблона по ссылке — методы для шаблонов принимают `impl Into<Lang>`.
impl From<&Lang> for Lang {
    fn from(l: &Lang) -> Self {
        *l
    }
}
