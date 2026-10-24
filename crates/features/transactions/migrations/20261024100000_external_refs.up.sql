-- Связи операций с записями внешних систем (SMS банка, позже выписки, Telegram, Apple Pay), docs/external-refs.md.
-- Внешнего ключа на fb_transactions нет намеренно: после удаления операции связь остаётся, и та же внешняя запись
-- не заводит операцию снова; «Отменить» удаление возвращает операцию с тем же id — связь снова указывает на неё.
CREATE TABLE fb_external_refs (
    id             UUID PRIMARY KEY,
    project_id     UUID        NOT NULL,
    provider       TEXT        NOT NULL CHECK (provider ~ '^[a-z0-9_]{1,40}$'),
    external_id    TEXT        NOT NULL CHECK (length(external_id) BETWEEN 1 AND 200),
    transaction_id UUID        NOT NULL,
    link           TEXT        NOT NULL CHECK (link IN ('CREATED', 'MATCHED')),
    received_at    TIMESTAMPTZ NULL,
    created_at     TIMESTAMPTZ NOT NULL DEFAULT now(),
    created_by     UUID        NOT NULL
);
CREATE UNIQUE INDEX fb_external_refs_key ON fb_external_refs (project_id, provider, external_id);
CREATE INDEX fb_external_refs_tx ON fb_external_refs (project_id, transaction_id);
