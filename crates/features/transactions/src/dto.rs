//! Контракт HTTP API фичи «Транзакции» и преобразования DTO ⇄ домен.
//!
//! Клиент присылает тип и поля формы, сервер строит из них проводки; ответ всегда содержит проводки.

use chrono::NaiveDate;
use kernel::i18n::{Lang, Localized};
use rust_decimal::Decimal;
use serde::{Deserialize, Serialize};
use utoipa::{IntoParams, ToSchema};
use uuid::Uuid;

use kernel::error::Problem;

use crate::domain::{Body, Description, Draft, Fee, Item, ItemDraft, ItemsDraft, Transaction, TransactionData, TxKind};

/// Позиция чека в старом формате `lines`: категория и сумма. Принимается по-прежнему — как позиция
/// «1 шт» с названием из `note`; `lines` заменяют все позиции операции.
#[derive(Debug, Clone, Deserialize, Serialize, ToSchema)]
pub struct LineDto {
    /// Категория; расход — `EXPENSE`, доход — `INCOME`
    #[schema(example = "FOOD_GROCERIES")]
    pub category_code: String,
    /// Сумма > 0 в валюте счёта
    #[schema(value_type = String, example = "900.00")]
    pub amount: Decimal,
    #[serde(default)]
    pub note: Option<String>,
}

/// Позиция чека в запросе (docs/group-1.md, 5.1): категория и сумма или количество × цена − скидка.
/// При правке: с `id` — изменить позицию (пустые категория и название остаются прежними, пустые все
/// денежные поля — тоже), без `id` — добавить.
#[derive(Debug, Clone, Default, Deserialize, ToSchema)]
pub struct ItemInput {
    /// Позиция этой операции (при правке)
    #[serde(default)]
    pub id: Option<Uuid>,
    /// Категория; расход — `EXPENSE`, доход — `INCOME`
    #[serde(default)]
    #[schema(example = "FOOD_SNACKS")]
    pub category_code: Option<String>,
    /// Название на языке ввода
    #[serde(default)]
    #[schema(example = "Круассан")]
    pub name: Option<String>,
    /// Количество > 0, до 6 знаков после запятой (по умолчанию 1)
    #[serde(default)]
    #[schema(value_type = Option<String>, example = "2")]
    pub quantity: Option<Decimal>,
    /// pcs, kg, l, g (по умолчанию pcs)
    #[serde(default)]
    #[schema(example = "pcs")]
    pub unit: Option<String>,
    /// Цена за единицу ≥ 0; с ценой сумма считается сама: round(quantity × price_per_unit) − discount
    #[serde(default)]
    #[schema(value_type = Option<String>, example = "25")]
    pub price_per_unit: Option<Decimal>,
    /// Скидка на позицию ≥ 0
    #[serde(default)]
    #[schema(value_type = Option<String>, example = "3")]
    pub discount: Option<Decimal>,
    /// Сумма позиции > 0 в валюте счёта; вместе с ценой — сверяется с ней (допуск — единица младшего разряда)
    #[serde(default)]
    #[schema(value_type = Option<String>, example = "47")]
    pub amount: Option<Decimal>,
    #[serde(default)]
    pub note: Option<String>,
    /// Для кого (код из справочника людей); `all` или "" — «Для всех». Не указано — как в шапке
    /// (`for_person_code` операции), у позиции с `id` — прежний получатель
    #[serde(default)]
    #[schema(example = "PINPAN")]
    pub for_person_code: Option<String>,
}

/// Получатель из запроса: `all` или пусто — «Для всех» явно.
pub fn recipient_of(value: Option<String>) -> Option<Option<String>> {
    value.map(|v| Some(v.trim().to_string()).filter(|v| !v.is_empty() && !v.eq_ignore_ascii_case(ALL)))
}

/// «Для всех» в запросах и фильтрах (код человека не может быть таким — его занимает справочник).
pub const ALL: &str = "all";

