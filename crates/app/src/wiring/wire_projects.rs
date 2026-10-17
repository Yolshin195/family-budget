//! Связка фичи `projects`: начальное наполнение нового проекта через публичные API
//! `currencies`, `categories` и `accounts` — базовая валюта, стартовое дерево категорий на трёх
//! языках и счёт в базовой валюте (чтобы сразу можно было вносить операции);
//! удаление всех данных проекта во всех фичах (удаление проекта, аккаунта, сброс демо).

use std::sync::{Arc, OnceLock};

use async_trait::async_trait;
use kernel::error::{AppError, AppResult};
use kernel::i18n::Lang;
use uuid::Uuid;

#[derive(Clone)]
pub struct BootstrapAdapter {
    pub currencies: Arc<::currencies::Service>,
    pub categories: Arc<::categories::Service>,
    /// Фича `accounts` собирается после `projects` — подключается в конце сборки.
    #[cfg(feature = "accounts")]
    pub accounts: Arc<OnceLock<Arc<::accounts::Service>>>,
}

impl BootstrapAdapter {
    pub fn new(currencies: Arc<::currencies::Service>, categories: Arc<::categories::Service>) -> Self {
        Self {
            currencies,
            categories,
            #[cfg(feature = "accounts")]
            accounts: Arc::default(),
        }
    }
}

/// Стартовый счёт нового проекта в базовой валюте: (код, тип, ru, en, th).
#[cfg(feature = "accounts")]
pub const DEFAULT_ACCOUNT: (&str, &str, &str, &str, &str) = ("CASH", "CASH", "Наличные", "Cash", "เงินสด");

/// (код, родитель, тип, ru, en, th)
pub const DEFAULT_CATEGORIES: &[(&str, Option<&str>, &str, &str, &str, &str)] = &[
    ("HOME", None, "EXPENSE", "Жильё", "Housing", "ที่อยู่อาศัย"),
    ("HOME_RENT", Some("HOME"), "EXPENSE", "Аренда", "Rent", "ค่าเช่า"),
    ("HOME_UTIL", Some("HOME"), "EXPENSE", "Коммунальные услуги", "Utilities", "ค่าสาธารณูปโภค"),
    ("HOME_ELECTRICITY", Some("HOME_UTIL"), "EXPENSE", "Электричество", "Electricity", "ค่าไฟ"),
    ("HOME_WATER", Some("HOME_UTIL"), "EXPENSE", "Вода", "Water", "ค่าน้ำ"),
    ("HOME_INTERNET", Some("HOME_UTIL"), "EXPENSE", "Интернет и связь", "Internet & phone", "อินเทอร์เน็ตและโทรศัพท์"),
    ("FOOD", None, "EXPENSE", "Еда", "Food", "อาหาร"),
    ("FOOD_GROCERIES", Some("FOOD"), "EXPENSE", "Продукты", "Groceries", "ของชำ"),
    ("FOOD_CAFE", Some("FOOD"), "EXPENSE", "Кафе и рестораны", "Cafes & restaurants", "ร้านอาหาร"),
    ("TRANSPORT", None, "EXPENSE", "Транспорт", "Transport", "การเดินทาง"),
    ("TRANSPORT_TAXI", Some("TRANSPORT"), "EXPENSE", "Такси", "Taxi", "แท็กซี่"),
    ("TRANSPORT_FUEL", Some("TRANSPORT"), "EXPENSE", "Топливо", "Fuel", "น้ำมัน"),
    ("HOUSEHOLD", None, "EXPENSE", "Хозяйство", "Household", "ของใช้ในบ้าน"),
    ("HEALTH", None, "EXPENSE", "Здоровье", "Health", "สุขภาพ"),
    ("KIDS", None, "EXPENSE", "Дети", "Kids", "ลูก"),
    ("FUN", None, "EXPENSE", "Развлечения", "Entertainment", "ความบันเทิง"),
    ("FUN_SUBSCRIPTIONS", Some("FUN"), "EXPENSE", "Подписки", "Subscriptions", "บริการสมาชิก"),
    // Комиссии банков и обменников: расход перевода с обменом валют (ТЗ, раздел 4).
    ("FEES", None, "EXPENSE", "Комиссии", "Fees", "ค่าธรรมเนียม"),
    ("SALARY", None, "INCOME", "Зарплата", "Salary", "เงินเดือน"),
    ("FREELANCE", None, "INCOME", "Подработка", "Side income", "รายได้เสริม"),
    ("OTHER_INCOME", None, "INCOME", "Прочие доходы", "Other income", "รายได้อื่น"),
];

/// Названия известных валют: (код, символ, ru, en, th).
pub const KNOWN_CURRENCIES: &[(&str, &str, &str, &str, &str)] = &[
    ("THB", "฿", "Тайский бат", "Thai baht", "บาท"),
    ("RUB", "₽", "Российский рубль", "Russian ruble", "รูเบิลรัสเซีย"),
    ("USD", "$", "Доллар США", "US dollar", "ดอลลาร์สหรัฐ"),
    ("EUR", "€", "Евро", "Euro", "ยูโร"),
];

