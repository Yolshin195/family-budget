-- Фича: transactions (Транзакции). Шапка + позиции. Без FK на таблицы других фич: ссылки
-- на счета и категории проверяет сервис через порты; FK только внутри фичи (позиции → шапка).
CREATE TABLE fb_transactions
(
    id               UUID PRIMARY KEY,
    project_id       UUID        NOT NULL,
    occurred_on      DATE        NOT NULL,
    account_code     TEXT        NOT NULL,
    tx_kind          TEXT        NOT NULL CHECK (tx_kind IN ('EXPENSE', 'INCOME')),
    description      TEXT        NULL,
    description_lang TEXT        NOT NULL CHECK (description_lang IN ('RU', 'EN', 'TH')),
    description_ru   TEXT        NULL,
    description_en   TEXT        NULL,
    description_th   TEXT        NULL,
    created_by       UUID        NOT NULL,
    plan_id          UUID        NULL,
    plan_date        DATE        NULL,
    created_at       TIMESTAMPTZ NOT NULL DEFAULT now(),
    CHECK ((plan_id IS NULL) = (plan_date IS NULL))
);

CREATE INDEX fb_transactions_project_occurred_on_idx ON fb_transactions (project_id, occurred_on DESC);
-- Одно вхождение плана оплачивается одной транзакцией.
CREATE UNIQUE INDEX fb_transactions_plan_uniq ON fb_transactions (project_id, plan_id, plan_date) WHERE plan_id IS NOT NULL;

CREATE TABLE fb_transaction_lines
(
    transaction_id UUID           NOT NULL REFERENCES fb_transactions (id) ON DELETE CASCADE,
    position       INTEGER        NOT NULL,
    category_code  TEXT           NOT NULL,
    amount         NUMERIC(18, 4) NOT NULL CHECK (amount > 0),
    note           TEXT           NULL,
    PRIMARY KEY (transaction_id, position)
);

CREATE INDEX fb_transaction_lines_category_idx ON fb_transaction_lines (category_code);
