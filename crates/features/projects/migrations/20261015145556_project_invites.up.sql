-- projects: project_invites (up)
-- Приглашения в семейный проект по ссылке или QR-коду. Сама ссылка в БД не хранится: она — id приглашения
-- с HMAC-подписью секретом приложения, поэтому копия базы не даёт рабочих ссылок, а владелец может
-- показать ссылку и QR повторно. Одно приглашение — один человек, срок — 7 дней.
CREATE TABLE fb_project_invites
(
    id         UUID PRIMARY KEY,
    project_id UUID        NOT NULL REFERENCES fb_projects (id) ON DELETE CASCADE,
    -- Владельцем по приглашению не стать: роль OWNER выдаёт только владелец в «Участниках».
    role       TEXT        NOT NULL CHECK (role IN ('EDITOR', 'VIEWER')),
    -- Удалил аккаунт пригласивший — его приглашения больше не действуют.
    created_by UUID        NOT NULL REFERENCES fb_users (id) ON DELETE CASCADE,
    created_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    expires_at TIMESTAMPTZ NOT NULL,
    used_by    UUID        NULL REFERENCES fb_users (id) ON DELETE SET NULL,
    used_at    TIMESTAMPTZ NULL,
    revoked_at TIMESTAMPTZ NULL
);

CREATE INDEX fb_project_invites_project_idx ON fb_project_invites (project_id, created_at DESC);
CREATE INDEX fb_project_invites_created_by_idx ON fb_project_invites (created_by);
CREATE INDEX fb_project_invites_used_by_idx ON fb_project_invites (used_by);
