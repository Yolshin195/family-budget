//! Доменная модель «Планирование»: плановые операции, правила повторения, вхождения.

use chrono::{Datelike, Days, Months, NaiveDate};
use kernel::code::{normalize, require_text};
use kernel::i18n::{Lang, Localized};
use rust_decimal::Decimal;
use uuid::Uuid;

/// Правило повторения.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Recurrence {
    Once,
    Daily,
    Weekly,
    Monthly,
    Yearly,
}

impl Recurrence {
    pub const VALUES: &'static [&'static str] = &["ONCE", "DAILY", "WEEKLY", "MONTHLY", "YEARLY"];

    pub fn as_str(&self) -> &'static str {
        match self {
            Recurrence::Once => "ONCE",
            Recurrence::Daily => "DAILY",
            Recurrence::Weekly => "WEEKLY",
            Recurrence::Monthly => "MONTHLY",
            Recurrence::Yearly => "YEARLY",
        }
    }

    pub fn parse(s: &str) -> Option<Self> {
        match s {
            "ONCE" => Some(Recurrence::Once),
            "DAILY" => Some(Recurrence::Daily),
            "WEEKLY" => Some(Recurrence::Weekly),
            "MONTHLY" => Some(Recurrence::Monthly),
            "YEARLY" => Some(Recurrence::Yearly),
            _ => None,
        }
    }
}

/// Название на трёх языках: оригинал + язык + переводы.
#[derive(Debug, Clone, PartialEq)]
pub struct Name {
    pub text: String,
    pub lang: Lang,
    pub ru: Option<String>,
    pub en: Option<String>,
    pub th: Option<String>,
}

impl Name {
    pub fn localized(&self) -> Localized<'_> {
        Localized { original: &self.text, lang: self.lang, ru: self.ru.as_deref(), en: self.en.as_deref(), th: self.th.as_deref() }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct Plan {
    pub id: Uuid,
    pub project_id: Uuid,
    pub name: Name,
    /// EXPENSE / INCOME / TRANSFER
    pub tx_kind: String,
    pub account_code: String,
    /// Категория расхода или дохода; у перевода — нет.
    pub category_code: Option<String>,
    /// Счёт зачисления перевода.
    pub to_account_code: Option<String>,
    /// Ожидаемая сумма в валюте счёта.
    pub amount: Decimal,
    pub recurrence: Recurrence,
    /// Каждые N единиц правила (N ≥ 1).
    pub interval: u32,
    pub start_on: NaiveDate,
    pub end_on: Option<NaiveDate>,
    pub active: bool,
    pub created_by: Uuid,
}

#[derive(Debug, Clone)]
pub struct PlanData {
    pub name: Name,
    pub tx_kind: String,
    pub account_code: String,
    pub category_code: Option<String>,
    pub to_account_code: Option<String>,
    pub amount: Decimal,
    pub recurrence: Recurrence,
    pub interval: u32,
    pub start_on: NaiveDate,
    pub end_on: Option<NaiveDate>,
    pub active: bool,
}

/// Предел вхождений за один расчёт (защита от DAILY на десятилетия).
pub const MAX_OCCURRENCES: usize = 3700;

impl Plan {
    pub fn create(project_id: Uuid, created_by: Uuid, data: PlanData) -> Result<Self, String> {
        let mut p = Plan {
            id: Uuid::new_v4(),
            project_id,
            name: data.name.clone(),
            tx_kind: String::new(),
            account_code: String::new(),
            category_code: None,
            to_account_code: None,
            amount: Decimal::ZERO,
            recurrence: data.recurrence,
            interval: 1,
            start_on: data.start_on,
            end_on: None,
            active: true,
            created_by,
        };
        p.change(data)?;
        Ok(p)
    }

