# projects — Пользователи, проекты, вход и доступ (написана вручную)

Вход и регистрация — через Rauthy (OpenID Connect, ТЗ раздел 12): приложение паролей не хранит.
Пользователь ищется только по `rauthy_sub` из ID-токена; email, имя и аватар обновляются при каждом входе.
Проекты PERSONAL/FAMILY с базовой валютой и флагом «Требовать 2FA», участники OWNER/EDITOR/VIEWER.

Сервис реализует порты ядра `kernel::auth::Memberships` (проект и роль) и `kernel::auth::Authenticator`
(серверные сессии и личные API-токены) — по ним middleware строит `Scope` для всех фич.

Безопасность (раздел 13):
- вход: authorization code + PKCE S256; `state`, `nonce` и верификатор — в подписанной cookie на 10 минут;
  ID-токен проверяется по JWKS Rauthy (подпись, `iss`, `aud`, `nonce`, срок) — `adapters/oidc.rs`;
- сессия: случайный id 256 бит в cookie `__Host-session` (HttpOnly, Secure, SameSite=Lax), в БД — SHA-256;
  неактивность ≤ 7 дней, жизнь ≤ 30 дней, новый id при каждом входе; «Выйти везде»; выход завершает и сессию Rauthy;
- второй фактор — по claim `amr` (код из письма `otp`, passkey `hwk`/`swk`, `mfa`);
- API-токены `fbt_…`: один проект, права (`read`, `write`, `<фича>:read|write`), срок; в БД — хеш; показ один раз;
- журнал безопасности `fb_audit_log`: входы, участники и роли, токены, удаление данных; хранится 1 год.

Таблицы: `fb_users`, `fb_projects`, `fb_project_members`, `fb_sessions`, `fb_api_tokens`, `fb_audit_log`.
Порты: `ProjectRepository`, `AccessRepository` (PostgreSQL/память), `IdentityProvider` (Rauthy / выключен),
`ProjectBootstrap` (базовая валюта + дерево категорий), `ProjectPurge` (удаление данных проекта во всех фичах)
→ `app/wiring/wire_projects.rs`.

Публично: `/auth/login`, `/auth/register`, `/auth/callback`, `/auth/logout` (POST + CSRF), `/auth/dev`
(только `APP_ENV=dev`), `/demo` (гость — VIEWER демо-проекта), `/lang/{ru|en|th}`.
Аккаунт (без проекта): `/onboarding` — имя, базовая валюта, язык → личный проект.
Под сессией: `/projects` (проекты, участники, роли, 2FA, удаление проекта), `/profile` (имя, язык, тема,
удаление аккаунта), `/profile/security` (устройства, «Выйти везде», API-токены, журнал, кабинет Rauthy).
REST: GET `/api/projects` (API-токен видит только свой проект).

Тесты: `cargo test -p projects` — вход (первый/повторный, чужой state, просроченный вход, Facebook с тем же email),
проверка ID-токена (nonce, aud, iss, срок, чужой ключ), сессии, токены, 2FA, роли, «чужой проект → 404», удаление аккаунта.
