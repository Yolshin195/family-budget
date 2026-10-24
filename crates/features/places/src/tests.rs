//! Тесты use cases фичи «Места»: без других фич, на in-memory адаптере и — если задан
//! `TEST_DATABASE_URL` — на PostgreSQL (`on_each_repo!`).

use std::sync::Arc;

use kernel::error::{AppError, AppResult};
use kernel::i18n::Lang;
use kernel::pagination::Pagination;
use kernel::testing::TestDb;
use kernel::web::Choice;
use uuid::Uuid;

use crate::adapters::memory::InMemoryPlaceRepository;
use crate::adapters::postgres::PostgresPlaceRepository;
use crate::dto::{CreatePlaceDto, ListQuery, PlaceFields};
use crate::ports::{Deps, Directories};
use crate::service::PlaceService;

/// Категории FOOD (расход), SALARY (доход) и OLD (архивная — её нет среди активных); счета CASH и CARD.
struct FakeDirectories;

#[async_trait::async_trait]
impl Directories for FakeDirectories {
    async fn categories(&self, _p: Uuid, _l: Lang) -> AppResult<Vec<Choice>> {
        let c =
            |code: &str, tag: &str| Choice { code: code.into(), label: code.to_lowercase(), depth: 0, tag: tag.into() };
        Ok(vec![c("FOOD", "EXPENSE"), c("SALARY", "INCOME")])
    }
    async fn accounts(&self, _p: Uuid, _l: Lang) -> AppResult<Vec<Choice>> {
        let a = |code: &str| Choice { code: code.into(), label: code.to_lowercase(), depth: 0, tag: "THB".into() };
        Ok(vec![a("CASH"), a("CARD")])
    }
}

fn deps() -> Deps {
    Deps { directories: Arc::new(FakeDirectories) }
}

