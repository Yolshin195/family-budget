//! Тесты на живом PostgreSQL (техдолг #10). База — из `TEST_DATABASE_URL`: без неё тесты на БД пропускаются,
//! и `cargo test` по-прежнему работает без БД. В CI база есть всегда (`ci.yml`, джоб Tests).
//!
//! Каждый тест — в своей временной схеме: тесты идут параллельно и не видят данных друг друга.

use sqlx::migrate::Migrator;
use sqlx::postgres::{PgConnectOptions, PgPoolOptions};
use sqlx::{Connection, PgConnection, PgPool};

/// Временная схема с пулом подключений к ней. После теста — [`TestDb::close`].
pub struct TestDb {
    pub pool: PgPool,
    url: String,
    schema: String,
}

impl TestDb {
    /// Пустая схема. `None` — `TEST_DATABASE_URL` не задан, тест пропускается.
    pub async fn new(name: &str) -> Option<Self> {
        let Ok(url) = std::env::var("TEST_DATABASE_URL") else {
            eprintln!("TEST_DATABASE_URL не задан — тест на PostgreSQL пропущен");
            return None;
        };
        let schema = format!("test_{name}_{}", uuid::Uuid::new_v4().simple());
        let mut admin = PgConnection::connect(&url).await.expect("TEST_DATABASE_URL: нет подключения к PostgreSQL");
        sqlx::raw_sql(&format!("CREATE SCHEMA {schema}")).execute(&mut admin).await.expect("CREATE SCHEMA");
        admin.close().await.ok();
        let opts: PgConnectOptions = url.parse().expect("TEST_DATABASE_URL");
        // Два подключения на тест: десятки тестов идут параллельно, а у PostgreSQL по умолчанию их 100.
        let pool = PgPoolOptions::new()
            .max_connections(2)
            .connect_with(opts.options([("search_path", schema.as_str())]))
            .await
            .expect("подключение к временной схеме");
        eprintln!("PostgreSQL: схема {schema}");
        Some(Self { pool, url, schema })
    }

    /// Схема с миграциями — как при старте приложения: в порядке списка, чужие версии в общей
    /// `_sqlx_migrations` не мешают.
    pub async fn migrated(name: &str, migrators: Vec<Migrator>) -> Option<Self> {
        let db = Self::new(name).await?;
        for mut m in migrators {
            m.set_ignore_missing(true);
            // Схема своя у каждого теста — общая блокировка миграций базы не нужна.
            m.set_locking(false);
            m.run(&db.pool).await.expect("миграции");
        }
        Some(db)
    }

    /// Удалить схему. Упавший тест её оставляет — имя есть в выводе теста.
    pub async fn close(self) {
        self.pool.close().await;
        let mut admin = PgConnection::connect(&self.url).await.expect("TEST_DATABASE_URL");
        sqlx::raw_sql(&format!("DROP SCHEMA {} CASCADE", self.schema)).execute(&mut admin).await.expect("DROP SCHEMA");
        admin.close().await.ok();
    }
}
