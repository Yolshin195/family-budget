//! Тесты use cases фичи «Категории» на in-memory адаптере: без БД и без других фич.

use std::sync::Arc;

use kernel::error::AppError;
use kernel::pagination::Pagination;
use uuid::Uuid;

use crate::adapters::memory::InMemoryCategoryRepository;
use crate::dto::{CreateCategoryDto, ListQuery, UpdateCategoryDto};
use crate::service::CategoryService;


/// Категория `PINPAN` уже в операциях, остальные — нет.
struct FakeUsage;

#[async_trait::async_trait]
impl crate::ports::CategoryUsage for FakeUsage {
    async fn in_use(&self, _p: Uuid, code: &str) -> kernel::error::AppResult<bool> {
        Ok(code == "PINPAN")
    }
}

fn deps() -> super::ports::Deps {
    super::ports::Deps { usage: Arc::new(FakeUsage) }
}

fn service() -> CategoryService {
    CategoryService::new(Arc::new(InMemoryCategoryRepository::default()), deps())
}

fn create_dto(code: &str) -> CreateCategoryDto {
    CreateCategoryDto {
        code: code.to_string(),
        name: format!("Запись {code}"),
        name_lang: "RU".to_string(),
        name_ru: Some("Пример".to_string()),
        name_en: Some("Пример".to_string()),
        name_th: Some("Пример".to_string()),
        category_kind: "EXPENSE".to_string(),
        parent_code: None,
        sort_order: 1,
    }
}

fn update_dto(name: &str) -> UpdateCategoryDto {
    UpdateCategoryDto {
        name: name.to_string(),
        archived: false,
        name_lang: "RU".to_string(),
        name_ru: Some("Пример".to_string()),
        name_en: Some("Пример".to_string()),
        name_th: Some("Пример".to_string()),
        category_kind: "EXPENSE".to_string(),
        parent_code: None,
        sort_order: 1,
    }
}

fn page(size: i64) -> Pagination {
    Pagination { page: 0, size }
}

#[tokio::test]
async fn create_and_get_normalizes_code() {
    let svc = service();
    let user = Uuid::new_v4();
    let created = svc.create(user, create_dto(" food ")).await.unwrap();
    assert_eq!(created.code, "FOOD");
    assert_eq!(svc.get(user, "food").await.unwrap().id, created.id);
}

#[tokio::test]
async fn duplicate_code_is_conflict() {
    let svc = service();
    let user = Uuid::new_v4();
    svc.create(user, create_dto("A")).await.unwrap();
    assert!(matches!(svc.create(user, create_dto("A")).await.unwrap_err(), AppError::Conflict(_)));
}

#[tokio::test]
async fn empty_name_is_validation_error() {
    let mut dto = create_dto("A");
    dto.name = "   ".into();
    assert!(matches!(service().create(Uuid::new_v4(), dto).await.unwrap_err(), AppError::Validation(_)));
}

#[tokio::test]
async fn invalid_name_lang_is_validation_error() {
    let mut dto = create_dto("A");
    dto.name_lang = "__WRONG__".into();
    assert!(matches!(service().create(Uuid::new_v4(), dto).await.unwrap_err(), AppError::Validation(_)));
}

#[tokio::test]
async fn invalid_category_kind_is_validation_error() {
    let mut dto = create_dto("A");
    dto.category_kind = "__WRONG__".into();
    assert!(matches!(service().create(Uuid::new_v4(), dto).await.unwrap_err(), AppError::Validation(_)));
}

#[tokio::test]
async fn users_are_isolated() {
    let svc = service();
    let (alice, bob) = (Uuid::new_v4(), Uuid::new_v4());
    svc.create(alice, create_dto("A")).await.unwrap();
    assert!(matches!(svc.get(bob, "A").await.unwrap_err(), AppError::NotFound(_)));
    assert!(svc.create(bob, create_dto("A")).await.is_ok());
}

#[tokio::test]
async fn update_changes_name() {
    let svc = service();
    let user = Uuid::new_v4();
    svc.create(user, create_dto("A")).await.unwrap();
    assert_eq!(svc.update(user, "A", update_dto("Новое имя")).await.unwrap().name, "Новое имя");
}

#[tokio::test]
async fn archive_hides_from_list_and_exists() {
    let svc = service();
    let user = Uuid::new_v4();
    svc.create(user, create_dto("A")).await.unwrap();
    assert!(svc.exists(user, "a").await.unwrap());
    svc.archive(user, "A").await.unwrap();
    assert!(!svc.exists(user, "A").await.unwrap());
    assert_eq!(svc.list(user, ListQuery::default(), page(10)).await.unwrap().page.total_elements, 0);
    let all = ListQuery { include_archived: true };
    assert_eq!(svc.list(user, all, page(10)).await.unwrap().page.total_elements, 1);
    assert!(matches!(svc.create(user, create_dto("A")).await.unwrap_err(), AppError::Conflict(_)));
}

#[tokio::test]
async fn list_is_paged_and_sorted() {
    let svc = service();
    let user = Uuid::new_v4();
    for code in ["C", "A", "B"] {
        svc.create(user, create_dto(code)).await.unwrap();
    }
    let result = svc.list(user, ListQuery::default(), page(2)).await.unwrap();
    let codes: Vec<_> = result.content.iter().map(|x| x.code.as_str()).collect();
    assert_eq!(codes, ["A", "B"]);
    assert_eq!((result.page.total_elements, result.page.total_pages), (3, 2));
}