impl ItemInput {
    fn into_draft(self) -> ItemDraft {
        ItemDraft {
            id: self.id,
            category_code: self.category_code,
            name: self.name,
            quantity: self.quantity,
            unit: self.unit,
            price: self.price_per_unit,
            discount: self.discount,
            amount: self.amount,
            note: self.note,
            for_person: recipient_of(self.for_person_code),
        }
    }
}

/// Позиция чека в ответе.
#[derive(Debug, Clone, Serialize, ToSchema)]
pub struct ItemDto {
    pub id: Uuid,
    #[schema(example = "FOOD_SNACKS")]
    pub category_code: String,
    pub name: Option<String>,
    #[schema(value_type = String, example = "2")]
    pub quantity: Decimal,
    /// pcs, kg, l, g
    #[schema(example = "pcs")]
    pub unit: String,
    #[schema(value_type = Option<String>, example = "25")]
    pub price_per_unit: Option<Decimal>,
    #[schema(value_type = String, example = "3")]
    pub discount: Decimal,
    /// Сумма позиции (после скидки)
    #[schema(value_type = String, example = "47")]
    pub amount: Decimal,
    pub note: Option<String>,
    /// Для кого (код из справочника людей); пусто — «Для всех»
    pub for_person_code: Option<String>,
}

impl From<Item> for ItemDto {
    fn from(x: Item) -> Self {
        Self {
            id: x.id,
            category_code: x.category_code,
            name: x.name,
            quantity: x.quantity.normalize(),
            unit: x.unit.as_str().to_string(),
            price_per_unit: x.price.map(|p| p.normalize()),
            discount: x.discount,
            amount: x.amount,
            note: x.note,
            for_person_code: x.for_person,
        }
    }
}

impl ItemDto {
    /// Одна штука без названия, цены и скидки — как обычная операция.
    pub fn is_simple(&self) -> bool {
        self.name.is_none() && self.price_per_unit.is_none() && self.quantity == Decimal::ONE && self.unit == "pcs" && self.discount.is_zero()
    }
}

/// Проводка: куда относится, валюта и сумма со знаком. Сумма проводок по каждой валюте = 0.
#[derive(Debug, Clone, Serialize, ToSchema)]
pub struct PostingDto {
    /// ACCOUNT — счёт, CATEGORY — категория, SYSTEM — системный счёт (EXCHANGE, OPENING)
    #[schema(example = "ACCOUNT")]
    pub ref_kind: String,
    #[schema(example = "CARD_KBANK")]
    pub ref_code: String,
    #[schema(example = "THB")]
    pub currency_code: String,
    /// «+» — счёт пополнился или деньги ушли на категорию расхода; «−» — наоборот
    #[schema(value_type = String, example = "-1250.00")]
    pub amount: Decimal,
    pub note: Option<String>,
}