/// Текст на языке `lang` + переводы на два других (перевод на язык оригинала не хранится).
pub fn localized(lang: Lang, ru: &str, en: &str, th: &str) -> (String, String, Option<String>, Option<String>, Option<String>) {
    let other = |l: Lang, v: &str| (l != lang).then(|| v.to_string());
    let text = match lang { Lang::Ru => ru, Lang::En => en, Lang::Th => th };
    (text.to_string(), lang.upper().to_string(), other(Lang::Ru, ru), other(Lang::En, en), other(Lang::Th, th))
}

pub async fn ensure_currency(svc: &::currencies::Service, project_id: Uuid, code: &str, lang: Lang) -> AppResult<()> {
    if svc.exists(project_id, code).await? {
        return Ok(());
    }
    let known = KNOWN_CURRENCIES.iter().find(|c| c.0 == code);
    let (symbol, ru, en, th) = known.map(|c| (c.1, c.2, c.3, c.4)).unwrap_or((code, code, code, code));
    let (name, name_lang, name_ru, name_en, name_th) = localized(lang, ru, en, th);
    let dto = ::currencies::dto::CreateCurrencyDto { code: code.into(), name, name_lang, name_ru, name_en, name_th, symbol: symbol.into(), minor_units: 2 };
    match svc.create(project_id, dto).await {
        Ok(_) | Err(AppError::Conflict(_)) => Ok(()),
        Err(e) => Err(e),
    }
}

#[async_trait]
impl ::projects::ProjectBootstrap for BootstrapAdapter {
    async fn init(&self, project_id: Uuid, base_currency: &str, lang: Lang) -> AppResult<()> {
        ensure_currency(&self.currencies, project_id, base_currency, lang).await?;
        for (i, (code, parent, kind, ru, en, th)) in DEFAULT_CATEGORIES.iter().enumerate() {
            let (name, name_lang, name_ru, name_en, name_th) = localized(lang, ru, en, th);
            let dto = ::categories::dto::CreateCategoryDto {
                code: code.to_string(),
                name,
                name_lang,
                name_ru,
                name_en,
                name_th,
                category_kind: kind.to_string(),
                parent_code: parent.map(str::to_string),
                sort_order: (i as i32 + 1) * 10,
            };
            match self.categories.create(project_id, dto).await {
                Ok(_) | Err(AppError::Conflict(_)) => {}
                Err(e) => return Err(e),
            }
        }
        #[cfg(feature = "accounts")]
        if let Some(accounts) = self.accounts.get() {
            let (code, kind, ru, en, th) = DEFAULT_ACCOUNT;
            let (name, name_lang, name_ru, name_en, name_th) = localized(lang, ru, en, th);
            let dto = ::accounts::dto::CreateAccountDto {
                code: code.into(),
                name,
                name_lang,
                name_ru,
                name_en,
                name_th,
                account_kind: kind.into(),
                currency_code: base_currency.into(),
                holder_user_id: None,
            };
            match accounts.create(project_id, dto).await {
                Ok(_) | Err(AppError::Conflict(_)) => {}
                Err(e) => return Err(e),
            }
        }
        Ok(())
    }
}

/// Кто хранит данные проекта: сервисы фич с `purge_project`. Заполняется в конце сборки.
#[derive(Default)]
pub struct PurgeTargets {
    pub list: Vec<Arc<dyn PurgeOne>>,
}

#[async_trait]
pub trait PurgeOne: Send + Sync {
    async fn purge(&self, project_id: Uuid) -> AppResult<u64>;
}

macro_rules! purge_one {
    ($feature:literal, $svc:ty) => {
        #[cfg(feature = $feature)]
        #[async_trait]
        impl PurgeOne for $svc {
            async fn purge(&self, project_id: Uuid) -> AppResult<u64> {
                self.purge_project(project_id).await
            }
        }
    };
}

purge_one!("transactions", ::transactions::Service);
purge_one!("planning", ::planning::Service);
purge_one!("limits", ::limits::Service);
purge_one!("rates", ::rates::Service);
purge_one!("accounts", ::accounts::Service);
purge_one!("categories", ::categories::Service);
purge_one!("currencies", ::currencies::Service);
purge_one!("mcp", ::mcp::Service);

/// Порт `ProjectPurge`: по очереди во всех включённых фичах (сначала зависящие — потом справочники).
#[derive(Clone, Default)]
pub struct PurgeAdapter(pub Arc<OnceLock<PurgeTargets>>);

#[async_trait]
impl ::projects::ProjectPurge for PurgeAdapter {
    async fn purge(&self, project_id: Uuid) -> AppResult<()> {
        if let Some(t) = self.0.get() {
            for f in &t.list {
                f.purge(project_id).await?;
            }
        }
        Ok(())
    }
}
