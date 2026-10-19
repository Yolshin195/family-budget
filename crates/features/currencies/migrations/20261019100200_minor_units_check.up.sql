-- Фича: currencies. Знаков после запятой — от 0 до 4 (этап 0, Ц8): суммы хранятся в NUMERIC(18,4).
-- Раньше это проверяла только форма, через REST и MCP проходили 7 и −1. Значения вне диапазона
-- не правим молча — миграция останавливается с перечнем.
DO
$$
    DECLARE
        bad TEXT;
    BEGIN
        SELECT string_agg(project_id || ' / ' || code || ' = ' || minor_units, ', ')
        INTO bad
        FROM fb_currencies
        WHERE minor_units NOT BETWEEN 0 AND 4;
        IF bad IS NOT NULL THEN
            RAISE EXCEPTION 'fb_currencies: знаков после запятой должно быть от 0 до 4 (проект / валюта = знаков): %', bad;
        END IF;
    END
$$;

ALTER TABLE fb_currencies ADD CONSTRAINT fb_currencies_minor_units CHECK (minor_units BETWEEN 0 AND 4);