/// Тело запроса на создание и изменение: тип и поля формы.
///
/// - `EXPENSE` / `INCOME`: `account_code`, `items` (или старый формат `lines`), необязательно `refund` (возврат —
///   обратные знаки) и `discount_total` (скидка на чек). При правке позиции — по `id`, `remove_item_ids` удаляет.
/// - `TRANSFER`: `account_code` (откуда), `to_account_code` (куда), `amount` (списано всего, вместе с
///   комиссией), `to_amount` (зачислено — только при разных валютах), необязательно
///   `fee_category_code` + `fee_amount` (комиссия, расход в валюте списания).
/// - `OPENING`: `account_code`, `amount` (начальный остаток, может быть отрицательным).
#[derive(Debug, Default, Deserialize, ToSchema)]
pub struct TransactionInput {
    pub occurred_on: NaiveDate,
    #[schema(example = "EXPENSE")]
    pub tx_kind: String,
    /// Счёт (у перевода — счёт списания); можно не передавать, если у места есть счёт по умолчанию
    #[serde(default)]
    #[schema(example = "CARD_KBANK")]
    pub account_code: String,
    #[serde(default)]
    pub refund: bool,
    /// Позиции чека (расход и доход)
    #[serde(default)]
    pub items: Vec<ItemInput>,
    /// Удалить позиции операции (при правке)
    #[serde(default)]
    pub remove_item_ids: Vec<Uuid>,
    /// Скидка на чек: делится по позициям пропорционально сумме; при правке прибавляется к записанным скидкам
    #[serde(default)]
    #[schema(value_type = Option<String>, example = "10")]
    pub discount_total: Option<Decimal>,
    /// Старый формат позиций: категория, сумма, уточнение. Заменяет все позиции операции
    #[serde(default)]
    pub lines: Vec<LineDto>,
    /// Позиции из формы — весь чек: не названные в `items` удаляются
    #[serde(skip)]
    pub replace_items: bool,
    #[serde(default)]
    pub to_account_code: Option<String>,
    #[serde(default)]
    #[schema(value_type = Option<String>, example = "10050.00")]
    pub amount: Option<Decimal>,
    #[serde(default)]
    #[schema(value_type = Option<String>, example = "3912.40")]
    pub to_amount: Option<Decimal>,
    #[serde(default)]
    pub fee_category_code: Option<String>,
    #[serde(default)]
    #[schema(value_type = Option<String>, example = "50.00")]
    pub fee_amount: Option<Decimal>,
    /// Описание на языке ввода
    #[serde(default)]
    pub description: Option<String>,
    /// Язык оригинала описания: RU / EN / TH
    #[schema(example = "RU")]
    pub description_lang: String,
    #[serde(default)]
    pub description_ru: Option<String>,
    #[serde(default)]
    pub description_en: Option<String>,
    #[serde(default)]
    pub description_th: Option<String>,
    /// План, вхождение которого оплачено (вместе с `plan_date`)
    #[serde(default)]
    pub plan_id: Option<Uuid>,
    #[serde(default)]
    pub plan_date: Option<NaiveDate>,
    /// Место (расход и доход): подставляет счёт и категорию новых позиций, если их не указали
    #[serde(default)]
    #[schema(example = "SEVEN_ELEVEN")]
    pub place_code: Option<String>,
    /// Для кого по умолчанию (расход и доход): код из справочника людей, `all` — «Для всех». Получают позиции без
    /// своего `for_person_code`; при правке — ещё и позиции с прежним общим получателем
    #[serde(default)]
    #[schema(example = "PINPAN")]
    pub for_person_code: Option<String>,
}

/// Переводы описания (оригинал не меняется).
#[derive(Debug, Deserialize, ToSchema)]
pub struct TranslationInput {
    #[serde(default)]
    pub description_ru: Option<String>,
    #[serde(default)]
    pub description_en: Option<String>,
    #[serde(default)]
    pub description_th: Option<String>,
}

#[derive(Debug, Clone, Serialize, ToSchema)]
pub struct TransactionDto {
    pub id: Uuid,
    pub occurred_on: NaiveDate,
    #[schema(example = "EXPENSE")]
    pub tx_kind: String,
    /// Счёт; у перевода — счёт списания
    pub account_code: String,
    /// Счёт зачисления (перевод)
    pub to_account_code: Option<String>,
    /// Возврат (расход или доход с обратными знаками)
    pub refund: bool,
    /// Позиции чека (расход и доход)
    pub items: Vec<ItemDto>,
    /// Старый формат позиций — для прежних клиентов API: категория, сумма, название в `note`. Новое — `items`
    pub lines: Vec<LineDto>,
    /// Сумма транзакции: сумма позиций; у перевода — списано всего; у начального остатка — остаток
    #[schema(value_type = String)]
    pub total: Decimal,
    /// Зачислено (перевод с обменом валют)
    #[schema(value_type = Option<String>)]
    pub to_amount: Option<Decimal>,
    pub fee_category_code: Option<String>,
    #[schema(value_type = Option<String>)]
    pub fee_amount: Option<Decimal>,
    /// Валюта счёта (у перевода — счёта списания)
    pub currency: String,
    /// Валюта счёта зачисления (перевод)
    pub to_currency: Option<String>,
    pub description: Option<String>,
    #[schema(example = "RU")]
    pub description_lang: String,
    pub description_ru: Option<String>,
    pub description_en: Option<String>,
    pub description_th: Option<String>,
    /// Кто внёс
    pub created_by: Uuid,
    pub plan_id: Option<Uuid>,
    pub plan_date: Option<NaiveDate>,
    /// Место (расход и доход)
    pub place_code: Option<String>,
    /// Для кого: общий получатель всех позиций; пусто — «Для всех» или разные (`recipients_differ`)
    pub for_person_code: Option<String>,
    /// У позиций разные получатели
    pub recipients_differ: bool,
    pub postings: Vec<PostingDto>,
}

