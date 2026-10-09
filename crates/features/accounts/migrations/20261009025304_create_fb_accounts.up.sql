-- Фича: accounts (Счета). Без FK на таблицы других фич: ссылки проверяет сервис через порты.
CREATE TABLE fb_accounts
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
    account_kind TEXT NOT NULL CHECK (account_kind IN ('CASH', 'CARD', 'BANK', 'SAVINGS')),
    currency_code TEXT NOT NULL,
    opening_balance NUMERIC(18, 4) NOT NULL,
    holder_user_id UUID NULL,
    created_at  TIMESTAMPTZ NOT NULL DEFAULT now(),
    UNIQUE (project_id, code)
);
