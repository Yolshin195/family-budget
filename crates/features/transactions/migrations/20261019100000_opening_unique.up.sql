-- Фича: transactions. Один начальный остаток на счёт (этап 0, Ц10). Раньше это проверял только
-- сервис («прочитал → записал»), и двойной клик «Сохранить» в форме счёта мог записать два остатка.
-- Счёт начального остатка дублируется в шапку, уникальность держит индекс.
ALTER TABLE fb_transactions ADD COLUMN opening_account TEXT NULL;

UPDATE fb_transactions t
SET opening_account = p.ref_code
FROM fb_postings p
WHERE p.transaction_id = t.id AND t.tx_kind = 'OPENING' AND p.ref_kind = 'ACCOUNT';

-- Повторы не удаляем молча: какой из остатков верный, решает владелец.
DO
$$
    DECLARE
        dup TEXT;
    BEGIN
        SELECT string_agg(project_id || ' / ' || opening_account, ', ')
        INTO dup
        FROM (SELECT project_id, opening_account
              FROM fb_transactions
              WHERE tx_kind = 'OPENING'
              GROUP BY project_id, opening_account
              HAVING count(*) > 1) d;
        IF dup IS NOT NULL THEN
            RAISE EXCEPTION 'fb_transactions: у счетов несколько начальных остатков (проект / счёт): % — оставьте по одному', dup;
        END IF;
    END
$$;

ALTER TABLE fb_transactions
    ADD CONSTRAINT fb_transactions_opening_account CHECK ((tx_kind = 'OPENING') = (opening_account IS NOT NULL));
CREATE UNIQUE INDEX fb_transactions_opening_uniq ON fb_transactions (project_id, opening_account) WHERE tx_kind = 'OPENING';
