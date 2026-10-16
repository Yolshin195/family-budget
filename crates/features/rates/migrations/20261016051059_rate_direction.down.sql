-- rates: rate_direction (down). Курсы, введённые «1 базовая = x валюты», остаются пересчитанными в rate.
ALTER TABLE fb_rates
    ALTER COLUMN rate TYPE NUMERIC(18, 6),
    DROP COLUMN entered,
    DROP COLUMN inverted;
