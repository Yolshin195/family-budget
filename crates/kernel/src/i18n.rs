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

tokio::task_local! {
    static CURRENT: Lang;
}

/// Язык текущего запроса — для сообщений, которые собираются вдали от страницы (ошибки домена и
/// сервисов, техдолг #9). Задают middleware: `auth::attach_meta` — из cookie и Accept-Language,
/// `auth::require_user` — с настройкой пользователя. Вне запроса (тесты, фоновые задачи) — русский.
pub fn current() -> Lang {
    CURRENT.try_with(|l| *l).unwrap_or_default()
}

/// Выполнить `f` с языком запроса `lang` (см. [`current`]).
pub async fn scope<F: std::future::Future>(lang: Lang, f: F) -> F::Output {
    CURRENT.scope(lang, f).await
}

/// Перевод на язык запроса с подстановкой параметров `{имя}`.
pub fn tr(feature: Option<Dict>, key: &str, args: &[(&str, &dyn std::fmt::Display)]) -> String {
    let mut s = t(current(), feature, key);
    for (name, value) in args {
        s = s.replace(&format!("{{{name}}}"), &value.to_string());
    }
    s
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
        "transfer" => ("Перевод", "Transfer", "โอน"),
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
        "close" => ("Закрыть", "Close", "ปิด"),
        "undo" => ("Отменить", "Undo", "เลิกทำ"),
        "loading" => ("Загрузка…", "Loading…", "กำลังโหลด…"),
        "retry" => ("Повторить", "Retry", "ลองอีกครั้ง"),
        "search" => ("Поиск", "Search", "ค้นหา"),
        "optional" => ("необязательно", "optional", "ไม่บังคับ"),
        "skip_to_content" => ("Перейти к содержимому", "Skip to content", "ข้ามไปยังเนื้อหา"),
        // каркас: боковое меню и нижняя панель
        "nav.main" => ("Основное меню", "Main menu", "เมนูหลัก"),
        "nav.add_expense" => ("Расход", "Expense", "รายจ่าย"),
        "nav.add_expense_hint" => ("Добавить расход (N)", "Add expense (N)", "เพิ่มรายจ่าย (N)"),
        "nav.directories" => ("Справочники", "Directories", "ข้อมูลอ้างอิง"),
        "nav.more" => ("Ещё", "More", "เพิ่มเติม"),
        "nav.profile" => ("Профиль", "Profile", "โปรไฟล์"),
        "nav.language" => ("Язык", "Language", "ภาษา"),
        "nav.collapse" => ("Свернуть меню", "Collapse menu", "ย่อเมนู"),
        "nav.expand" => ("Развернуть меню", "Expand menu", "ขยายเมนู"),
        "nav.switch_project" => ("Сменить проект", "Switch project", "สลับโปรเจกต์"),
        "nav.breadcrumbs" => ("Путь к странице", "Breadcrumbs", "เส้นทาง"),
        "nav.keys" => ("Горячие клавиши", "Keyboard shortcuts", "แป้นพิมพ์ลัด"),
        // быстрый расход (раздел 10)
        "quick.expense" => ("Новый расход", "New expense", "รายจ่ายใหม่"),
        "quick.income" => ("Новый доход", "New income", "รายรับใหม่"),
        "quick.transfer" => ("Новый перевод", "New transfer", "โอนใหม่"),
        "quick.from" => ("Откуда", "From", "จากบัญชี"),
        "quick.to" => ("Куда", "To", "ไปยังบัญชี"),
        "quick.received" => ("Зачислено", "Credited", "เข้าบัญชี"),
        "quick.choose_to" => ("Выберите другой счёт зачисления", "Choose another account to credit", "เลือกบัญชีปลายทางอื่น"),
        "quick.no_rate" => (
            "Нет курса {cur} к базовой валюте — сумма не войдёт в бюджет, пока не добавите курс",
            "No {cur} rate to the base currency — the amount won't count in the budget until you add a rate",
            "ไม่มีอัตรา {cur} เทียบสกุลหลัก — ยอดนี้จะไม่รวมในงบประมาณจนกว่าจะเพิ่มอัตรา",
        ),
        "quick.received_required" => ("Сколько пришло в валюте счёта?", "How much arrived, in the account's currency?", "เข้าบัญชีเท่าไรในสกุลเงินของบัญชี?"),
        "quick.rate" => ("Курс", "Rate", "อัตรา"),
        "quick.rate_ref" => ("по справочнику", "reference", "ตามข้อมูลอ้างอิง"),
        "quick.details" => ("С комиссией", "Add a fee", "เพิ่มค่าธรรมเนียม"),
        "quick.amount_hint" => ("Можно считать: 120+45", "You can add up: 120+45", "คำนวณได้: 120+45"),
        "quick.frequent" => ("Частые", "Frequent", "ใช้บ่อย"),
        "quick.search" => ("Найти категорию на любом языке", "Find a category in any language", "ค้นหาหมวดหมู่ได้ทุกภาษา"),
        "quick.no_match" => ("Ничего не нашлось", "No matches", "ไม่พบ"),
        "quick.yesterday" => ("Вчера", "Yesterday", "เมื่อวาน"),
        "quick.other_date" => ("Другая…", "Other…", "วันอื่น…"),
        "quick.split" => ("Подробная карточка", "Full details", "รายละเอียดทั้งหมด"),
        "quick.save_more" => ("Сохранить и ещё", "Save & add another", "บันทึกและเพิ่มอีก"),
        "quick.available" => ("Доступно", "Available", "ใช้ได้"),
        "quick.left" => ("Останется доступно", "Will remain", "จะเหลือ"),
        "quick.over" => ("Превышение лимита", "Over the limit by", "เกินวงเงิน"),
        "quick.warn" => ("Больше 80 % лимита", "Over 80% of the limit", "เกิน 80% ของวงเงิน"),
        "quick.saved_anyway" => ("Сохранить можно — это только предупреждение.", "You can still save — it's just a warning.", "บันทึกได้ — เป็นเพียงคำเตือน"),
        "quick.choose_category" => ("Выберите категорию", "Choose a category", "เลือกหมวดหมู่"),
        "quick.no_accounts" => ("Сначала добавьте счёт в «Справочники → Счета».", "Add an account first in Directories → Accounts.", "เพิ่มบัญชีก่อนใน ข้อมูลอ้างอิง → บัญชี"),
        "quick.failed" => ("Не удалось сохранить. Проверьте поля и повторите.", "Couldn't save. Check the fields and try again.", "บันทึกไม่สำเร็จ ตรวจสอบแล้วลองอีกครั้ง"),
        "quick.load_failed" => ("Не удалось загрузить форму. Проверьте соединение и повторите.", "Couldn't load the form. Check your connection and retry.", "โหลดฟอร์มไม่สำเร็จ ตรวจสอบการเชื่อมต่อแล้วลองใหม่"),
        "quick.undone" => ("Операция отменена", "Transaction undone", "ยกเลิกรายการแล้ว"),
        // горячие клавиши
        "keys.n" => ("Новый расход", "New expense", "รายจ่ายใหม่"),
        "keys.i" => ("Новый доход", "New income", "รายรับใหม่"),
        "keys.t" => ("Новый перевод между счетами", "New transfer between accounts", "โอนระหว่างบัญชีใหม่"),
        "keys.esc" => ("Закрыть окно", "Close dialog", "ปิดหน้าต่าง"),
        "keys.help" => ("Список клавиш", "Shortcut list", "รายการแป้นลัด"),
        "keys.enter" => ("Сохранить форму", "Save the form", "บันทึกฟอร์ม"),
        "keys.ctrl_enter" => ("Сохранить и добавить ещё", "Save and add another", "บันทึกและเพิ่มอีก"),
        // тосты
        "toast.deleted" => ("Удалено", "Deleted", "ลบแล้ว"),
        "toast.archived" => ("Перенесено в архив", "Archived", "เก็บถาวรแล้ว"),
        "toast.restored" => ("Восстановлено", "Restored", "กู้คืนแล้ว"),
        "toast.saved" => ("Сохранено", "Saved", "บันทึกแล้ว"),
        "toast.undo_expired" => ("Отменить уже нельзя — прошло слишком много времени", "Too late to undo", "หมดเวลายกเลิกแล้ว"),
        // состояния списков
        "state.load_failed" => ("Не удалось загрузить данные", "Couldn't load data", "โหลดข้อมูลไม่สำเร็จ"),
        "state.load_failed_hint" => ("Проверьте соединение и обновите страницу.", "Check your connection and reload the page.", "ตรวจสอบการเชื่อมต่อแล้วโหลดหน้าใหม่"),
        // страницы ошибок
        "error.404" => ("Страница не найдена", "Page not found", "ไม่พบหน้านี้"),
        "error.404_hint" => ("Адрес устарел или в нём опечатка. Вернитесь на главную.", "The link is outdated or has a typo. Go back to the start page.", "ลิงก์อาจเก่าหรือพิมพ์ผิด กลับไปหน้าแรก"),
        "error.403" => ("Нет доступа", "Access denied", "ไม่มีสิทธิ์เข้าถึง"),
        "error.403_hint" => ("У вашей роли нет прав на это действие. Попросите владельца проекта.", "Your role can't do this. Ask the project owner.", "บทบาทของคุณทำสิ่งนี้ไม่ได้ ติดต่อเจ้าของโปรเจกต์"),
        "error.429" => ("Слишком много запросов", "Too many requests", "คำขอมากเกินไป"),
        "error.429_hint" => ("Подождите минуту и повторите.", "Wait a minute and try again.", "รอสักครู่แล้วลองอีกครั้ง"),
        "error.500" => ("Что-то пошло не так", "Something went wrong", "เกิดข้อผิดพลาด"),
        "error.500_hint" => ("Мы уже знаем об ошибке. Обновите страницу через минуту.", "We've logged the error. Reload the page in a minute.", "เราบันทึกข้อผิดพลาดแล้ว โหลดหน้าใหม่อีกครั้งในอีกสักครู่"),
        "error.503" => ("Вход временно недоступен", "Sign-in is temporarily unavailable", "เข้าสู่ระบบไม่ได้ชั่วคราว"),
        "error.503_hint" => ("Сервис входа не отвечает. Попробуйте позже — обычно это несколько минут.", "The sign-in service isn't responding. Try again later — usually it takes a few minutes.", "บริการเข้าสู่ระบบไม่ตอบสนอง โปรดลองใหม่ภายหลัง โดยปกติใช้เวลาไม่กี่นาที"),
        "error.check_hint" => ("Проверьте введённые данные и повторите.", "Check what you entered and try again.", "ตรวจสอบข้อมูลแล้วลองอีกครั้ง"),
        "error.home" => ("На главную", "Go to start page", "ไปหน้าแรก"),
        // проект требует 2FA (раздел 13)
        "mfa.title" => ("Проект требует второй фактор", "This project requires two-factor sign-in", "โปรเจกต์นี้ต้องยืนยันตัวตนสองขั้นตอน"),
        "mfa.text" => ("Владелец проекта включил обязательную 2FA. Включите код по email или passkey в Rauthy и войдите заново — тогда данные проекта откроются.",
                       "The owner requires 2FA. Turn on email codes or a passkey in Rauthy and sign in again to see the project.",
                       "เจ้าของโปรเจกต์กำหนดให้ใช้ 2FA เปิดรหัสทางอีเมลหรือ passkey ใน Rauthy แล้วเข้าสู่ระบบอีกครั้ง"),
        "mfa.enable" => ("Включить 2FA", "Turn on 2FA", "เปิด 2FA"),
        "mfa.relogin" => ("Войти заново", "Sign in again", "เข้าสู่ระบบอีกครั้ง"),
        "mfa.other_project" => ("Или перейдите в другой проект", "Or switch to another project", "หรือสลับไปโปรเจกต์อื่น"),
        "OWNER" => ("Владелец", "Owner", "เจ้าของ"),
        "EDITOR" => ("Редактор", "Editor", "ผู้แก้ไข"),
        "VIEWER" => ("Наблюдатель", "Viewer", "ผู้ชม"),
        "theme.SYSTEM" => ("Как в системе", "System", "ตามระบบ"),
        "theme.LIGHT" => ("Светлая", "Light", "สว่าง"),
        "theme.DARK" => ("Тёмная", "Dark", "มืด"),
        // Ошибки ядра (техдолг #9): правила кодов и ссылок, ответы БД, сбои.
        "err.empty" => ("не может быть пустым", "cannot be empty", "ต้องไม่เว้นว่าง"),
        "err.no_spaces" => ("не должен содержать пробелов", "must not contain spaces", "ต้องไม่มีช่องว่าง"),
        "err.ref_not_found" => ("запись '{code}' не найдена", "'{code}' not found", "ไม่พบ '{code}'"),
        "err.not_found" => ("запись не найдена", "record not found", "ไม่พบรายการ"),
        "err.duplicate" => ("запись с таким ключом уже существует", "a record with this key already exists", "มีรายการที่ใช้คีย์นี้อยู่แล้ว"),
        "err.internal" => ("внутренняя ошибка сервера", "internal server error", "เซิร์ฟเวอร์ขัดข้อง"),
        "err.unavailable" => ("сервис временно недоступен, попробуйте позже", "the service is temporarily unavailable, try again later", "บริการไม่พร้อมใช้งานชั่วคราว ลองใหม่ภายหลัง"),
        "err.too_big" => ("слишком большое число", "the number is too large", "ตัวเลขใหญ่เกินไป"),
        "err.not_a_number" => ("'{value}' — не число", "'{value}' is not a number", "'{value}' ไม่ใช่ตัวเลข"),
        "err.enter_amount" => ("введите сумму", "enter an amount", "กรอกจำนวนเงิน"),
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

/// Для тестов полноты переводов: ключи `t("…")` из шаблонов и кода в `dirs`, которых нет ни
/// в словаре фичи, ни в словаре ядра (иначе пользователь увидел бы сам ключ).
pub fn missing_keys(dirs: &[std::path::PathBuf], feature: Option<Dict>) -> Vec<String> {
    fn walk(dir: &std::path::Path, out: &mut Vec<std::path::PathBuf>) {
        let Ok(entries) = std::fs::read_dir(dir) else { return };
        for e in entries.flatten() {
            let p = e.path();
            if p.is_dir() {
                walk(&p, out);
            } else if p.extension().is_some_and(|x| x == "html" || x == "rs") {
                out.push(p);
            }
        }
    }
    let mut files = Vec::new();
    for d in dirs {
        walk(d, &mut files);
    }
    let mut missing = Vec::new();
    for f in files {
        let text = std::fs::read_to_string(&f).unwrap_or_default();
        for (i, _) in text.match_indices("t(\"") {
            // `.t("…")` и `self.t("…")`, но не `format!("…")` и прочие вызовы, оканчивающиеся на t(.
            let before = text[..i].chars().last();
            if before != Some('.') {
                continue;
            }
            let rest = &text[i + 3..];
            let Some(end) = rest.find('"') else { continue };
            let key = &rest[..end];
            if key.is_empty() || !key.chars().all(|c| c.is_ascii_alphanumeric() || c == '_' || c == '.') {
                continue;
            }
            if feature.and_then(|d| d(key)).or_else(|| dict(key)).is_none() && !missing.contains(&key.to_string()) {
                missing.push(key.to_string());
            }
        }
    }
    missing
}

#[cfg(test)]
mod completeness {
    use super::*;

    #[test]
    fn kernel_templates_are_translated() {
        let root = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"));
        let missing = missing_keys(&[root.join("templates"), root.join("src")], None);
        assert!(missing.is_empty(), "нет перевода для ключей: {missing:?}");
    }

    #[test]
    fn no_empty_translations() {
        for key in ["app.title", "nav.add_expense", "quick.expense", "error.404", "mfa.title"] {
            let (ru, en, th) = dict(key).unwrap();
            assert!(!ru.is_empty() && !en.is_empty() && !th.is_empty(), "{key}");
        }
    }
}
