//! Тесты фичи «Сайт»: воронка без персональных данных, полнота переводов лендинга.

use std::sync::Arc;

use async_trait::async_trait;
use chrono::{DateTime, NaiveDate, Utc};
use kernel::error::AppResult;

use crate::ports::{Deps, FunnelSource};
use crate::service::SiteService;

struct FakeFunnel;

#[async_trait]
impl FunnelSource for FakeFunnel {
    async fn signups(&self, from: DateTime<Utc>, _to: DateTime<Utc>) -> AppResult<i64> {
        Ok(if from.date_naive() == NaiveDate::from_ymd_opt(2026, 10, 13).unwrap() { 3 } else { 0 })
    }
    async fn first_expenses(&self, _from: DateTime<Utc>, _to: DateTime<Utc>) -> AppResult<i64> {
        Ok(1)
    }
}

#[tokio::test]
async fn funnel_counts_visits_signups_and_first_expense() {
    let svc = SiteService::new(Deps { funnel: Arc::new(FakeFunnel) });
    let day = NaiveDate::from_ymd_opt(2026, 10, 13).unwrap();
    for _ in 0..10 {
        svc.visit_on(day);
    }
    svc.visit_on(day.pred_opt().unwrap());
    let f = svc.funnel(day).await.unwrap();
    assert_eq!((f.visits, f.signups, f.first_expenses), (10, 3, 1));
}

#[test]
fn templates_are_translated() {
    let root = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let missing = kernel::i18n::missing_keys(&[root.join("templates"), root.join("src/adapters")], Some(crate::i18n::dict));
    assert!(missing.is_empty(), "нет перевода для ключей: {missing:?}");
}

#[test]
fn legal_pages_have_all_sections_in_every_language() {
    for kind in ["privacy", "terms", "deletion"] {
        for key in ["title", "intro", "h1", "p1", "h2", "p2", "h3", "p3", "h4", "p4"] {
            let (ru, en, th) = crate::i18n::dict(&format!("{kind}.{key}")).unwrap_or_else(|| panic!("{kind}.{key}"));
            assert!(!ru.is_empty() && !en.is_empty() && !th.is_empty(), "{kind}.{key}");
        }
    }
    // Лендинг не обещает «бесплатно», пока не решена монетизация (ТЗ, раздел 11).
    for key in ["hero.title", "hero.subtitle", "meta.description", "final.text"] {
        let (ru, en, th) = crate::i18n::dict(key).unwrap();
        assert!(!ru.contains("бесплат") && !en.to_lowercase().contains("free") && !th.contains("ฟรี"), "{key}");
    }
}
