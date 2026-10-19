-- Группа 1, шаг 5: место операции (справочник places). Только у расхода и дохода; перевод и начальный остаток —
-- всегда без места. FK на чужую таблицу нет: ссылку проверяет сервис через порт.
ALTER TABLE fb_transactions ADD COLUMN place_code TEXT NULL;
CREATE INDEX fb_transactions_place ON fb_transactions (project_id, place_code) WHERE place_code IS NOT NULL;
