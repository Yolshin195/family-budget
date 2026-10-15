-- Фича: accounts. Двойная запись (ТЗ, раздел 3): остаток счёта — сумма его проводок, начальный
-- остаток — транзакция OPENING в журнале операций. Прежние начальные остатки сохраняются во
-- временную таблицу; миграция фичи transactions превращает их в транзакции и удаляет таблицу.
CREATE TABLE fb_accounts_opening_legacy AS
SELECT project_id, code, currency_code, opening_balance, created_at
FROM fb_accounts
WHERE opening_balance <> 0;

ALTER TABLE fb_accounts DROP COLUMN opening_balance;
