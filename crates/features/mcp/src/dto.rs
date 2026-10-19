//! Параметры инструментов MCP (JSON Schema для ассистента). Описания — по-английски: так модели
//! надёжнее понимают поля. Суммы — строкой или числом, даты — `YYYY-MM-DD`.

use schemars::JsonSchema;
use serde::{Deserialize, Deserializer};

/// Сумма строкой («120.50», «120+45») или числом — ассистенты присылают и так, и так.
fn amount<'de, D: Deserializer<'de>>(d: D) -> Result<String, D::Error> {
    #[derive(Deserialize)]
    #[serde(untagged)]
    enum NumOrStr {
        Num(serde_json::Number),
        Str(String),
    }
    Ok(match NumOrStr::deserialize(d)? {
        NumOrStr::Num(n) => n.to_string(),
        NumOrStr::Str(s) => s,
    })
}

fn opt_amount<'de, D: Deserializer<'de>>(d: D) -> Result<Option<String>, D::Error> {
    #[derive(Deserialize)]
    struct Wrap(#[serde(deserialize_with = "amount")] String);
    Ok(Option::<Wrap>::deserialize(d)?.map(|w| w.0))
}

#[derive(Debug, Default, Deserialize, JsonSchema)]
pub struct Translations {
    /// Russian translation of the description
    pub ru: Option<String>,
    /// English translation of the description
    pub en: Option<String>,
    /// Thai translation of the description
    pub th: Option<String>,
}

#[derive(Debug, Default, Deserialize, JsonSchema)]
pub struct DateArgs {
    /// Date YYYY-MM-DD (default: today)
    pub date: Option<String>,
}

#[derive(Debug, Default, Deserialize, JsonSchema)]
pub struct ListTransactionsArgs {
    /// From date YYYY-MM-DD, inclusive (default: 30 days ago)
    pub from: Option<String>,
    /// To date YYYY-MM-DD, inclusive (default: today)
    pub to: Option<String>,
    /// Account code or name
    pub account: Option<String>,
    /// expense | income | transfer
    pub kind: Option<String>,
    /// Category code or name: only transactions with a line in this category or its subcategories
    pub category: Option<String>,
    /// Text to search in descriptions and line notes (any language)
    pub text: Option<String>,
    /// Page size, 1–200 (default 50)
    pub limit: Option<u32>,
    /// Skip this many transactions (paging)
    pub offset: Option<u32>,
}

#[derive(Debug, Default, Deserialize, JsonSchema)]
pub struct BudgetArgs {
    /// week | month | year (default: month)
    pub view: Option<String>,
    /// Any date inside the period, YYYY-MM-DD (default: today)
    pub date: Option<String>,
}

#[derive(Debug, Default, Deserialize, JsonSchema)]
pub struct UpcomingArgs {
    /// From date YYYY-MM-DD (default: today)
    pub from: Option<String>,
    /// To date YYYY-MM-DD (default: 30 days after `from`)
    pub to: Option<String>,
}

#[derive(Debug, Default, Deserialize, JsonSchema)]
pub struct ItemArgs {
    /// update_transaction only: id of an existing item of this transaction (from list_transactions) to change;
    /// omit to add a new item. Omitted fields of an existing item keep their values; a new quantity, price or
    /// discount recomputes the amount, an amount without a price removes the price. Error fields like
    /// items[3].amount count items in the receipt after the edit
    pub id: Option<String>,
    /// Category code or name (expense category for expenses, income category for income); required for new items
    pub category: Option<String>,
    /// Item name as on the receipt, in the language it was written ("Croissant", "Круассан"); not translated
    pub name: Option<String>,
    /// Quantity, e.g. "2" or "0.45" (default 1)
    #[serde(default, deserialize_with = "opt_amount")]
    #[schemars(with = "Option<String>")]
    pub quantity: Option<String>,
    /// Unit of the quantity: pcs | kg | l | g (default pcs)
    pub unit: Option<String>,
    /// Price per unit from the price tag; with a price the amount is computed: round(quantity × price) − discount
    #[serde(default, deserialize_with = "opt_amount")]
    #[schemars(with = "Option<String>")]
    pub price: Option<String>,
    /// Discount on this item in the account currency
    #[serde(default, deserialize_with = "opt_amount")]
    #[schemars(with = "Option<String>")]
    pub discount: Option<String>,
    /// Item amount in the account currency, e.g. "120.50" — required without a price; with a price it must match
    /// the computed amount (otherwise the error code is AMOUNT_MISMATCH)
    #[serde(default, deserialize_with = "opt_amount")]
    #[schemars(with = "Option<String>")]
    pub amount: Option<String>,
    /// Optional note for this item
    pub note: Option<String>,
}

#[derive(Debug, Default, Deserialize, JsonSchema)]
pub struct AddFlowArgs {
    /// Total amount in the account currency, e.g. "120.50" (use either amount+category or items)
    #[serde(default, deserialize_with = "opt_amount")]
    #[schemars(with = "Option<String>")]
    pub amount: Option<String>,
    /// Category code or name for a single-line transaction
    pub category: Option<String>,
    /// Receipt items (with names, quantities and prices when visible) instead of amount+category
    pub items: Option<Vec<ItemArgs>>,
    /// Discount on the whole receipt, split across items in proportion to their amounts
    #[serde(default, deserialize_with = "opt_amount")]
    #[schemars(with = "Option<String>")]
    pub discount_total: Option<String>,
    /// Account code or name (default: the user's last used account)
    pub account: Option<String>,
    /// Date YYYY-MM-DD, or "today" / "yesterday" (default: today)
    pub date: Option<String>,
    /// Short description, e.g. "Coffee at Starbucks"
    pub description: Option<String>,
    /// Language of `description`: ru | en | th (default: the user's language)
    pub description_lang: Option<String>,
    /// Translations of the description into the other app languages
    pub translations: Option<Translations>,
    /// Expenses only: this is a refund (money returned to the account)
    pub refund: Option<bool>,
}

#[derive(Debug, Default, Deserialize, JsonSchema)]
pub struct AddTransferArgs {
    /// Source account code or name
    pub from_account: String,
    /// Destination account code or name
    pub to_account: String,
    /// Amount taken from the source account, in its currency
    #[serde(deserialize_with = "amount")]
    #[schemars(with = "String")]
    pub amount: String,
    /// Amount received, in the destination currency — required when currencies differ (exchange)
    #[serde(default, deserialize_with = "opt_amount")]
    #[schemars(with = "Option<String>")]
    pub to_amount: Option<String>,
    /// Fee in the source currency, part of `amount`
    #[serde(default, deserialize_with = "opt_amount")]
    #[schemars(with = "Option<String>")]
    pub fee_amount: Option<String>,
    /// Expense category code or name for the fee (required with fee_amount)
    pub fee_category: Option<String>,
    /// Date YYYY-MM-DD, or "today" / "yesterday" (default: today)
    pub date: Option<String>,
    pub description: Option<String>,
    /// Language of `description`: ru | en | th (default: the user's language)
    pub description_lang: Option<String>,
    pub translations: Option<Translations>,
}

#[derive(Debug, Default, Deserialize, JsonSchema)]
pub struct UpdateTransactionArgs {
    /// Transaction id (from list_transactions or add_* results)
    pub id: String,
    /// New date YYYY-MM-DD
    pub date: Option<String>,
    /// New account (source account for transfers)
    pub account: Option<String>,
    /// New amount: for a single-item expense/income or a transfer
    #[serde(default, deserialize_with = "opt_amount")]
    #[schemars(with = "Option<String>")]
    pub amount: Option<String>,
    /// New category for a single-item expense/income
    pub category: Option<String>,
    /// Expense/income items to change (with id) or add (without id); items not listed stay as they are
    pub items: Option<Vec<ItemArgs>>,
    /// Ids of items to delete from this expense/income
    pub remove_item_ids: Option<Vec<String>>,
    /// Discount on the whole receipt added to the items' discounts, split in proportion to their amounts
    #[serde(default, deserialize_with = "opt_amount")]
    #[schemars(with = "Option<String>")]
    pub discount_total: Option<String>,
    /// Transfers: new destination account
    pub to_account: Option<String>,
    /// Transfers: new received amount (exchange)
    #[serde(default, deserialize_with = "opt_amount")]
    #[schemars(with = "Option<String>")]
    pub to_amount: Option<String>,
    /// New description (replaces translations unless new ones are given)
    pub description: Option<String>,
    pub description_lang: Option<String>,
    pub translations: Option<Translations>,
}

#[derive(Debug, Default, Deserialize, JsonSchema)]
pub struct PayPlannedArgs {
    /// Plan id from list_upcoming_payments
    pub plan_id: String,
    /// Scheduled date of the occurrence, YYYY-MM-DD (from list_upcoming_payments)
    pub date: String,
    /// Actual amount if it differs from the planned one
    #[serde(default, deserialize_with = "opt_amount")]
    #[schemars(with = "Option<String>")]
    pub amount: Option<String>,
    /// Account if it differs from the planned one
    pub account: Option<String>,
    /// Date the payment was actually made, YYYY-MM-DD (default: today)
    pub paid_on: Option<String>,
}

#[derive(Debug, Default, Deserialize, JsonSchema)]
pub struct SkipPlannedArgs {
    /// Plan id from list_upcoming_payments
    pub plan_id: String,
    /// Scheduled date of the occurrence to skip, YYYY-MM-DD
    pub date: String,
}

// ---------------------------------------------------------------- справочники, курсы, лимиты, планы

#[derive(Debug, Default, Deserialize, JsonSchema)]
pub struct NameTranslations {
    /// Russian name
    pub ru: Option<String>,
    /// English name
    pub en: Option<String>,
    /// Thai name
    pub th: Option<String>,
}

#[derive(Debug, Default, Deserialize, JsonSchema)]
pub struct AddCurrencyArgs {
    /// New currency code, UPPER_SNAKE — the ISO 4217 code when there is one, e.g. "USD"
    pub code: String,
    /// Name in `name_lang`, e.g. "US dollar"
    pub name: String,
    /// Language of `name`: ru | en | th (default: the user's language)
    pub name_lang: Option<String>,
    /// The name in all app languages (ru, en, th)
    pub translations: Option<NameTranslations>,
    /// Symbol, e.g. "$"
    pub symbol: String,
    /// Digits after the decimal point, 0–8 (default 2)
    pub minor_units: Option<u32>,
}

#[derive(Debug, Default, Deserialize, JsonSchema)]
pub struct UpdateCurrencyArgs {
    /// Currency code (or name) to change
    pub code: String,
    /// New name (pass translations too: old ones are dropped when the name changes)
    pub name: Option<String>,
    /// Language of the name: ru | en | th (default: the user's language for a new name)
    pub name_lang: Option<String>,
    /// Translations to set (with a new name) or to replace one by one (without it); "" removes one
    pub translations: Option<NameTranslations>,
    /// New symbol
    pub symbol: Option<String>,
    /// New number of digits after the decimal point, 0–8
    pub minor_units: Option<u32>,
}

#[derive(Debug, Default, Deserialize, JsonSchema)]
pub struct AddAccountArgs {
    /// New account code, UPPER_SNAKE, e.g. "KBANK" or "CARD_KBANK"
    pub code: String,
    /// Name in `name_lang`, e.g. "K-Bank"
    pub name: String,
    /// Language of `name`: ru | en | th (default: the user's language)
    pub name_lang: Option<String>,
    /// The name in all app languages (ru, en, th)
    pub translations: Option<NameTranslations>,
    /// cash | card | bank | savings
    pub kind: String,
    /// Currency code or name, e.g. "THB"
    pub currency: String,
    /// Opening balance in the account currency (negative for a debt). Needs the transactions:write right too
    #[serde(default, deserialize_with = "opt_amount")]
    #[schemars(with = "Option<String>")]
    pub opening_balance: Option<String>,
    /// Date of the opening balance, YYYY-MM-DD (default: today)
    pub opening_date: Option<String>,
}

#[derive(Debug, Default, Deserialize, JsonSchema)]
pub struct UpdateAccountArgs {
    /// Account code (or name) to change
    pub code: String,
    /// New name (pass translations too: old ones are dropped when the name changes)
    pub name: Option<String>,
    /// Language of the name: ru | en | th (default: the user's language for a new name)
    pub name_lang: Option<String>,
    /// Translations to set (with a new name) or to replace one by one (without it); "" removes one
    pub translations: Option<NameTranslations>,
    /// New kind: cash | card | bank | savings
    pub kind: Option<String>,
    /// New currency code or name (only while the account has no transactions)
    pub currency: Option<String>,
    /// New opening balance in the account currency; 0 removes it. Needs the transactions:write right too
    #[serde(default, deserialize_with = "opt_amount")]
    #[schemars(with = "Option<String>")]
    pub opening_balance: Option<String>,
    /// Date of the opening balance, YYYY-MM-DD (default: today); pass it with opening_balance
    pub opening_date: Option<String>,
}

#[derive(Debug, Default, Deserialize, JsonSchema)]
pub struct AddCategoryArgs {
    /// New category code, UPPER_SNAKE, e.g. "PETS"
    pub code: String,
    /// Name in `name_lang`, e.g. "Pets"
    pub name: String,
    /// Language of `name`: ru | en | th (default: the user's language)
    pub name_lang: Option<String>,
    /// The name in all app languages (ru, en, th)
    pub translations: Option<NameTranslations>,
    /// expense | income (default: the parent's kind; required for a top-level category)
    pub kind: Option<String>,
    /// Parent category code or name (omit for a top-level category)
    pub parent: Option<String>,
    /// Position among siblings, smaller first (default: last)
    pub sort_order: Option<i32>,
}

#[derive(Debug, Default, Deserialize, JsonSchema)]
pub struct UpdateCategoryArgs {
    /// Category code (or name) to change
    pub code: String,
    /// New name (pass translations too: old ones are dropped when the name changes)
    pub name: Option<String>,
    /// Language of the name: ru | en | th (default: the user's language for a new name)
    pub name_lang: Option<String>,
    /// Translations to set (with a new name) or to replace one by one (without it); "" removes one
    pub translations: Option<NameTranslations>,
    /// Move under this parent (code or name); "" moves it to the top level
    pub parent: Option<String>,
    /// New position among siblings, smaller first
    pub sort_order: Option<i32>,
}

#[derive(Debug, Default, Deserialize, JsonSchema)]
pub struct SetRateArgs {
    /// Currency code or name, e.g. "USD" (not the base currency)
    pub currency: String,
    /// How many units of the BASE currency 1 unit of `currency` costs, e.g. 36.5 for 1 USD = 36.5 THB.
    /// Pass exactly one of `rate` and `inverse_rate`
    #[serde(default, deserialize_with = "opt_amount")]
    #[schemars(with = "Option<String>")]
    pub rate: Option<String>,
    /// How many units of `currency` 1 unit of the BASE currency costs, e.g. 2.5 for 1 THB = 2.5 RUB —
    /// the way banks quote weak currencies. Pass exactly one of `rate` and `inverse_rate`
    #[serde(default, deserialize_with = "opt_amount")]
    #[schemars(with = "Option<String>")]
    pub inverse_rate: Option<String>,
    /// Date YYYY-MM-DD (default: today)
    pub date: Option<String>,
}

#[derive(Debug, Default, Deserialize, JsonSchema)]
pub struct ListRatesArgs {
    /// From date YYYY-MM-DD, inclusive (default: no limit)
    pub from: Option<String>,
    /// To date YYYY-MM-DD, inclusive (default: no limit)
    pub to: Option<String>,
    /// Currency code or name: only its rates
    pub currency: Option<String>,
}

#[derive(Debug, Default, Deserialize, JsonSchema)]
pub struct SetLimitArgs {
    /// Category code or name; the limit covers its subcategories too
    pub category: String,
    /// Limit amount in the base currency of the project; 0 is a real zero limit (no spending allowed). Omit when `remove` is true
    #[serde(default, deserialize_with = "opt_amount")]
    #[schemars(with = "Option<String>")]
    pub amount: Option<String>,
    /// week | month | year (default: month; when removing — the period of the rule in force)
    pub period: Option<String>,
    /// Valid from date YYYY-MM-DD (default: the start of the current period — Monday, the 1st or January 1). A rule applies to the whole period containing this date
    pub valid_from: Option<String>,
    /// true: remove the limit from `valid_from` on — no limit after that, earlier periods keep theirs (this adds a rule, nothing is deleted)
    pub remove: Option<bool>,
}

#[derive(Debug, Default, Deserialize, JsonSchema)]
pub struct AddPlanArgs {
    /// Name in `name_lang`, e.g. "Rent"
    pub name: String,
    /// Language of `name`: ru | en | th (default: the user's language)
    pub name_lang: Option<String>,
    /// The name in all app languages (ru, en, th)
    pub translations: Option<NameTranslations>,
    /// expense | income | transfer
    pub kind: String,
    /// Account code or name (the source account for a transfer)
    pub account: String,
    /// Category code or name — for expense and income
    pub category: Option<String>,
    /// Destination account code or name — for a transfer
    pub to_account: Option<String>,
    /// Expected amount in the account currency
    #[serde(deserialize_with = "amount")]
    #[schemars(with = "String")]
    pub amount: String,
    /// once | daily | weekly | monthly | yearly
    pub recurrence: String,
    /// Every N days/weeks/months/years, 1–366 (default 1)
    pub interval: Option<u32>,
    /// First date YYYY-MM-DD (default: today)
    pub start_on: Option<String>,
    /// Last date YYYY-MM-DD (default: no end)
    pub end_on: Option<String>,
}

#[derive(Debug, Default, Deserialize, JsonSchema)]
pub struct UpdatePlanArgs {
    /// Plan id from list_plans or list_upcoming_payments
    pub id: String,
    /// New name (pass translations too: old ones are dropped when the name changes)
    pub name: Option<String>,
    /// Language of the name: ru | en | th (default: the user's language for a new name)
    pub name_lang: Option<String>,
    /// Translations to set (with a new name) or to replace one by one (without it); "" removes one
    pub translations: Option<NameTranslations>,
    /// New account code or name
    pub account: Option<String>,
    /// New category code or name (expense and income plans)
    pub category: Option<String>,
    /// New destination account code or name (transfer plans)
    pub to_account: Option<String>,
    /// New expected amount in the account currency
    #[serde(default, deserialize_with = "opt_amount")]
    #[schemars(with = "Option<String>")]
    pub amount: Option<String>,
    /// once | daily | weekly | monthly | yearly
    pub recurrence: Option<String>,
    /// Every N units of the recurrence, 1–366
    pub interval: Option<u32>,
    /// New first date YYYY-MM-DD
    pub start_on: Option<String>,
    /// New last date YYYY-MM-DD; "" removes the end date
    pub end_on: Option<String>,
    /// false pauses the plan (no more expected payments), true resumes it
    pub active: Option<bool>,
}
