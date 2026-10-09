-- Фича: planning (Планирование). Ссылки на счета и категории проверяет сервис через порты.
-- Оплата вхождения хранится в транзакции (plan_id + plan_date), здесь — только планы и пропуски.
CREATE TABLE fb_plans
(
    id            UUID PRIMARY KEY,
    project_id    UUID           NOT NULL,
    name          TEXT           NOT NULL,
    name_lang     TEXT           NOT NULL CHECK (name_lang IN ('RU', 'EN', 'TH')),
    name_ru       TEXT           NULL,
    name_en       TEXT           NULL,
    name_th       TEXT           NULL,
    tx_kind       TEXT           NOT NULL CHECK (tx_kind IN ('EXPENSE', 'INCOME')),
    account_code  TEXT           NOT NULL,
    category_code TEXT           NOT NULL,
    amount        NUMERIC(18, 4) NOT NULL CHECK (amount > 0),
    recurrence    TEXT           NOT NULL CHECK (recurrence IN ('ONCE', 'DAILY', 'WEEKLY', 'MONTHLY', 'YEARLY')),
    interval_n    INTEGER        NOT NULL DEFAULT 1 CHECK (interval_n BETWEEN 1 AND 366),
    start_on      DATE           NOT NULL,
    end_on        DATE           NULL CHECK (end_on IS NULL OR end_on >= start_on),
    active        BOOLEAN        NOT NULL DEFAULT TRUE,
    created_by    UUID           NOT NULL,
    created_at    TIMESTAMPTZ    NOT NULL DEFAULT now()
);

CREATE INDEX fb_plans_project_idx ON fb_plans (project_id);

CREATE TABLE fb_plan_skips
(
    plan_id    UUID        NOT NULL REFERENCES fb_plans (id) ON DELETE CASCADE,
    plan_date  DATE        NOT NULL,
    project_id UUID        NOT NULL,
    skipped_by UUID        NOT NULL,
    skipped_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    PRIMARY KEY (plan_id, plan_date)
);
