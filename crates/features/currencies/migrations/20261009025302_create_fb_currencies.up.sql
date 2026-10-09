-- Фича: currencies (Валюты). Без FK на таблицы других фич: ссылки проверяет сервис через порты.
CREATE TABLE fb_currencies
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
    symbol      TEXT NOT NULL,
    minor_units INTEGER NOT NULL,
    created_at  TIMESTAMPTZ NOT NULL DEFAULT now(),
    UNIQUE (project_id, code)
);
