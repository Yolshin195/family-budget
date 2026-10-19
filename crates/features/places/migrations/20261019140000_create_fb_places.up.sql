-- Фича: places (Места). Без FK на таблицы других фич: категорию и счёт по умолчанию проверяет сервис через порт.
CREATE TABLE fb_places
(
    id                    UUID PRIMARY KEY,
    project_id            UUID        NOT NULL,
    code                  TEXT        NOT NULL,
    name                  TEXT        NOT NULL,
    -- Варианты написания для поиска (в том числе на тайском).
    aliases               TEXT[]      NOT NULL DEFAULT '{}',
    default_category_code TEXT        NULL,
    default_account_code  TEXT        NULL,
    place_kind            TEXT        NOT NULL DEFAULT 'OTHER'
        CHECK (place_kind IN ('SHOP', 'CAFE', 'MARKET', 'ONLINE', 'SERVICE', 'OTHER')),
    -- Филиал или адрес.
    location              TEXT        NULL,
    archived              BOOLEAN     NOT NULL DEFAULT FALSE,
    created_at            TIMESTAMPTZ NOT NULL DEFAULT now(),
    UNIQUE (project_id, code)
);