    pub fn change(&mut self, d: PlanData) -> Result<(), String> {
        if !["EXPENSE", "INCOME", "TRANSFER"].contains(&d.tx_kind.as_str()) {
            return Err(format!("tx_kind: недопустимое значение '{}'", d.tx_kind));
        }
        let account_code = normalize("account_code", &d.account_code)?;
        let present = |v: Option<String>| v.filter(|s| !s.trim().is_empty());
        // Перевод — счёт зачисления вместо категории; расход и доход — категория.
        let (category_code, to_account_code) = if d.tx_kind == "TRANSFER" {
            let to = normalize("to_account_code", &present(d.to_account_code).ok_or("to_account_code: выберите счёт зачисления")?)?;
            if to == account_code {
                return Err("to_account_code: счёт зачисления должен отличаться от счёта списания".into());
            }
            (None, Some(to))
        } else {
            (Some(normalize("category_code", &present(d.category_code).ok_or("category_code: выберите категорию")?)?), None)
        };
        if d.amount <= Decimal::ZERO {
            return Err("amount: сумма должна быть больше нуля".into());
        }
        if !(1..=366).contains(&d.interval) {
            return Err("interval: от 1 до 366".into());
        }
        if d.end_on.is_some_and(|e| e < d.start_on) {
            return Err("end_on: дата окончания раньше даты начала".into());
        }
        let lang = d.name.lang;
        let tr = |l: Lang, v: Option<String>| if l == lang { None } else { kernel::i18n::clean(v) };
        self.name = Name {
            text: require_text("name", &d.name.text)?,
            lang,
            ru: tr(Lang::Ru, d.name.ru),
            en: tr(Lang::En, d.name.en),
            th: tr(Lang::Th, d.name.th),
        };
        self.tx_kind = d.tx_kind;
        self.account_code = account_code;
        self.category_code = category_code;
        self.to_account_code = to_account_code;
        self.amount = d.amount;
        self.recurrence = d.recurrence;
        self.interval = d.interval;
        self.start_on = d.start_on;
        self.end_on = d.end_on;
        self.active = d.active;
        Ok(())
    }

    /// k-я дата по правилу (k = 0 — дата начала). `None` — даты не существует (за пределами календаря).
    fn nth(&self, k: u32) -> Option<NaiveDate> {
        let n = k.checked_mul(self.interval)?;
        match self.recurrence {
            Recurrence::Once => (k == 0).then_some(self.start_on),
            Recurrence::Daily => self.start_on.checked_add_days(Days::new(n as u64)),
            Recurrence::Weekly => self.start_on.checked_add_days(Days::new(7 * n as u64)),
            // chrono сам переносит 31-е на последний день короткого месяца, 29.02 — на 28.02.
            Recurrence::Monthly => self.start_on.checked_add_months(Months::new(n)),
            Recurrence::Yearly => self.start_on.checked_add_months(Months::new(n.checked_mul(12)?)),
        }
    }

    /// Даты вхождений в периоде [from, to] (без учёта `active`).
    pub fn dates(&self, from: NaiveDate, to: NaiveDate) -> Vec<NaiveDate> {
        let to = self.end_on.map_or(to, |e| e.min(to));
        if to < from || to < self.start_on {
            return Vec::new();
        }
        // Пропускаем заведомо ранние шаги, чтобы не перебирать годы ежедневного плана.
        let mut k: u32 = match self.recurrence {
            Recurrence::Daily if from > self.start_on => ((from - self.start_on).num_days() as u32) / self.interval,
            Recurrence::Weekly if from > self.start_on => ((from - self.start_on).num_days() as u32) / (7 * self.interval),
            Recurrence::Monthly if from > self.start_on => {
                let months = (from.year() - self.start_on.year()) * 12 + from.month() as i32 - self.start_on.month() as i32;
                (months.max(1) as u32 - 1) / self.interval
            }
            _ => 0,
        };
        let mut out = Vec::new();
        while let Some(d) = self.nth(k) {
            if d > to || out.len() >= MAX_OCCURRENCES {
                break;
            }
            if d >= from {
                out.push(d);
            }
            if self.recurrence == Recurrence::Once {
                break;
            }
            k += 1;
        }
        out
    }

    /// `date` — одна из дат правила.
    pub fn occurs_on(&self, date: NaiveDate) -> bool {
        self.dates(date, date).first() == Some(&date)
    }
}

/// Статус вхождения.
#[derive(Debug, Clone, PartialEq)]
pub enum Status {
    /// Ожидается (дата не прошла).
    Expected,
    /// Дата прошла, решения нет.
    Overdue,
    /// Оплачено транзакцией.
    Paid { transaction_id: Uuid, amount: Decimal, paid_on: NaiveDate },
    /// Пропущено.
    Skipped,
}

impl Status {
    pub fn code(&self) -> &'static str {
        match self {
            Status::Expected => "EXPECTED",
            Status::Overdue => "OVERDUE",
            Status::Paid { .. } => "PAID",
            Status::Skipped => "SKIPPED",
        }
    }
}

