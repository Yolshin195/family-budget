//! Контракт HTTP API фичи «Люди» и преобразования DTO ⇄ домен.

use kernel::i18n::Lang;
use serde::{Deserialize, Serialize};
use utoipa::{IntoParams, ToSchema};
use uuid::Uuid;

use crate::domain::{Person, PersonData};

#[derive(Debug, Default, Deserialize, ToSchema)]
pub struct CreatePersonDto {
    /// Уникальный код: латиница, цифры и `_`, до 40 знаков
    #[schema(example = "PINPAN")]
    pub code: String,
    #[serde(flatten)]
    pub data: PersonFields,
}

/// Поля человека, кроме кода. В `PUT` передаются все: пропущенное поле очищается.
#[derive(Debug, Default, Deserialize, ToSchema)]
pub struct PersonFields {
    /// Отображаемое имя; не переводится
    #[schema(example = "Pinpan")]
    pub name: String,
    /// Другие имена для поиска, на любом языке
    #[serde(default)]
    pub aliases: Vec<String>,
}

#[derive(Debug, Clone, Serialize, ToSchema)]
pub struct PersonDto {
    pub id: Uuid,
    pub code: String,
    /// Имя; пусто — у удалённого участника (`former_member`)
    pub name: String,
    pub aliases: Vec<String>,
    /// Участник проекта; пусто — человек без аккаунта
    pub user_id: Option<Uuid>,
    /// Участник удалил аккаунт: показывается «Удалённый участник»
    pub former_member: bool,
    pub archived: bool,
}

#[derive(Debug, Default, Deserialize, IntoParams)]
pub struct ListQuery {
    /// Показать и архивные записи
    #[serde(default)]
    pub include_archived: bool,
}

impl From<Person> for PersonDto {
    fn from(x: Person) -> Self {
        Self { id: x.id, code: x.code, name: x.name, aliases: x.aliases, user_id: x.user_id, former_member: x.former_member, archived: x.archived }
    }
}

impl PersonFields {
    pub fn into_domain(self) -> PersonData {
        PersonData { name: self.name, aliases: self.aliases }
    }
}

impl PersonDto {
    /// Имя на языке `lang`: у удалённого участника — «Удалённый участник».
    pub fn title(&self, lang: Lang) -> String {
        if self.former_member && self.name.is_empty() {
            return kernel::i18n::t(lang, Some(crate::i18n::dict), "former_member");
        }
        self.name.clone()
    }

    /// Имя, другие имена и код через `kernel::search::SEP` — для поиска.
    pub fn search(&self, lang: Lang) -> String {
        let title = self.title(lang);
        kernel::search::join(std::iter::once(title.as_str()).chain(self.aliases.iter().map(String::as_str)).chain([self.code.as_str()]))
    }

    /// Запись участника проекта (не человека без аккаунта).
    pub fn is_member(&self) -> bool {
        self.user_id.is_some()
    }
}
