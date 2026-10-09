//! Тесты use cases «Проекты»: регистрация, вход, семейный проект, роли, сессия.

use std::sync::{Arc, Mutex};

use kernel::auth::{Memberships, Role, Tokens};
use kernel::error::{AppError, AppResult};
use kernel::i18n::Lang;
use uuid::Uuid;

use crate::adapters::memory::InMemoryProjectRepository;
use crate::domain::ProjectKind;
use crate::ports::{Deps, ProjectBootstrap};
use crate::service::ProjectService;

#[derive(Default)]
struct FakeBootstrap(Mutex<Vec<(Uuid, String)>>);

#[async_trait::async_trait]
impl ProjectBootstrap for Arc<FakeBootstrap> {
    async fn init(&self, project_id: Uuid, base_currency: &str, _lang: Lang) -> AppResult<()> {
        self.0.lock().unwrap().push((project_id, base_currency.into()));
        Ok(())
    }
}

fn service() -> (ProjectService, Arc<FakeBootstrap>) {
    let boot = Arc::new(FakeBootstrap::default());
    let svc = ProjectService::new(Arc::new(InMemoryProjectRepository::default()), Deps { bootstrap: Arc::new(boot.clone()) }, Tokens::hs256("test"));
    (svc, boot)
}

#[tokio::test]
async fn register_creates_personal_project_and_login_works() {
    let (svc, boot) = service();
    let id = svc.register("Aleksey@Demo.Local", "Алексей", "secret1", Lang::Ru, "thb").await.unwrap();
    assert_eq!(boot.0.lock().unwrap()[0].1, "THB");
    assert_eq!(svc.login("aleksey@demo.local", "secret1").await.unwrap(), id);
    assert!(matches!(svc.login("aleksey@demo.local", "wrong!!").await.unwrap_err(), AppError::Validation(_)));
    assert!(matches!(svc.register("aleksey@demo.local", "X", "secret1", Lang::Ru, "THB").await.unwrap_err(), AppError::Conflict(_)));
    assert!(matches!(svc.register("b@demo.local", "X", "123", Lang::Ru, "THB").await.unwrap_err(), AppError::Validation(_)));
    let s = svc.session(id, None).await.unwrap().unwrap();
    assert_eq!((s.project_name.as_str(), s.role), ("Личный бюджет", Role::Owner));
}

#[tokio::test]
async fn family_project_members_and_roles() {
    let (svc, _) = service();
    let me = svc.register("me@demo.local", "Me", "secret1", Lang::En, "THB").await.unwrap();
    let anna = svc.register("anna@demo.local", "Anna", "secret1", Lang::Th, "THB").await.unwrap();
    let personal = svc.session(me, None).await.unwrap().unwrap().project_id;
    // в личный проект второго участника не добавить
    assert!(matches!(svc.add_member(me, personal, "anna@demo.local", Role::Editor).await.unwrap_err(), AppError::Validation(_)));
    let family = svc.create_project(me, "Family", ProjectKind::Family, "THB").await.unwrap();
    svc.add_member(me, family, "anna@demo.local", Role::Viewer).await.unwrap();
    // Anna видит семейный проект с ролью VIEWER; чужой личный проект ей не доступен
    let s = svc.session(anna, Some(family)).await.unwrap().unwrap();
    assert_eq!((s.project_id, s.role, s.members.len()), (family, Role::Viewer, 2));
    assert_ne!(svc.session(anna, Some(personal)).await.unwrap().unwrap().project_id, personal);
    // только владелец управляет участниками; последнего владельца не убрать
    assert!(matches!(svc.set_role(anna, family, anna, Role::Owner).await.unwrap_err(), AppError::Forbidden(_)));
    assert!(matches!(svc.set_role(me, family, me, Role::Editor).await.unwrap_err(), AppError::Validation(_)));
    svc.set_role(me, family, anna, Role::Editor).await.unwrap();
    svc.remove_member(anna, family, anna).await.unwrap();
    assert_eq!(svc.overview(me).await.unwrap().iter().find(|p| p.id == family).unwrap().members.len(), 1);
}
