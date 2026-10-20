//! Сборка всего приложения на живом PostgreSQL (техдолг #10): миграции всех фич в порядке регистрации,
//! связки wiring и демо-данные через настоящие SQL-адаптеры. Без `TEST_DATABASE_URL` тест пропускается.

use std::sync::Arc;

use kernel::config::Config;
use kernel::context::AppContext;
use kernel::testing::TestDb;

use crate::wiring;

/// Чистая база → все миграции → демо-данные → повторный старт (миграции не повторяются, демо не
/// дублируется) → сброс демо-семьи.
#[tokio::test]
async fn app_starts_on_clean_database() {
    let Some(db) = TestDb::new("app").await else { return };
    let ctx = AppContext { pool: db.pool.clone(), config: Arc::new(Config::test()) };
    for start in 1..=2 {
        let wired = wiring::features(&ctx);
        let feats = kernel::feature::assemble(&ctx, wired.features).await.unwrap_or_else(|e| panic!("старт {start}: {e}"));
        assert!(feats.names.contains(&"transactions"));
        #[cfg(feature = "seed")]
        {
            let seed = wired.seed;
            crate::seed::run(&seed, true, true).await.unwrap_or_else(|e| panic!("демо-данные, старт {start}: {e}"));
            if start == 2 {
                crate::seed::reset_family(&seed).await.expect("сброс демо-семьи");
            }
        }
    }
    let (projects,): (i64,) = sqlx::query_as("SELECT count(*) FROM fb_projects").fetch_one(&db.pool).await.unwrap();
    // Личные проекты Алексея и Anna и одна семья — повторный старт их не дублирует.
    assert_eq!(projects, 3);
    // Двойная запись: остаток каждой валюты в каждой операции сходится (триггер это держит, проверяем итог).
    let (unbalanced,): (i64,) = sqlx::query_as(
        "SELECT count(*) FROM (SELECT transaction_id FROM fb_postings GROUP BY transaction_id, currency_code HAVING sum(amount) <> 0) x",
    )
    .fetch_one(&db.pool)
    .await
    .unwrap();
    assert_eq!(unbalanced, 0);
    db.close().await;
}
