-- Фича: transactions. Двойная запись (ТЗ, раздел 4): транзакция = шапка + проводки, сумма проводок
-- по каждой валюте равна нулю. Позиции и счёт в шапке переносятся в проводки; начальные остатки
-- счетов (их сохранила миграция фичи accounts) становятся транзакциями OPENING.
CREATE TABLE fb_postings
(
    transaction_id UUID           NOT NULL REFERENCES fb_transactions (id) ON DELETE CASCADE,
    position       INTEGER        NOT NULL,
    ref_kind       TEXT           NOT NULL CHECK (ref_kind IN ('ACCOUNT', 'CATEGORY', 'SYSTEM')),
    ref_code       TEXT           NOT NULL,
    currency_code  TEXT           NOT NULL,
    amount         NUMERIC(18, 4) NOT NULL CHECK (amount <> 0),
    note           TEXT           NULL,
    PRIMARY KEY (transaction_id, position)
);

CREATE INDEX fb_postings_ref_idx ON fb_postings (ref_kind, ref_code);

-- Разовый перенос данных. Валюта счёта и автор начального остатка читаются из таблиц других фич —
-- только здесь; дальше фичи связаны лишь через порты.
DO
$$
    BEGIN
        IF to_regclass('fb_accounts') IS NOT NULL THEN
            INSERT INTO fb_postings (transaction_id, position, ref_kind, ref_code, currency_code, amount)
            SELECT t.id, 1, 'ACCOUNT', t.account_code, a.currency_code,
                   CASE t.tx_kind WHEN 'EXPENSE' THEN -s.total ELSE s.total END
            FROM fb_transactions t
                     JOIN (SELECT transaction_id, SUM(amount) AS total FROM fb_transaction_lines GROUP BY transaction_id) s
                          ON s.transaction_id = t.id
                     JOIN fb_accounts a ON a.project_id = t.project_id AND a.code = t.account_code;

            INSERT INTO fb_postings (transaction_id, position, ref_kind, ref_code, currency_code, amount, note)
            SELECT l.transaction_id, l.position + 1, 'CATEGORY', l.category_code, a.currency_code,
                   CASE t.tx_kind WHEN 'EXPENSE' THEN l.amount ELSE -l.amount END, l.note
            FROM fb_transaction_lines l
                     JOIN fb_transactions t ON t.id = l.transaction_id
                     JOIN fb_accounts a ON a.project_id = t.project_id AND a.code = t.account_code;
        END IF;
        -- Шапки без проводок (счёт не найден) — несогласованные данные, их не сбалансировать.
        DELETE FROM fb_transactions t WHERE NOT EXISTS (SELECT 1 FROM fb_postings p WHERE p.transaction_id = t.id);
    END
$$;

ALTER TABLE fb_transactions DROP CONSTRAINT fb_transactions_tx_kind_check;
ALTER TABLE fb_transactions
    ADD CONSTRAINT fb_transactions_tx_kind_check CHECK (tx_kind IN ('EXPENSE', 'INCOME', 'TRANSFER', 'OPENING'));
ALTER TABLE fb_transactions
    ADD CONSTRAINT fb_transactions_opening_no_plan CHECK (tx_kind <> 'OPENING' OR plan_id IS NULL);
ALTER TABLE fb_transactions DROP COLUMN account_code;
DROP TABLE fb_transaction_lines;

-- Начальные остатки счетов → транзакции OPENING (после смены схемы: тип OPENING, без счёта в шапке).
DO
$$
    BEGIN
        IF to_regclass('fb_accounts_opening_legacy') IS NOT NULL THEN
            CREATE TEMP TABLE fb_opening_tmp ON COMMIT DROP AS
            SELECT gen_random_uuid() AS id, o.project_id, o.code, o.currency_code, o.opening_balance,
                   LEAST(o.created_at::date,
                         COALESCE((SELECT min(t.occurred_on)
                                   FROM fb_transactions t
                                            JOIN fb_postings p ON p.transaction_id = t.id
                                   WHERE t.project_id = o.project_id AND p.ref_kind = 'ACCOUNT' AND p.ref_code = o.code),
                                  o.created_at::date))                         AS opened_on,
                   (SELECT pr.created_by FROM fb_projects pr WHERE pr.id = o.project_id) AS created_by
            FROM fb_accounts_opening_legacy o
            WHERE o.opening_balance <> 0;
            DELETE FROM fb_opening_tmp WHERE created_by IS NULL;

            INSERT INTO fb_transactions (id, project_id, occurred_on, tx_kind, description_lang, created_by)
            SELECT id, project_id, opened_on, 'OPENING', 'RU', created_by FROM fb_opening_tmp;
            INSERT INTO fb_postings (transaction_id, position, ref_kind, ref_code, currency_code, amount)
            SELECT id, 1, 'ACCOUNT', code, currency_code, opening_balance FROM fb_opening_tmp;
            INSERT INTO fb_postings (transaction_id, position, ref_kind, ref_code, currency_code, amount)
            SELECT id, 2, 'SYSTEM', 'OPENING', currency_code, -opening_balance FROM fb_opening_tmp;

            DROP TABLE fb_accounts_opening_legacy;
        END IF;
    END
$$;

-- Главное правило двойной записи проверяется и в БД: при фиксации SQL-транзакции сумма проводок
-- каждой затронутой транзакции по каждой валюте должна быть нулевой.
CREATE FUNCTION fb_postings_balanced() RETURNS trigger
    LANGUAGE plpgsql AS
$$
DECLARE
    tx  UUID;
    bad TEXT;
BEGIN
    IF TG_OP = 'DELETE' THEN
        tx := OLD.transaction_id;
    ELSE
        tx := NEW.transaction_id;
    END IF;
    SELECT currency_code INTO bad
    FROM fb_postings
    WHERE transaction_id = tx
    GROUP BY currency_code
    HAVING SUM(amount) <> 0
    LIMIT 1;
    IF bad IS NOT NULL THEN
        RAISE EXCEPTION 'fb_postings: сумма проводок транзакции % в % не равна нулю', tx, bad
            USING ERRCODE = 'check_violation';
    END IF;
    RETURN NULL;
END
$$;

CREATE CONSTRAINT TRIGGER fb_postings_balanced
    AFTER INSERT OR UPDATE OR DELETE
    ON fb_postings
    DEFERRABLE INITIALLY DEFERRED
    FOR EACH ROW
EXECUTE FUNCTION fb_postings_balanced();