/// Фильтр списка.
#[derive(Debug, Default, Deserialize, IntoParams)]
pub struct ListQuery {
    pub from: Option<NaiveDate>,
    pub to: Option<NaiveDate>,
    /// Транзакции с проводкой на этот счёт
    pub account_code: Option<String>,
    /// Тип: EXPENSE / INCOME / TRANSFER / OPENING
    pub tx_kind: Option<String>,
    pub created_by: Option<Uuid>,
    /// Только без перевода описания на этот язык (ru / en / th)
    pub untranslated: Option<String>,
    /// Только операции в этом месте
    pub place_code: Option<String>,
    /// Только операции с позицией для этого человека (`all` — для всех); итоги ленты — по этим позициям
    pub for_person_code: Option<String>,
}

impl From<Transaction> for TransactionDto {
    fn from(x: Transaction) -> Self {
        let total = x.total();
        let currency_of = |code: &str| x.postings.iter().find(|p| p.is_account(code)).map(|p| p.currency.clone()).unwrap_or_default();
        let (account_code, to_account_code, refund, items, to_amount, fee) = match x.body() {
            Body::Flow { account, items, refund, .. } => (account, None, refund, items, None, None),
            Body::Transfer { from, to, to_amount, fee, .. } => (from, Some(to), false, Vec::new(), to_amount, fee),
            Body::Opening { account, .. } => (account, None, false, Vec::new(), None, None),
        };
        let for_person_code = crate::domain::common_recipient(&items);
        let recipients_differ = items.iter().any(|i| i.for_person != items[0].for_person);
        Self {
            for_person_code,
            recipients_differ,
            id: x.id,
            occurred_on: x.occurred_on,
            tx_kind: x.tx_kind.as_str().to_string(),
            currency: currency_of(&account_code),
            to_currency: to_account_code.as_deref().map(currency_of),
            account_code,
            to_account_code,
            refund,
            lines: items.iter().map(|x| LineDto { category_code: x.category_code.clone(), amount: x.amount, note: x.name.clone() }).collect(),
            items: items.into_iter().map(Into::into).collect(),
            total,
            to_amount,
            fee_category_code: fee.as_ref().map(|f| f.category_code.clone()),
            fee_amount: fee.map(|f| f.amount),
            description: x.description.text,
            description_lang: x.description.lang.upper().to_string(),
            description_ru: x.description.ru,
            description_en: x.description.en,
            description_th: x.description.th,
            created_by: x.created_by,
            plan_id: x.plan.map(|p| p.0),
            plan_date: x.plan.map(|p| p.1),
            place_code: x.place_code,
            postings: x
                .postings
                .into_iter()
                .map(|p| PostingDto { ref_kind: p.ref_kind.as_str().into(), ref_code: p.ref_code, currency_code: p.currency, amount: p.amount, note: p.note })
                .collect(),
        }
    }
}