/// Статус вхождения по фактам: оплата → PAID, пропуск → SKIPPED, иначе по дате.
pub fn status(date: NaiveDate, today: NaiveDate, paid: Option<(Uuid, Decimal, NaiveDate)>, skipped: bool) -> Status {
    match (paid, skipped) {
        (Some((transaction_id, amount, paid_on)), _) => Status::Paid { transaction_id, amount, paid_on },
        (None, true) => Status::Skipped,
        (None, false) if date < today => Status::Overdue,
        _ => Status::Expected,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn d(y: i32, m: u32, day: u32) -> NaiveDate {
        NaiveDate::from_ymd_opt(y, m, day).unwrap()
    }

    fn plan(r: Recurrence, interval: u32, start: NaiveDate) -> Plan {
        Plan::create(
            Uuid::nil(),
            Uuid::nil(),
            PlanData {
                name: Name { text: "Аренда".into(), lang: Lang::Ru, ru: Some("x".into()), en: Some("Rent".into()), th: None },
                tx_kind: "EXPENSE".into(),
                account_code: "card".into(),
                category_code: Some("rent".into()),
                to_account_code: None,
                amount: Decimal::from(15000),
                recurrence: r,
                interval,
                start_on: start,
                end_on: None,
                active: true,
            },
        )
        .unwrap()
    }

    #[test]
    fn monthly_on_31st_falls_back_to_last_day() {
        let p = plan(Recurrence::Monthly, 1, d(2026, 1, 31));
        assert_eq!(p.dates(d(2026, 1, 1), d(2026, 4, 30)), vec![d(2026, 1, 31), d(2026, 2, 28), d(2026, 3, 31), d(2026, 4, 30)]);
        assert_eq!(p.name.ru, None, "перевод на язык оригинала не хранится");
    }

    #[test]
    fn monthly_window_in_the_middle() {
        let p = plan(Recurrence::Monthly, 1, d(2025, 3, 5));
        assert_eq!(p.dates(d(2026, 10, 1), d(2026, 10, 31)), vec![d(2026, 10, 5)]);
        let q = plan(Recurrence::Monthly, 3, d(2026, 1, 10));
        assert_eq!(q.dates(d(2026, 1, 1), d(2026, 12, 31)), vec![d(2026, 1, 10), d(2026, 4, 10), d(2026, 7, 10), d(2026, 10, 10)]);
    }

    #[test]
    fn weekly_every_second_week() {
        let p = plan(Recurrence::Weekly, 2, d(2026, 10, 2));
        assert_eq!(p.dates(d(2026, 10, 1), d(2026, 10, 31)), vec![d(2026, 10, 2), d(2026, 10, 16), d(2026, 10, 30)]);
        assert!(p.occurs_on(d(2026, 10, 16)));
        assert!(!p.occurs_on(d(2026, 10, 9)));
    }

    #[test]
    fn once_daily_yearly_and_end() {
        assert_eq!(plan(Recurrence::Once, 1, d(2026, 11, 12)).dates(d(2026, 1, 1), d(2027, 12, 31)), vec![d(2026, 11, 12)]);
        let mut p = plan(Recurrence::Daily, 1, d(2026, 10, 1));
        p.end_on = Some(d(2026, 10, 3));
        assert_eq!(p.dates(d(2026, 9, 1), d(2026, 12, 1)).len(), 3);
        let y = plan(Recurrence::Yearly, 1, d(2024, 2, 29));
        assert_eq!(y.dates(d(2025, 1, 1), d(2025, 12, 31)), vec![d(2025, 2, 28)]);
    }

    #[test]
    fn statuses() {
        let today = d(2026, 10, 9);
        assert_eq!(status(d(2026, 10, 5), today, None, false), Status::Overdue);
        assert_eq!(status(d(2026, 10, 15), today, None, false), Status::Expected);
        assert_eq!(status(d(2026, 10, 5), today, None, true), Status::Skipped);
        assert_eq!(status(d(2026, 10, 5), today, Some((Uuid::nil(), Decimal::ONE, today)), true).code(), "PAID");
    }

    #[test]
    fn validation() {
        let mut data = PlanData {
            name: Name { text: " ".into(), lang: Lang::Ru, ru: None, en: None, th: None },
            tx_kind: "EXPENSE".into(),
            account_code: "A".into(),
            category_code: Some("C".into()),
            to_account_code: None,
            amount: Decimal::ONE,
            recurrence: Recurrence::Monthly,
            interval: 1,
            start_on: d(2026, 1, 1),
            end_on: None,
            active: true,
        };
        assert!(Plan::create(Uuid::nil(), Uuid::nil(), data.clone()).is_err());
        data.name.text = "X".into();
        data.amount = Decimal::ZERO;
        assert!(Plan::create(Uuid::nil(), Uuid::nil(), data.clone()).is_err());
        data.amount = Decimal::ONE;
        data.end_on = Some(d(2025, 1, 1));
        assert!(Plan::create(Uuid::nil(), Uuid::nil(), data).is_err());
    }
}
