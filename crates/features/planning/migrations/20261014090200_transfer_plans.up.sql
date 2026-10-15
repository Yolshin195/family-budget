-- Фича: planning. Плановый перевод между счетами (ТЗ, раздел 6): вместо категории — счёт зачисления.
ALTER TABLE fb_plans DROP CONSTRAINT fb_plans_tx_kind_check;
ALTER TABLE fb_plans ADD CONSTRAINT fb_plans_tx_kind_check CHECK (tx_kind IN ('EXPENSE', 'INCOME', 'TRANSFER'));
ALTER TABLE fb_plans ALTER COLUMN category_code DROP NOT NULL;
ALTER TABLE fb_plans ADD COLUMN to_account_code TEXT NULL;
ALTER TABLE fb_plans ADD CONSTRAINT fb_plans_target_check CHECK (
    (tx_kind = 'TRANSFER' AND to_account_code IS NOT NULL AND category_code IS NULL)
        OR (tx_kind <> 'TRANSFER' AND to_account_code IS NULL AND category_code IS NOT NULL));
