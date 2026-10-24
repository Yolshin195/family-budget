//! Тесты use cases фичи «Люди»: без других фич, на in-memory адаптере и — если задан
//! `TEST_DATABASE_URL` — на PostgreSQL (`on_each_repo!`).

use std::sync::Arc;

use kernel::error::AppError;
use kernel::i18n::Lang;
use kernel::pagination::Pagination;
use kernel::testing::TestDb;
use uuid::Uuid;

use crate::adapters::memory::InMemoryPersonRepository;
use crate::adapters::postgres::PostgresPersonRepository;
use crate::dto::{CreatePersonDto, ListQuery, PersonFields};
use crate::service::PersonService;

/// Хранилища для теста: in-memory и, если задан `TEST_DATABASE_URL`, PostgreSQL (техдолг #10).
async fn backends() -> Vec<(&'static str, PersonService, Option<TestDb>)> {
    let mut out = vec![("memory", PersonService::new(Arc::new(InMemoryPersonRepository::default())), None)];
    if let Some(db) = TestDb::migrated("people", vec![sqlx::migrate!("./migrations")]).await {
        let repo = Arc::new(PostgresPersonRepository::new(db.pool.clone()));
        out.push(("postgres", PersonService::new(repo), Some(db)));
    }
    out
}

/// Тест use case на каждом хранилище из [`backends`]; в теле `svc` — сервис на этом хранилище.
macro_rules! on_each_repo {
    ($(#[$meta:meta])* async fn $name:ident($svc:ident) $body:block) => {
        $(#[$meta])*
        #[tokio::test]
        async fn $name() {
            for (repo, svc, db) in backends().await {
                eprintln!("хранилище: {repo}");
                let $svc = &svc;
                $body
                if let Some(db) = db {
                    db.close().await;
                }
            }
        }
    };
}

fn person(code: &str, name: &str) -> CreatePersonDto {
    CreatePersonDto { code: code.into(), data: PersonFields { name: name.into(), ..Default::default() } }
}

on_each_repo! {
    /// Человек без аккаунта: код — латиница, цифры и `_`; другие имена без повторов; занятый код — конфликт.
    async fn create_rules_and_isolation(svc) {
        let (p, other) = (Uuid::new_v4(), Uuid::new_v4());
        let dto = CreatePersonDto {
            code: " pinpan ".into(),
            data: PersonFields { name: " Pinpan ".into(), aliases: vec!["Пинпан".into(), " ".into(), "ปิ่นปั่น".into(), "PINPAN".into()] },
        };
        let created = svc.create(p, dto).await.unwrap();
        assert_eq!((created.code.as_str(), created.name.as_str(), created.user_id, created.is_member()), ("PINPAN", "Pinpan", None, false));
        assert_eq!(created.aliases, vec!["Пинпан", "ปิ่นปั่น"], "без пустых, повторов и самого имени");
        assert!(kernel::search::fold(&created.search(Lang::Ru)).contains("пинпан"));
        assert!(matches!(svc.create(p, person("PINPAN", "Другая")).await.unwrap_err(), AppError::Conflict(m) if m.starts_with("code:")));
        for bad in ["ПИНПАН", "A/B"] {
            assert!(matches!(svc.create(p, person(bad, "X")).await.unwrap_err(), AppError::Invalid(e) if e.field == "code"), "{bad}");
        }
        assert!(matches!(svc.create(p, person("EMPTY", " ")).await.unwrap_err(), AppError::Invalid(e) if e.field == "name"));
        assert!(matches!(svc.get(other, "PINPAN").await.unwrap_err(), AppError::NotFound(_)));
        svc.update(p, "pinpan", PersonFields { name: "Пинпан".into(), aliases: vec![] }).await.unwrap();
        assert_eq!(svc.get(p, "PINPAN").await.unwrap().name, "Пинпан");
        svc.archive(p, "PINPAN").await.unwrap();
        assert!(svc.active(p, "pinpan").await.unwrap().is_none());
        assert_eq!(svc.list(p, ListQuery { include_archived: true }, Pagination { page: 0, size: 10 }).await.unwrap().content.len(), 1);
        svc.restore(p, "PINPAN").await.unwrap();
        assert!(svc.active(p, "PINPAN").await.unwrap().is_some());
        assert_eq!(svc.purge_project(p).await.unwrap(), 1);
    }
}

on_each_repo! {
    /// Раздел 5.9: запись участника — при вступлении, с кодом из имени; совпадение — `_2`; выход — архив,
    /// повторное вступление — та же запись из архива; вручную запись участника не архивируется.
    async fn member_records(svc) {
        let p = Uuid::new_v4();
        let (anna, anna2, aleksey) = (Uuid::new_v4(), Uuid::new_v4(), Uuid::new_v4());
        svc.create(p, person("ANNA", "Аня")).await.unwrap();
        let a = svc.member_joined(p, anna, "Anna").await.unwrap();
        assert_eq!((a.code.as_str(), a.name.as_str(), a.user_id), ("ANNA_2", "Anna", Some(anna)));
        assert_eq!(svc.member_joined(p, anna2, "Ánna").await.unwrap().code, "ANNA_3");
        assert_eq!(svc.member_joined(p, aleksey, "Алексей").await.unwrap().code, "MEMBER");
        assert_eq!(svc.member_joined(p, anna, "Anna K.").await.unwrap().code, "ANNA_2", "повторный вызов — та же запись");
        // Имя правят в справочнике; вступление его не перетирает.
        svc.update(p, "ANNA_2", PersonFields { name: "Мама".into(), aliases: vec![] }).await.unwrap();
        assert!(matches!(svc.archive(p, "ANNA_2").await.unwrap_err(), AppError::Invalid(e) if e.field == "code"));
        svc.member_left(p, anna).await.unwrap();
        assert!(svc.active(p, "ANNA_2").await.unwrap().is_none());
        assert!(matches!(svc.restore(p, "ANNA_2").await.unwrap_err(), AppError::Invalid(_)), "запись участника из архива вручную не возвращается");
        let back = svc.member_joined(p, anna, "Anna").await.unwrap();
        assert_eq!((back.code.as_str(), back.name.as_str(), back.archived), ("ANNA_2", "Мама", false));
        assert_eq!(svc.of_member(p, aleksey).await.unwrap().map(|x| x.code), Some("MEMBER".to_string()));
    }
}

on_each_repo! {
    /// Удаление аккаунта: во всех проектах запись теряет имя и связь, уходит в архив и показывается
    /// «Удалённый участник» на языке зрителя; её можно назвать заново.
    async fn account_deleted_forgets_member(svc) {
        let (p, q, u) = (Uuid::new_v4(), Uuid::new_v4(), Uuid::new_v4());
        svc.member_joined(p, u, "Anna").await.unwrap();
        svc.member_joined(q, u, "Anna").await.unwrap();
        svc.update(p, "ANNA", PersonFields { name: "Anna".into(), aliases: vec!["Аня".into()] }).await.unwrap();
        svc.account_deleted(u).await.unwrap();
        for project in [p, q] {
            let x = svc.get(project, "ANNA").await.unwrap();
            assert_eq!((x.user_id, x.name.as_str(), x.aliases.len(), x.former_member, x.archived), (None, "", 0, true, true));
            assert_eq!((x.title(Lang::Ru), x.title(Lang::En)), ("Удалённый участник".to_string(), "Former member".to_string()));
        }
        assert!(svc.of_member(p, u).await.unwrap().is_none());
        // Теперь это человек без аккаунта: его можно вернуть из архива и назвать.
        svc.restore(p, "ANNA").await.unwrap();
        svc.update(p, "ANNA", PersonFields { name: "".into(), aliases: vec![] }).await.unwrap();
        assert!(svc.get(p, "ANNA").await.unwrap().former_member, "пустое имя оставляет «Удалённый участник»");
        let named = svc.update(p, "ANNA", PersonFields { name: "Тётя Аня".into(), aliases: vec![] }).await.unwrap();
        assert_eq!((named.title(Lang::En), named.former_member), ("Тётя Аня".to_string(), false));
    }
}

/// Ошибки — на языке запроса (техдолг #9).
#[tokio::test]
async fn errors_are_translated() {
    let svc = PersonService::new(Arc::new(InMemoryPersonRepository::default()));
    let p = Uuid::new_v4();
    svc.create(p, person("A", "A")).await.unwrap();
    let th =
        kernel::i18n::scope(Lang::Th, async { svc.create(p, person("A", "B")).await.unwrap_err().to_string() }).await;
    assert_eq!(th, "code: รหัส “A” ถูกใช้แล้ว");
    let en = kernel::i18n::scope(Lang::En, async { svc.get(p, "NOPE").await.unwrap_err().to_string() }).await;
    assert_eq!(en, "person “NOPE” not found");
}

#[test]
fn templates_are_translated() {
    let root = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let missing =
        kernel::i18n::missing_keys(&[root.join("templates"), root.join("src/adapters")], Some(crate::i18n::dict));
    assert!(missing.is_empty(), "нет перевода для ключей: {missing:?}");
}

/// Техдолг #9: тексты ошибок фичи — только в словаре `i18n.rs`; `http.rs` (OpenAPI для разработчиков) не проверяется.
#[test]
fn no_russian_literals_in_code() {
    let root = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("src");
    for file in ["domain.rs", "service.rs", "dto.rs", "ports.rs", "adapters/web.rs", "adapters/postgres.rs"] {
        let text = std::fs::read_to_string(root.join(file)).unwrap();
        // Тесты домена — в конце файла: в них русские имена — данные.
        let text = text.split("#[cfg(test)]").next().unwrap_or_default();
        for (n, line) in text.lines().enumerate() {
            let line = line.trim_start();
            if line.starts_with("//") || line.starts_with("#[schema") {
                continue;
            }
            let code_part = line.split(" //").next().unwrap_or_default();
            let literals: Vec<&str> = code_part.split('"').skip(1).step_by(2).collect();
            let russian = literals.iter().find(|l| {
                l.chars().any(|c| ('а'..='я').contains(&c.to_ascii_lowercase()) || ('А'..='Я').contains(&c) || c == 'ё')
            });
            assert!(
                russian.is_none(),
                "{file}:{}: строка на русском в коде — перенесите в i18n.rs: {:?}",
                n + 1,
                russian
            );
        }
    }
}
