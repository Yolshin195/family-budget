# projects — Пользователи, проекты, участники (написана вручную)

Пользователи (email + пароль argon2, язык), проекты PERSONAL/FAMILY с базовой валютой, участники с ролями OWNER/EDITOR/VIEWER.
Сервис реализует порт ядра `kernel::auth::Memberships`: middleware по нему выбирает активный проект и роль.
Новый проект наполняется через порт `ProjectBootstrap` (базовая валюта + дерево категорий на трёх языках) → `app/wiring/wire_projects.rs`.

Публично: `/login`, `/register`, `/logout`, `/lang/{ru|en|th}`, POST `/api/auth/login` (JSON → JWT).
Под JWT: GET `/api/projects`; страница `/projects` (создание проекта, участники, роли, переключение).

С Rauthy: регистрация и вход уходят провайдеру, `sub` токена сопоставляется с `fb_users.id`; меняется `kernel/src/auth.rs` и эта фича.