impl TransactionInput {
    pub fn into_domain(self) -> Result<TransactionData, Problem> {
        let err = |field: &str, key: &str, args: &[(&str, &dyn std::fmt::Display)]| Problem::validation(field, kernel::i18n::tr(Some(crate::i18n::dict), key, args));
        let tx_kind = TxKind::parse(&self.tx_kind).ok_or_else(|| err("tx_kind", "err.kind_invalid", &[("value", &self.tx_kind)]))?;
        let lang = Lang::parse(&self.description_lang).ok_or_else(|| err("description_lang", "err.lang_invalid", &[("value", &self.description_lang)]))?;
        let plan = match (self.plan_id, self.plan_date) {
            (Some(id), Some(date)) => Some((id, date)),
            (None, None) => None,
            _ => return Err(err("plan_id", "err.plan_pair", &[])),
        };
        let body = match tx_kind {
            TxKind::Expense | TxKind::Income => {
                let items = match (self.lines.is_empty(), self.items.is_empty()) {
                    (false, false) => return Err(err("items", "err.lines_and_items", &[])),
                    // Старый формат: категория, сумма, уточнение → название; весь чек целиком.
                    (false, true) => ItemsDraft::replace(
                        self.lines.into_iter().map(|l| ItemDraft { name: l.note, ..ItemDraft::simple(&l.category_code, l.amount) }).collect(),
                    ),
                    _ => ItemsDraft {
                        items: self.items.into_iter().map(ItemInput::into_draft).collect(),
                        replace: self.replace_items,
                        remove: self.remove_item_ids,
                        ..Default::default()
                    },
                };
                let items = ItemsDraft { discount_total: self.discount_total, for_default: recipient_of(self.for_person_code), ..items };
                Draft::Flow { income: tx_kind == TxKind::Income, account: self.account_code.clone(), items, refund: self.refund }
            }
            TxKind::Transfer => Draft::Transfer {
                from: self.account_code.clone(),
                to: self.to_account_code.clone().filter(|s| !s.trim().is_empty()).ok_or_else(|| err("to_account_code", "err.to_account_required", &[]))?,
                amount: self.amount.ok_or_else(|| err("amount", "err.amount_required", &[]))?,
                to_amount: self.to_amount,
                fee: match (self.fee_category_code.clone().filter(|s| !s.trim().is_empty()), self.fee_amount) {
                    (Some(category_code), Some(amount)) => Some(Fee { category_code, amount }),
                    (None, None) => None,
                    (None, Some(_)) => return Err(err("fee_category_code", "err.fee_category_required", &[])),
                    (Some(_), None) => return Err(err("fee_amount", "err.fee_amount_required", &[])),
                },
            },
            TxKind::Opening => Draft::Opening { account: self.account_code.clone(), amount: self.amount.ok_or_else(|| err("amount", "err.opening_required", &[]))? },
        };
        Ok(TransactionData {
            occurred_on: self.occurred_on,
            description: Description { text: self.description, lang, ru: self.description_ru, en: self.description_en, th: self.description_th },
            plan,
            place_code: self.place_code,
            body,
        })
    }
}

impl TransactionDto {
    fn localized(&self) -> Option<Localized<'_>> {
        self.description.as_deref().map(|original| Localized {
            original,
            lang: Lang::parse(&self.description_lang).unwrap_or_default(),
            ru: self.description_ru.as_deref(),
            en: self.description_en.as_deref(),
            th: self.description_th.as_deref(),
        })
    }

    /// Описание на языке интерфейса (пусто, если описания нет).
    pub fn title(&self, lang: impl Into<Lang>) -> &str {
        let lang = lang.into();
        self.localized().map(|l| l.show(lang)).unwrap_or("")
    }

    /// Есть описание, но нет перевода на язык интерфейса.
    pub fn missing_tr(&self, lang: impl Into<Lang>) -> bool {
        let lang = lang.into();
        self.localized().is_some_and(|l| l.missing(lang))
    }

    pub fn is_expense(&self) -> bool {
        self.tx_kind == "EXPENSE"
    }

    pub fn is_income(&self) -> bool {
        self.tx_kind == "INCOME"
    }

    pub fn is_transfer(&self) -> bool {
        self.tx_kind == "TRANSFER"
    }

    pub fn is_opening(&self) -> bool {
        self.tx_kind == "OPENING"
    }

    /// Как транзакция изменила остаток счёта.
    pub fn amount_on(&self, account: &str) -> Decimal {
        self.postings.iter().filter(|p| p.ref_kind == "ACCOUNT" && p.ref_code == account).map(|p| p.amount).sum()
    }

    /// Сколько ушло на перевод без комиссии.
    pub fn sent(&self) -> Decimal {
        self.total - self.fee_amount.unwrap_or_default()
    }
}

