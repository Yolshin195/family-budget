//! Use cases фичи «Сайт»: счётчик визитов лендинга и воронка конверсии.

use std::collections::BTreeMap;
use std::sync::Mutex;

use chrono::{NaiveDate, TimeZone, Utc};
use kernel::error::AppResult;

use crate::domain::Funnel;
use crate::ports::Deps;

pub struct SiteService {
    deps: Deps,
    /// Визиты лендинга по дням (UTC) — только числа, без IP и cookie.
    visits: Mutex<BTreeMap<NaiveDate, u64>>,
}

impl SiteService {
    pub fn new(deps: Deps) -> Self {
        Self { deps, visits: Mutex::new(BTreeMap::new()) }
    }

    pub fn record_visit(&self) {
        self.record_visit_on(Utc::now().date_naive());
    }

    fn record_visit_on(&self, day: NaiveDate) {
        if let Ok(mut v) = self.visits.lock() {
            *v.entry(day).or_default() += 1;
            // Держим только последние 60 дней.
            while v.len() > 60 {
                let first = *v.keys().next().unwrap_or(&day);
                v.remove(&first);
            }
        }
    }

    /// Воронка за день: визиты (с запуска процесса), регистрации и первые расходы (из БД).
    pub async fn funnel(&self, day: NaiveDate) -> AppResult<Funnel> {
        let from = Utc.from_utc_datetime(&day.and_hms_opt(0, 0, 0).unwrap_or_default());
        let to = from + chrono::Duration::days(1);
        let visits = self.visits.lock().map(|v| v.get(&day).copied().unwrap_or(0)).unwrap_or(0);
        Ok(Funnel {
            visits,
            signups: self.deps.funnel.signups(from, to).await?,
            first_expenses: self.deps.funnel.first_expenses(from, to).await?,
        })
    }
}

#[cfg(test)]
impl SiteService {
    pub fn visit_on(&self, day: NaiveDate) {
        self.record_visit_on(day);
    }
}
