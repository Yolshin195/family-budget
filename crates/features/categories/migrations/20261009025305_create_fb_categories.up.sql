-- Фича: categories (Категории). Без FK на таблицы других фич: ссылки проверяет сервис через порты.
CREATE TABLE fb_categories
(
    id          UUID PRIMARY KEY,
    project_id     UUID        NOT NULL,
    code        TEXT        NOT NULL,
    name        TEXT        NOT NULL,
    archived    BOOLEAN     NOT NULL DEFAULT FALSE,
    name_lang   TEXT NOT NULL CHECK (name_lang IN ('RU', 'EN', 'TH')),
    name_ru     TEXT NULL,
    name_en     TEXT NULL,
    name_th     TEXT NULL,
    category_kind TEXT NOT NULL CHECK (category_kind IN ('EXPENSE', 'INCOME')),
    parent_code TEXT NULL,
    sort_order  INTEGER NOT NULL,
    created_at  TIMESTAMPTZ NOT NULL DEFAULT now(),
    UNIQUE (project_id, code)
);
