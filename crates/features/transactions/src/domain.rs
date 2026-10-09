//! Доменная модель «Транзакции»: шапка + позиции. Никаких sqlx/axum/serde.

use chrono::NaiveDate;
use kernel::code::normalize;
use kernel::i18n::{Lang, Localized};
use rust_decimal::Decimal;
use uuid::Uuid;

/// Тип операции; знак суммы определяется типом, суммы позиций всегда положительные.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TxKind {
    Expense,
    Income,
}

impl TxKind {
    pub const VALUES: &'static [&'static str] = &["EXPENSE", "INCOME"];

    pub fn as_str(&self) -> &'static str {
        match self {
            TxKind::Expense => "EXPENSE",
            TxKind::Income => "INCOME",
        }
    }

    pub fn parse(s: &str) -> Option<Self> {
        match s {
            "EXPENSE" => Some(TxKind::Expense),
            "INCOME" => Some(TxKind::Income),
            _ => None,
        }
    }
}

/// Позиция транзакции: своя категория и сумма.
#[derive(Debug, Clone, PartialEq)]
pub struct Line {
    pub category_code: String,
    pub amount: Decimal,
    pub note: Option<String>,
}

/// Описание: оригинал + язык + переводы.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct Description {
    pub text: Option<String>,
    pub lang: Lang,
    pub ru: Option<String>,
    pub en: Option<String>,
    pub th: Option<String>,
}

impl Description {
    pub fn localized(&self) -> Option<Localized<'_>> {
        self.text.as_deref().map(|original| Localized {
            original,
            lang: self.lang,
            ru: self.ru.as_deref(),
            en: self.en.as_deref(),
            th: self.th.as_deref(),
        })
    }

    fn cleaned(self) -> Self {
        let lang = self.lang;
        // Перевод на язык оригинала не храним — он и есть оригинал.
        let tr = |l: Lang, v: Option<String>| if l == lang { None } else { kernel::i18n::clean(v) };
        Self {
            text: kernel::i18n::clean(self.text),
            lang,
            ru: tr(Lang::Ru, self.ru),
            en: tr(Lang::En, self.en),
            th: tr(Lang::Th, self.th),
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct Transaction {
    pub id: Uuid,
    pub project_id: Uuid,
    pub occurred_on: NaiveDate,
    pub account_code: String,
    pub tx_kind: TxKind,
    pub description: Description,
    pub created_by: Uuid,
    /// Вхождение плана, которое оплачено этой транзакцией.
    pub plan: Option<(Uuid, NaiveDate)>,
    pub lines: Vec<Line>,
}

/// Данные для создания и изменения.
#[derive(Debug, Clone)]
pub struct TransactionData {
    pub occurred_on: NaiveDate,
    pub account_code: String,
    pub tx_kind: TxKind,
    pub description: Description,
    pub plan: Option<(Uuid, NaiveDate)>,
    pub lines: Vec<Line>,
}

/// Позиций в одной транзакции — не больше (защита от мусорных запросов).
pub const MAX_LINES: usize = 100;

impl Transaction {
    pub fn create(project_id: Uuid, created_by: Uuid, data: TransactionData) -> Result<Self, String> {
        let mut t = Self {
            id: Uuid::new_v4(),
            project_id,
            occurred_on: data.occurred_on,
            account_code: String::new(),
            tx_kind: data.tx_kind,
            description: Description::default(),
            created_by,
            plan: None,
            lines: Vec::new(),
        };
        t.change(data)?;
        Ok(t)
    }

    /// Полная замена данных: набор позиций заменяется целиком.
    pub fn change(&mut self, data: TransactionData) -> Result<(), String> {
        if data.lines.is_empty() {
            return Err("lines: нужна хотя бы одна позиция".into());
        }
        if data.lines.len() > MAX_LINES {
            return Err(format!("lines: не больше {MAX_LINES} позиций"));
        }
        let mut lines = Vec::with_capacity(data.lines.len());
        for (i, l) in data.lines.into_iter().enumerate() {
            if l.amount <= Decimal::ZERO {
                return Err(format!("lines[{}].amount: сумма должна быть больше нуля", i + 1));
            }
            if l.amount.scale() > 4 {
                return Err(format!("lines[{}].amount: не больше 4 знаков после запятой", i + 1));
            }
            lines.push(Line {
                category_code: normalize(&format!("lines[{}].category_code", i + 1), &l.category_code)?,
                amount: l.amount,
                note: kernel::i18n::clean(l.note),
            });
        }
        self.occurred_on = data.occurred_on;
        self.account_code = normalize("account_code", &data.account_code)?;
        self.tx_kind = data.tx_kind;
        self.description = data.description.cleaned();
        self.plan = data.plan;
        self.lines = lines;
        Ok(())
    }

    /// Сумма транзакции = сумма позиций (всегда положительная).
    pub fn total(&self) -> Decimal {
        self.lines.iter().map(|l| l.amount).sum()
    }

    /// Изменить только переводы описания.
    pub fn translate(&mut self, ru: Option<String>, en: Option<String>, th: Option<String>) {
        let d = Description { text: self.description.text.clone(), lang: self.description.lang, ru, en, th };
        self.description = d.cleaned();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::str::FromStr;

    fn data(lines: Vec<(&str, &str)>) -> TransactionData {
        TransactionData {
            occurred_on: NaiveDate::from_ymd_opt(2026, 10, 1).unwrap(),
            account_code: "card".into(),
            tx_kind: TxKind::Expense,
            description: Description { text: Some(" Чек ".into()), lang: Lang::Ru, ru: Some("Чек".into()), en: Some("Receipt".into()), th: Some(" ".into()) },
            plan: None,
            lines: lines.into_iter().map(|(c, a)| Line { category_code: c.into(), amount: Decimal::from_str(a).unwrap(), note: None }).collect(),
        }
    }

    #[test]
    fn total_is_sum_of_lines_and_description_is_cleaned() {
        let t = Transaction::create(Uuid::nil(), Uuid::nil(), data(vec![("food", "900"), ("home", "250"), ("cafe", "100")])).unwrap();
        assert_eq!(t.total(), Decimal::from(1250));
        assert_eq!(t.account_code, "CARD");
        assert_eq!(t.lines[0].category_code, "FOOD");
        assert_eq!(t.description.text.as_deref(), Some("Чек"));
        assert_eq!(t.description.ru, None, "перевод на язык оригинала не хранится");
        assert_eq!(t.description.th, None);
    }

    #[test]
    fn invalid_lines() {
        assert!(Transaction::create(Uuid::nil(), Uuid::nil(), data(vec![])).is_err());
        assert!(Transaction::create(Uuid::nil(), Uuid::nil(), data(vec![("food", "0")])).is_err());
        assert!(Transaction::create(Uuid::nil(), Uuid::nil(), data(vec![("food", "-5")])).is_err());
        assert!(Transaction::create(Uuid::nil(), Uuid::nil(), data(vec![("food", "1.00001")])).is_err());
    }
}