fn child(code: &str, parent: &str, kind: &str) -> CreateCategoryDto {
    CreateCategoryDto { parent_code: Some(parent.to_string()), category_kind: kind.to_string(), ..create_dto(code) }
}

#[tokio::test]
async fn tree_rules() {
    let svc = service();
    let p = Uuid::new_v4();
    svc.create(p, create_dto("FOOD")).await.unwrap();
    svc.create(p, child("CAFE", "food", "EXPENSE")).await.unwrap();
    // родитель не найден
    assert!(matches!(svc.create(p, child("X", "NOPE", "EXPENSE")).await.unwrap_err(), AppError::Validation(_)));
    // тип не совпадает с родителем
    assert!(matches!(svc.create(p, child("SALARY", "FOOD", "INCOME")).await.unwrap_err(), AppError::Validation(_)));
    // цикл: FOOD под CAFE
    let cyc = UpdateCategoryDto { parent_code: Some("CAFE".into()), ..update_dto("Еда") };
    assert!(matches!(svc.update(p, "FOOD", cyc).await.unwrap_err(), AppError::Validation(_)));
    // архив родителя при активном потомке запрещён
    assert!(matches!(svc.archive(p, "FOOD").await.unwrap_err(), AppError::Validation(_)));
    svc.archive(p, "CAFE").await.unwrap();
    svc.archive(p, "FOOD").await.unwrap();
}

#[tokio::test]
async fn tree_depth_limit() {
    let svc = service();
    let p = Uuid::new_v4();
    svc.create(p, create_dto("L1")).await.unwrap();
    for (c, parent) in [("L2", "L1"), ("L3", "L2"), ("L4", "L3"), ("L5", "L4")] {
        svc.create(p, child(c, parent, "EXPENSE")).await.unwrap();
    }
    assert!(matches!(svc.create(p, child("L6", "L5", "EXPENSE")).await.unwrap_err(), AppError::Validation(_)));
    let tree = svc.tree(p, false).await.unwrap();
    assert_eq!(tree.iter().map(|(d, c)| (*d, c.code.as_str())).collect::<Vec<_>>(), vec![(0, "L1"), (1, "L2"), (2, "L3"), (3, "L4"), (4, "L5")]);
}

/// ТЗ, раздел 13: каждый запрос фильтруется по проекту — чужой проект видит 404, а не данные.
#[tokio::test]
async fn foreign_project_gets_not_found() {
    let svc = service();
    let (mine, other) = (Uuid::new_v4(), Uuid::new_v4());
    svc.create(mine, create_dto("FOOD")).await.unwrap();
    assert!(matches!(svc.get(other, "FOOD").await.unwrap_err(), AppError::NotFound(_)));
    assert!(matches!(svc.update(other, "FOOD", update_dto("x")).await.unwrap_err(), AppError::NotFound(_)));
    assert!(matches!(svc.archive(other, "FOOD").await.unwrap_err(), AppError::NotFound(_)));
    assert!(matches!(svc.restore(other, "FOOD").await.unwrap_err(), AppError::NotFound(_)));
    assert!(!svc.exists(other, "FOOD").await.unwrap());
    assert_eq!(svc.list(other, ListQuery::default(), page(10)).await.unwrap().page.total_elements, 0);
}

#[tokio::test]
async fn archive_can_be_undone_and_project_purged() {
    let svc = service();
    let (mine, other) = (Uuid::new_v4(), Uuid::new_v4());
    svc.create(mine, create_dto("FOOD")).await.unwrap();
    svc.create(other, create_dto("FOOD")).await.unwrap();
    svc.archive(mine, "FOOD").await.unwrap();
    assert!(!svc.exists(mine, "FOOD").await.unwrap());
    svc.restore(mine, "FOOD").await.unwrap();
    assert!(svc.exists(mine, "FOOD").await.unwrap(), "«Отменить» возвращает запись");
    assert_eq!(svc.purge_project(mine).await.unwrap(), 1);
    assert!(!svc.exists(mine, "FOOD").await.unwrap());
    assert!(svc.exists(other, "FOOD").await.unwrap(), "чужой проект не задет");
}

#[test]
fn templates_are_translated() {
    let root = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let missing = kernel::i18n::missing_keys(&[root.join("templates"), root.join("src/adapters")], Some(crate::i18n::dict));
    assert!(missing.is_empty(), "нет перевода для ключей: {missing:?}");
}

/// Ц2: у PINPAN есть операции — тип «расход» не меняется на «доход» (старые суммы ушли бы в дерево
/// доходов); у неиспользованной категории тип меняется, как раньше.
#[tokio::test]
async fn kind_change_with_postings_is_rejected() {
    let svc = service();
    let p = Uuid::new_v4();
    svc.create(p, create_dto("PINPAN")).await.unwrap();
    svc.create(p, create_dto("GIFTS")).await.unwrap();
    let income = |name: &str| UpdateCategoryDto { category_kind: "INCOME".into(), ..update_dto(name) };
    let err = svc.update(p, "PINPAN", income("Pinpan")).await.unwrap_err();
    assert!(matches!(err, AppError::Validation(m) if m.starts_with("category_kind")), "тип категории с операциями сменился");
    assert_eq!(svc.update(p, "PINPAN", update_dto("Pinpan")).await.unwrap().name, "Pinpan", "без смены типа правка проходит");
    assert_eq!(svc.update(p, "GIFTS", income("Подарки")).await.unwrap().category_kind, "INCOME");
}
