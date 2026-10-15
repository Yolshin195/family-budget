-- Доступы ассистентов (MCP) по OAuth: кто (пользователь), какой клиент (azp токена провайдера, например
-- `claude`), к какому проекту и с какими правами. Права меняет только человек в браузере; токен
-- провайдера сам по себе прав не даёт — без строки здесь ассистент видит только подсказку о настройке.
CREATE TABLE fb_mcp_grants (
    id           UUID        PRIMARY KEY,
    user_id      UUID        NOT NULL,
    client_id    TEXT        NOT NULL,
    project_id   UUID        NOT NULL,
    scopes       TEXT[]      NOT NULL,
    -- Доступ открыт при входе со вторым фактором (для проектов с «Требовать 2FA»).
    mfa          BOOLEAN     NOT NULL DEFAULT FALSE,
    created_at   TIMESTAMPTZ NOT NULL DEFAULT now(),
    updated_at   TIMESTAMPTZ NOT NULL DEFAULT now(),
    last_used_at TIMESTAMPTZ,
    UNIQUE (user_id, client_id)
);
CREATE INDEX fb_mcp_grants_project_idx ON fb_mcp_grants (project_id);