/// Счёт в форме быстрого расхода.
#[derive(Debug, Clone, Serialize, ToSchema)]
pub struct QuickAccountDto {
    pub code: String,
    pub title: String,
    pub currency: String,
    /// Символ валюты для показа рядом с суммой (฿, $)
    pub symbol: String,
    /// Курс 1 единицы валюты счёта к базовой валюте на дату (для предупреждения о лимите и курса перевода);
    /// `null` — курса нет: сумма не войдёт в бюджет, пока его не добавят
    #[schema(value_type = Option<String>)]
    pub rate_to_base: Option<Decimal>,
}

/// Категория в форме быстрого расхода: дерево, поиск на трёх языках, остаток лимита за месяц.
#[derive(Debug, Clone, Serialize, ToSchema)]
pub struct QuickCategoryDto {
    pub code: String,
    pub parent_code: Option<String>,
    /// EXPENSE / INCOME
    pub kind: String,
    /// Название на языке интерфейса
    pub title: String,
    pub depth: usize,
    /// Оригинал, переводы и код через `\u{1f}` — для поиска на любом из трёх языков
    pub search: String,
    /// Лимит узла за месяц (в базовой валюте), если задан
    #[schema(value_type = Option<String>)]
    pub limit: Option<Decimal>,
    /// Факт + план за месяц по поддереву
    #[schema(value_type = String)]
    pub forecast: Decimal,
    /// Сколько ещё можно потратить с учётом предков
    #[schema(value_type = Option<String>)]
    pub available: Option<Decimal>,
}

/// Подсказки для быстрого расхода: всё, чтобы добавить операцию за ≤ 5 секунд.
#[derive(Debug, Clone, Serialize, ToSchema)]
pub struct QuickDefaultsDto {
    pub date: NaiveDate,
    pub base_currency: String,
    /// Последний счёт, которым пользователь пользовался в расходах и доходах проекта
    pub default_account: Option<String>,
    /// Счета последнего перевода пользователя: (откуда, куда)
    pub last_transfer: Option<(String, String)>,
    pub accounts: Vec<QuickAccountDto>,
    /// До 6 частых категорий расходов пользователя за 30 дней
    pub frequent_expense: Vec<String>,
    /// До 6 частых категорий доходов
    pub frequent_income: Vec<String>,
    pub categories: Vec<QuickCategoryDto>,
    /// Активные места: сначала частые в расходах пользователя за 30 дней, затем по названию. Пусто — мест нет
    pub places: Vec<QuickPlaceDto>,
    /// Коды частых мест (из `places`)
    pub frequent_places: Vec<String>,
}

/// Место в форме быстрого расхода: что оно подставит.
#[derive(Debug, Clone, Serialize, ToSchema)]
pub struct QuickPlaceDto {
    pub code: String,
    /// «7-Eleven · Сукхумвит 24»
    pub title: String,
    /// Название, варианты написания и код через `\u{1f}` — для поиска
    pub search: String,
    /// Категория по умолчанию (если активна)
    pub category: Option<String>,
    /// Счёт по умолчанию (если активен)
    pub account: Option<String>,
}

#[derive(Debug, Default, Deserialize, IntoParams)]
pub struct QuickQuery {
    /// Дата операции (по умолчанию — сегодня): от неё считаются месяц лимитов и курсы
    pub date: Option<NaiveDate>,
}

/// Остаток счёта на дату.
#[derive(Debug, Clone, Serialize, ToSchema)]
pub struct BalanceDto {
    pub account_code: String,
    #[schema(value_type = String)]
    pub balance: Decimal,
}

#[derive(Debug, Default, Deserialize, IntoParams)]
pub struct BalanceQuery {
    /// Дата (по умолчанию — сегодня)
    pub date: Option<NaiveDate>,
}
