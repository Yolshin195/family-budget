-- Фича: planning. Серии планов (этап 0, Ц4). Смена правила дат задним числом больше не переписывает
-- историю: прежний план закрывается (end_on), новый продолжает серию (тот же series_id) и действует
-- с effective_from — даты правила раньше неё он не повторяет, их оплаты и пропуски остались у прежнего
-- плана. series_id — ещё и задел для истории сумм плана (группа 2).
ALTER TABLE fb_plans ADD COLUMN series_id UUID NULL;
UPDATE fb_plans SET series_id = id;
ALTER TABLE fb_plans ALTER COLUMN series_id SET NOT NULL;
ALTER TABLE fb_plans ADD COLUMN effective_from DATE NULL;
CREATE INDEX fb_plans_series_idx ON fb_plans (project_id, series_id);
