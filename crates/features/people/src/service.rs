//! Use cases фичи «Люди». Знает только домен и порты — ничего об HTTP и SQL.
//! Люди не удаляются, а архивируются: на них ссылаются позиции операций. Записи участников проекта
//! ведёт фича `projects` (вступление, выход, удаление аккаунта) — вручную их не архивируют.

use std::sync::Arc;

use kernel::error::{AppError, AppResult, Problem};
use kernel::i18n::tr;
use kernel::pagination::{PagedResponse, Pagination};
use uuid::Uuid;

use crate::domain::{Person, member_code};
use crate::dto::{CreatePersonDto, ListQuery, PersonDto, PersonFields};
use crate::ports::PersonRepository;

/// Сколько раз подбирать код участнику, если его заняли параллельно.
const CODE_ATTEMPTS: usize = 5;

pub struct PersonService {
    repo: Arc<dyn PersonRepository>,
}

fn t(key: &str, args: &[(&str, &dyn std::fmt::Display)]) -> String {
    tr(Some(crate::i18n::dict), key, args)
}

impl PersonService {
    pub fn new(repo: Arc<dyn PersonRepository>) -> Self {
        Self { repo }
    }

    /// Человек без аккаунта — только явно: сервер не создаёт людей из свободного текста.
    pub async fn create(&self, project_id: Uuid, dto: CreatePersonDto) -> AppResult<PersonDto> {
        let item = Person::create(project_id, &dto.code, dto.data.into_domain())?;
        if self.repo.find_by_code(project_id, &item.code).await?.is_some() {
            return Err(taken(&item.code));
        }
        self.repo.insert(&item).await?;
        Ok(item.into())
    }

    pub async fn get(&self, project_id: Uuid, code: &str) -> AppResult<PersonDto> {
        Ok(self.load(project_id, code).await?.into())
    }

    pub async fn list(
        &self,
        project_id: Uuid,
        query: ListQuery,
        pagination: Pagination,
    ) -> AppResult<PagedResponse<PersonDto>> {
        let all = query.include_archived;
        let items = self.repo.list(project_id, all, pagination.limit(), pagination.offset()).await?;
        let total = self.repo.count(project_id, all).await?;
        Ok(PagedResponse::new(items.into_iter().map(Into::into).collect(), &pagination, total))
    }

    /// Все люди проекта по имени — для выбора в операциях, ленты и MCP (их немного).
    pub async fn all(&self, project_id: Uuid, include_archived: bool) -> AppResult<Vec<PersonDto>> {
        Ok(self.repo.list(project_id, include_archived, 10_000, 0).await?.into_iter().map(Into::into).collect())
    }

    /// Имя и другие имена; код, участник и архив не меняются.
    pub async fn update(&self, project_id: Uuid, code: &str, dto: PersonFields) -> AppResult<PersonDto> {
        let mut item = self.load(project_id, code).await?;
        item.change(dto.into_domain())?;
        self.repo.update(&item).await?;
        Ok(item.into())
    }

    /// Архив вместо удаления: человек пропадает из выбора, операции его сохраняют. Запись участника
    /// уходит в архив сама, когда он выходит из проекта.
    pub async fn archive(&self, project_id: Uuid, code: &str) -> AppResult<()> {
        self.set_archived(project_id, code, true).await
    }

    /// Вернуть из архива («Отменить» в тосте).
    pub async fn restore(&self, project_id: Uuid, code: &str) -> AppResult<()> {
        self.set_archived(project_id, code, false).await
    }

    async fn set_archived(&self, project_id: Uuid, code: &str, archived: bool) -> AppResult<()> {
        let mut item = self.load(project_id, code).await?;
        if item.user_id.is_some() {
            return Err(Problem::validation("code", t("err.member_archive", &[])).into());
        }
        item.archived = archived;
        self.repo.update(&item).await
    }

    /// Удалить все записи проекта (удаление проекта или аккаунта).
    pub async fn purge_project(&self, project_id: Uuid) -> AppResult<u64> {
        self.repo.purge(project_id).await
    }

    // ---- Публичный контракт для других фич ----

    /// Активный человек с таким кодом.
    pub async fn active(&self, project_id: Uuid, code: &str) -> AppResult<Option<PersonDto>> {
        let Ok(code) = kernel::code::normalize("code", code) else { return Ok(None) };
        Ok(self.repo.find_by_code(project_id, &code).await?.filter(|p| !p.archived).map(Into::into))
    }

    /// Запись пользователя в проекте — в выборе она «Для себя».
    pub async fn of_member(&self, project_id: Uuid, user_id: Uuid) -> AppResult<Option<PersonDto>> {
        Ok(self.repo.find_by_user(project_id, user_id).await?.map(Into::into))
    }

    /// Участник вступил в проект (создал его, принял приглашение) или уже в нём состоит (запуск
    /// приложения): запись с именем из профиля; повторное вступление возвращает прежнюю из архива.
    /// Имя у существующей записи не меняется — его правят в справочнике.
    pub async fn member_joined(&self, project_id: Uuid, user_id: Uuid, name: &str) -> AppResult<PersonDto> {
        for _ in 0..CODE_ATTEMPTS {
            if let Some(mut p) = self.repo.find_by_user(project_id, user_id).await? {
                if p.archived {
                    p.archived = false;
                    self.repo.update(&p).await?;
                }
                return Ok(p.into());
            }
            let code = member_code(name, &self.repo.codes(project_id).await?);
            let p = Person::member(project_id, code, user_id, name);
            match self.repo.insert(&p).await {
                Ok(()) => return Ok(p.into()),
                // Код или запись участника появились параллельно — смотрим заново.
                Err(AppError::Conflict(_)) => continue,
                Err(e) => return Err(e),
            }
        }
        Err(AppError::Conflict(t("err.code_taken", &[("code", &member_code(name, &[]))])))
    }

    /// Участник вышел из проекта или его удалили: запись — в архив, операции её сохраняют.
    pub async fn member_left(&self, project_id: Uuid, user_id: Uuid) -> AppResult<()> {
        if let Some(mut p) = self.repo.find_by_user(project_id, user_id).await?.filter(|p| !p.archived) {
            p.archived = true;
            self.repo.update(&p).await?;
        }
        Ok(())
    }

    /// Пользователь удалил аккаунт: во всех проектах его запись теряет имя и связь с ним и уходит в архив.
    pub async fn account_deleted(&self, user_id: Uuid) -> AppResult<()> {
        for mut p in self.repo.of_user(user_id).await? {
            p.forget_member();
            self.repo.update(&p).await?;
        }
        Ok(())
    }

    async fn load(&self, project_id: Uuid, code: &str) -> AppResult<Person> {
        let code = kernel::code::code("code", code)?;
        self.repo
            .find_by_code(project_id, &code)
            .await?
            .ok_or_else(|| AppError::NotFound(t("err.not_found", &[("code", &code)])))
    }
}

fn taken(code: &str) -> AppError {
    AppError::Conflict(Problem::validation("code", t("err.code_taken", &[("code", &code)])).to_string())
}
