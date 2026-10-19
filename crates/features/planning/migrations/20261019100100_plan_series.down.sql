DROP INDEX IF EXISTS fb_plans_series_idx;
ALTER TABLE fb_plans DROP COLUMN IF EXISTS effective_from;
ALTER TABLE fb_plans DROP COLUMN IF EXISTS series_id;
