-- Фича: people (Люди, группа 1, шаг 6): для кого покупка. Без FK на таблицы других фич: участника проекта
-- связывает с записью фича projects через порт. Записи текущих участников создаёт приложение при запуске
-- (тем же правилом кода, что и при вступлении в проект).
CREATE TABLE fb_people
(
    id            UUID PRIMARY KEY,
    project_id    UUID        NOT NULL,
    code          TEXT        NOT NULL,
    -- Отображаемое имя; не переводится. Пусто — только у удалённого участника (имя показывается переводом).
    name          TEXT        NOT NULL,
    -- Другие имена для поиска.
    aliases       TEXT[]      NOT NULL DEFAULT '{}',
    -- Участник проекта; пусто — человек без аккаунта.
    user_id       UUID        NULL,
    -- Запись участника, который удалил аккаунт: имя стёрто, показывается «Удалённый участник».
    former_member BOOLEAN     NOT NULL DEFAULT FALSE,
    archived      BOOLEAN     NOT NULL DEFAULT FALSE,
    created_at    TIMESTAMPTZ NOT NULL DEFAULT now(),
    UNIQUE (project_id, code),
    UNIQUE (project_id, user_id)
);
CREATE INDEX fb_people_user ON fb_people (user_id) WHERE user_id IS NOT NULL;
