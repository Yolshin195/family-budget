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

#[derive(Debug, Deserialize, JsonSchema)]
pub struct ItemArgs {
    /// Category code or name (expense category for expenses, income category for income)
    pub category: String,
    /// Amount in the account currency, e.g. "120.50"
    #[serde(deserialize_with = "amount")]
    #[schemars(with = "String")]
    pub amount: String,
    /// Optional note for this line
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
    /// Several lines (a receipt split into categories) instead of amount+category
    pub items: Option<Vec<ItemArgs>>,
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
    /// New amount: for a single-line expense/income or a transfer
    #[serde(default, deserialize_with = "opt_amount")]
    #[schemars(with = "Option<String>")]
    pub amount: Option<String>,
    /// New category for a single-line expense/income
    pub category: Option<String>,
    /// Replace all lines of an expense/income
    pub items: Option<Vec<ItemArgs>>,
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
