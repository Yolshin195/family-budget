-- Фича: limits (Лимиты). Без FK на таблицы других фич: ссылки проверяет сервис через порты.
CREATE TABLE fb_limits
(
    id          UUID PRIMARY KEY,
    project_id     UUID        NOT NULL,
    valid_from  DATE NOT NULL,
    category_code TEXT NOT NULL,
    limit_period TEXT NOT NULL CHECK (limit_period IN ('WEEK', 'MONTH', 'YEAR')),
    amount      NUMERIC(18, 4) NOT NULL,
    created_at  TIMESTAMPTZ NOT NULL DEFAULT now()
);

CREATE INDEX fb_limits_project_valid_from_idx ON fb_limits (project_id, valid_from DESC);

CREATE UNIQUE INDEX fb_limits_uniq ON fb_limits (project_id, category_code, valid_from);
