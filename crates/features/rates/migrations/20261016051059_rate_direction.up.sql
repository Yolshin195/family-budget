-- rates: rate_direction (up). Курс можно ввести в любую сторону (issue #22):
-- entered — число, как его ввели; inverted — направление (false: 1 валюта = entered базовой,
-- true: 1 базовая = entered валюты). rate по-прежнему «базовой за 1 единицу валюты», его считает сервис.
ALTER TABLE fb_rates
    ADD COLUMN inverted BOOLEAN NOT NULL DEFAULT false,
    ADD COLUMN entered  NUMERIC(18, 6);

UPDATE fb_rates SET entered = rate;

ALTER TABLE fb_rates
    ALTER COLUMN entered SET NOT NULL,
    ALTER COLUMN rate TYPE NUMERIC(24, 12);
