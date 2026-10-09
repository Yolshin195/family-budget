-- Фича: projects (Пользователи, проекты, участники). Регистрируется первой.
CREATE TABLE fb_users
(
    id            UUID PRIMARY KEY,
    email         TEXT        NOT NULL,
    display_name  TEXT        NOT NULL,
    password_hash TEXT        NOT NULL,
    lang          TEXT        NOT NULL DEFAULT 'RU' CHECK (lang IN ('RU', 'EN', 'TH')),
    created_at    TIMESTAMPTZ NOT NULL DEFAULT now()
);

CREATE UNIQUE INDEX fb_users_email_uniq ON fb_users (lower(email));

CREATE TABLE fb_projects
(
    id            UUID PRIMARY KEY,
    name          TEXT        NOT NULL,
    project_kind  TEXT        NOT NULL CHECK (project_kind IN ('PERSONAL', 'FAMILY')),
    base_currency TEXT        NOT NULL,
    created_by    UUID        NOT NULL REFERENCES fb_users (id),
    created_at    TIMESTAMPTZ NOT NULL DEFAULT now()
);

CREATE TABLE fb_project_members
(
    project_id UUID        NOT NULL REFERENCES fb_projects (id) ON DELETE CASCADE,
    user_id    UUID        NOT NULL REFERENCES fb_users (id) ON DELETE CASCADE,
    role       TEXT        NOT NULL CHECK (role IN ('OWNER', 'EDITOR', 'VIEWER')),
    joined_at  TIMESTAMPTZ NOT NULL DEFAULT now(),
    PRIMARY KEY (project_id, user_id)
);

CREATE INDEX fb_project_members_user_idx ON fb_project_members (user_id);
