-- Фича: projects. ТЗ v3 (разделы 12–14): вход через Rauthy, серверные сессии, API-токены, журнал безопасности.
-- Пароли больше не хранятся: пользователь находится по `sub` из ID-токена Rauthy.

-- Пользователи: rauthy_sub вместо пароля. Существующим (созданным до Rauthy) даём заглушку `legacy:<id>` —
-- войти под ними через Rauthy нельзя, пока администратор не проставит настоящий sub.
ALTER TABLE fb_users ADD COLUMN rauthy_sub TEXT NULL;
UPDATE fb_users SET rauthy_sub = 'legacy:' || id::text WHERE rauthy_sub IS NULL;
ALTER TABLE fb_users ALTER COLUMN rauthy_sub SET NOT NULL;
CREATE UNIQUE INDEX fb_users_rauthy_sub_uniq ON fb_users (rauthy_sub);
ALTER TABLE fb_users DROP COLUMN password_hash;
ALTER TABLE fb_users ADD COLUMN avatar_url TEXT NULL;
ALTER TABLE fb_users ADD COLUMN theme TEXT NOT NULL DEFAULT 'SYSTEM' CHECK (theme IN ('LIGHT', 'DARK', 'SYSTEM'));
-- Email обновляется из ID-токена при каждом входе и уникален в Rauthy, а не здесь: после смены адреса
-- у одного пользователя старая копия у другого не должна ломать вход. Поиск по email (приглашения) — по индексу.
DROP INDEX fb_users_email_uniq;
CREATE INDEX fb_users_email_idx ON fb_users (lower(email));

-- Проект может требовать второй фактор от участников (признак `amr` в ID-токене).
ALTER TABLE fb_projects ADD COLUMN require_2fa BOOLEAN NOT NULL DEFAULT false;
CREATE INDEX fb_projects_created_by_idx ON fb_projects (created_by);

-- Серверные сессии: в cookie — случайный id 256 бит, здесь — только его SHA-256.
CREATE TABLE fb_sessions
(
    id_hash      TEXT PRIMARY KEY,
    user_id      UUID        NOT NULL REFERENCES fb_users (id) ON DELETE CASCADE,
    created_at   TIMESTAMPTZ NOT NULL DEFAULT now(),
    last_seen_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    -- Абсолютный срок жизни (≤ 30 дней); неактивность (≤ 7 дней) считается от last_seen_at.
    expires_at   TIMESTAMPTZ NOT NULL,
    -- Был ли второй фактор при входе (по claim `amr`).
    mfa          BOOLEAN     NOT NULL DEFAULT false,
    ip           TEXT        NULL,
    user_agent   TEXT        NULL,
    revoked_at   TIMESTAMPTZ NULL
);

CREATE INDEX fb_sessions_user_idx ON fb_sessions (user_id);

-- Личные API-токены для клиентов без браузера: один проект, права, срок. Токен показывается один раз.
CREATE TABLE fb_api_tokens
(
    id           UUID PRIMARY KEY,
    user_id      UUID        NOT NULL REFERENCES fb_users (id) ON DELETE CASCADE,
    project_id   UUID        NOT NULL REFERENCES fb_projects (id) ON DELETE CASCADE,
    name         TEXT        NOT NULL,
    token_hash   TEXT        NOT NULL,
    -- read, write, <фича>:read, <фича>:write
    scopes       TEXT[]      NOT NULL,
    expires_at   TIMESTAMPTZ NULL,
    last_used_at TIMESTAMPTZ NULL,
    revoked_at   TIMESTAMPTZ NULL,
    created_at   TIMESTAMPTZ NOT NULL DEFAULT now(),
    CHECK (cardinality(scopes) > 0)
);

CREATE UNIQUE INDEX fb_api_tokens_hash_uniq ON fb_api_tokens (token_hash);
CREATE INDEX fb_api_tokens_user_idx ON fb_api_tokens (user_id);
CREATE INDEX fb_api_tokens_project_idx ON fb_api_tokens (project_id);

-- Журнал безопасности: входы, участники и роли, API-токены, удаление данных. Хранится 1 год.
-- Без FK: записи переживают удаление пользователя (user_id обнуляется — личных данных не остаётся).
CREATE TABLE fb_audit_log
(
    id         UUID PRIMARY KEY,
    at         TIMESTAMPTZ NOT NULL DEFAULT now(),
    user_id    UUID        NULL,
    project_id UUID        NULL,
    action     TEXT        NOT NULL,
    ip         TEXT        NULL,
    user_agent TEXT        NULL,
    details    JSONB       NOT NULL DEFAULT '{}'::jsonb
);

CREATE INDEX fb_audit_log_user_at_idx ON fb_audit_log (user_id, at DESC);
CREATE INDEX fb_audit_log_at_idx ON fb_audit_log (at);
