-- Фича: rates (Курсы валют). Без FK на таблицы других фич: ссылки проверяет сервис через порты.
CREATE TABLE fb_rates
(
    id          UUID PRIMARY KEY,
    project_id     UUID        NOT NULL,
    rate_date   DATE NOT NULL,
    currency_code TEXT NOT NULL,
    rate        NUMERIC(18, 6) NOT NULL,
    created_at  TIMESTAMPTZ NOT NULL DEFAULT now()
);

CREATE INDEX fb_rates_project_rate_date_idx ON fb_rates (project_id, rate_date DESC);

CREATE UNIQUE INDEX fb_rates_uniq ON fb_rates (project_id, currency_code, rate_date);