/// Хранилища для теста: in-memory и, если задан `TEST_DATABASE_URL`, PostgreSQL (техдолг #10).
async fn backends() -> Vec<(&'static str, PlaceService, Option<TestDb>)> {
    let mut out = vec![("memory", PlaceService::new(Arc::new(InMemoryPlaceRepository::default()), deps()), None)];
    if let Some(db) = TestDb::migrated("places", vec![sqlx::migrate!("./migrations")]).await {
        let repo = Arc::new(PostgresPlaceRepository::new(db.pool.clone()));
        out.push(("postgres", PlaceService::new(repo, deps()), Some(db)));
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

fn fields(name: &str) -> PlaceFields {
    PlaceFields { name: name.into(), ..Default::default() }
}

fn place(code: &str, name: &str) -> CreatePlaceDto {
    CreatePlaceDto { code: code.into(), data: fields(name) }
}

on_each_repo! {
    /// Место с вариантами написания, категорией и счётом; повторы и пустые варианты отбрасываются.
    async fn create_and_get(svc) {
        let p = Uuid::new_v4();
        let dto = CreatePlaceDto {
            code: " seven_eleven ".into(),
            data: PlaceFields {
                name: " 7-Eleven ".into(),
                aliases: vec!["7-11".into(), " ".into(), "เซเว่น".into(), "7-ELEVEN".into(), "7-11".into()],
                default_category_code: Some("food".into()),
                default_account_code: Some("CASH".into()),
                place_kind: "shop".into(),
                location: Some(" Сукхумвит 24 ".into()),
                parent_code: None,
            },
        };
        let created = svc.create(p, dto).await.unwrap();
        assert_eq!((created.code.as_str(), created.name.as_str(), created.place_kind.as_str()), ("SEVEN_ELEVEN", "7-Eleven", "SHOP"));
        assert_eq!(created.aliases, vec!["7-11", "เซเว่น"], "без пустых, повторов и самого названия");
        assert_eq!((created.default_category_code.as_deref(), created.default_account_code.as_deref()), (Some("FOOD"), Some("CASH")));
        assert_eq!(created.title, "7-Eleven · Сукхумвит 24");
        let got = svc.get(p, "seven_eleven").await.unwrap();
        assert_eq!((got.id, got.aliases.clone()), (created.id, created.aliases.clone()));
        assert!(kernel::search::fold(&got.search).contains("เซเว่น"));
    }
}

on_each_repo! {
    /// Код — латиница, цифры и `_`; занятый код — конфликт; название обязательно; ссылки — на активные записи.
    async fn rules(svc) {
        let p = Uuid::new_v4();
        svc.create(p, place("CAFE_AMAZON", "Café Amazon")).await.unwrap();
        assert!(matches!(svc.create(p, place("cafe_amazon", "Другое")).await.unwrap_err(), AppError::Conflict(m) if m.starts_with("code:")));
        for bad in ["КАФЕ", "A/B"] {
            assert!(matches!(svc.create(p, place(bad, "X")).await.unwrap_err(), AppError::Invalid(e) if e.field == "code"), "{bad}");
        }
        assert!(matches!(svc.create(p, place("EMPTY", " ")).await.unwrap_err(), AppError::Invalid(e) if e.field == "name"));
        let bad_ref = |c: Option<&str>, a: Option<&str>| CreatePlaceDto {
            code: "X".into(),
            data: PlaceFields { default_category_code: c.map(Into::into), default_account_code: a.map(Into::into), ..fields("X") },
        };
        assert!(matches!(svc.create(p, bad_ref(Some("OLD"), None)).await.unwrap_err(), AppError::Invalid(e) if e.field == "default_category_code"));
        assert!(matches!(svc.create(p, bad_ref(None, Some("WISE"))).await.unwrap_err(), AppError::Invalid(e) if e.field == "default_account_code"));
        let kind = CreatePlaceDto { code: "K".into(), data: PlaceFields { place_kind: "bar".into(), ..fields("K") } };
        assert!(matches!(svc.create(p, kind).await.unwrap_err(), AppError::Invalid(e) if e.field == "place_kind"));
        let many = CreatePlaceDto { code: "M".into(), data: PlaceFields { aliases: (0..21).map(|i| format!("a{i}")).collect(), ..fields("M") } };
        assert!(matches!(svc.create(p, many).await.unwrap_err(), AppError::Invalid(e) if e.field == "aliases"));
    }
}

on_each_repo! {
    /// Правка заменяет поля целиком; архив скрывает место из списка, «Отменить» возвращает; чужой проект не виден.
    async fn update_archive_and_isolation(svc) {
        let (p, other) = (Uuid::new_v4(), Uuid::new_v4());
        svc.create(p, place("MAKRO", "Makro")).await.unwrap();
        svc.create(p, place("ALPHA", "alpha market")).await.unwrap();
        let changed = svc
            .update(p, "makro", PlaceFields { aliases: vec!["แม็คโคร".into()], place_kind: "market".into(), default_account_code: Some("CARD".into()), ..fields("Makro") })
            .await
            .unwrap();
        assert_eq!((changed.place_kind.as_str(), changed.aliases.clone(), changed.default_account_code.as_deref()), ("MARKET", vec!["แม็คโคร".to_string()], Some("CARD")));
        let names = |list: Vec<crate::dto::PlaceDto>| list.into_iter().map(|x| x.code).collect::<Vec<_>>();
        assert_eq!(names(svc.all(p, false).await.unwrap()), vec!["ALPHA", "MAKRO"], "по названию без учёта регистра");
        svc.archive(p, "MAKRO").await.unwrap();
        assert_eq!(names(svc.all(p, false).await.unwrap()), vec!["ALPHA"]);
        assert!(svc.active(p, "makro").await.unwrap().is_none());
        assert_eq!(svc.list(p, ListQuery { include_archived: true }, Pagination { page: 0, size: 10 }).await.unwrap().content.len(), 2);
        svc.restore(p, "MAKRO").await.unwrap();
        assert!(svc.active(p, "makro").await.unwrap().is_some());
        assert!(matches!(svc.get(other, "MAKRO").await.unwrap_err(), AppError::NotFound(_)));
        assert_eq!(svc.purge_project(p).await.unwrap(), 2);
        assert!(svc.all(p, true).await.unwrap().is_empty());
    }
}

fn child(code: &str, name: &str, parent: &str) -> CreatePlaceDto {
    CreatePlaceDto { code: code.into(), data: PlaceFields { parent_code: Some(parent.into()), ..fields(name) } }
}

on_each_repo! {
    /// Сеть → магазин: магазин идёт сразу за сетью, берёт у неё категорию и счёт, если своих нет;
    /// полное название без повтора имени сети; варианты написания сети находят и магазин.
    async fn tree_inherits_from_chain(svc) {
        let p = Uuid::new_v4();
        let chain = PlaceFields { aliases: vec!["7-11".into()], default_category_code: Some("FOOD".into()), default_account_code: Some("CARD".into()), ..fields("7-Eleven") };
        svc.create(p, CreatePlaceDto { code: "SEVEN".into(), data: chain }).await.unwrap();
        svc.create(p, place("AMAZON", "Café Amazon")).await.unwrap();
        let asok = CreatePlaceDto { code: "SEVEN_ASOK".into(), data: PlaceFields { parent_code: Some("seven".into()), location: Some("Asok".into()), ..fields("7-Eleven") } };
        let shop = svc.create(p, asok).await.unwrap();
        assert_eq!((shop.parent_code.as_deref(), shop.depth, shop.title.as_str()), (Some("SEVEN"), 1, "7-Eleven · Asok"));
        assert_eq!((shop.default_category_code.as_deref(), shop.effective_category_code.as_deref(), shop.effective_account_code.as_deref()), (None, Some("FOOD"), Some("CARD")));
        assert!(kernel::search::fold(&shop.search).contains("7-11"), "{}", shop.search);
        let own = CreatePlaceDto { code: "SEVEN_MALL".into(), data: PlaceFields { default_account_code: Some("CASH".into()), ..child("SEVEN_MALL", "Terminal 21", "SEVEN").data } };
        let mall = svc.create(p, own).await.unwrap();
        assert_eq!((mall.title.as_str(), mall.effective_account_code.as_deref()), ("7-Eleven › Terminal 21", Some("CASH")), "своё значение важнее сети");
        let codes: Vec<String> = svc.all(p, false).await.unwrap().into_iter().map(|x| x.code).collect();
        assert_eq!(codes, vec!["SEVEN", "SEVEN_ASOK", "SEVEN_MALL", "AMAZON"], "магазины — сразу за сетью, соседи по названию");
        assert_eq!(svc.active(p, "seven_asok").await.unwrap().unwrap().effective_category_code.as_deref(), Some("FOOD"));
    }
}

on_each_repo! {
    /// Родитель существует и активен, нет цикла, глубина ≤ 3; сеть с активными магазинами не архивируется,
    /// магазин в архивной сети не возвращается.
    async fn tree_rules(svc) {
        let p = Uuid::new_v4();
        svc.create(p, place("A", "A")).await.unwrap();
        svc.create(p, child("B", "B", "A")).await.unwrap();
        svc.create(p, child("C", "C", "B")).await.unwrap();
        let field = |e: AppError| matches!(e, AppError::Invalid(ref x) if x.field == "parent_code");
        assert!(field(svc.create(p, child("D", "D", "C")).await.unwrap_err()), "четвёртый уровень");
        assert!(field(svc.create(p, child("X", "X", "NOPE")).await.unwrap_err()));
        assert!(field(svc.update(p, "A", PlaceFields { parent_code: Some("C".into()), ..fields("A") }).await.unwrap_err()), "цикл");
        assert!(field(svc.update(p, "A", PlaceFields { parent_code: Some("A".into()), ..fields("A") }).await.unwrap_err()), "сам в себя");
        svc.create(p, place("Z", "Z")).await.unwrap();
        svc.update(p, "B", PlaceFields { parent_code: Some("Z".into()), ..fields("B") }).await.unwrap();
        assert_eq!(svc.get(p, "C").await.unwrap().title, "Z › B › C", "перенос вместе с магазинами");
        svc.create(p, place("ROOT", "Root")).await.unwrap();
        assert!(field(svc.update(p, "Z", PlaceFields { parent_code: Some("ROOT".into()), ..fields("Z") }).await.unwrap_err()), "с поддеревом глубже 3");
        let toast = |e: AppError| matches!(e, AppError::Invalid(ref x) if x.field.is_empty());
        assert!(toast(svc.archive(p, "Z").await.unwrap_err()), "в Z есть активный B");
        svc.archive(p, "C").await.unwrap();
        svc.archive(p, "B").await.unwrap();
        svc.archive(p, "Z").await.unwrap();
        assert!(toast(svc.restore(p, "B").await.unwrap_err()), "сначала сеть");
        assert!(field(svc.create(p, child("Y", "Y", "Z")).await.unwrap_err()), "в архивную сеть нельзя");
        svc.restore(p, "Z").await.unwrap();
        svc.restore(p, "B").await.unwrap();
    }
}

/// Ошибки — на языке запроса (техдолг #9).
#[tokio::test]
async fn errors_are_translated() {
    let svc = PlaceService::new(Arc::new(InMemoryPlaceRepository::default()), deps());
    let p = Uuid::new_v4();
    svc.create(p, place("A", "A")).await.unwrap();
    let th =
        kernel::i18n::scope(Lang::Th, async { svc.create(p, place("A", "B")).await.unwrap_err().to_string() }).await;
    assert_eq!(th, "code: รหัส “A” ถูกใช้แล้ว");
    let en = kernel::i18n::scope(Lang::En, async { svc.get(p, "NOPE").await.unwrap_err().to_string() }).await;
    assert_eq!(en, "place “NOPE” not found");
}

#[test]
fn templates_are_translated() {
    let root = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let missing =
        kernel::i18n::missing_keys(&[root.join("templates"), root.join("src/adapters")], Some(crate::i18n::dict));
    assert!(missing.is_empty(), "нет перевода для ключей: {missing:?}");
    for k in crate::domain::PlaceKind::VALUES {
        assert!(crate::i18n::dict(&format!("kind.{k}")).is_some(), "нет перевода типа {k}");
    }
}

/// Техдолг #9: тексты ошибок фичи — только в словаре `i18n.rs`; `http.rs` (OpenAPI для разработчиков) не проверяется.
#[test]
fn no_russian_literals_in_code() {
    let root = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("src");
    for file in ["domain.rs", "service.rs", "dto.rs", "ports.rs", "adapters/web.rs", "adapters/postgres.rs"] {
        let text = std::fs::read_to_string(root.join(file)).unwrap();
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
