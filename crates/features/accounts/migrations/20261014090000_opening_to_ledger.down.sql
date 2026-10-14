-- Начальные остатки остаются транзакциями OPENING в журнале операций; колонка возвращается нулевой.
ALTER TABLE fb_accounts ADD COLUMN opening_balance NUMERIC(18, 4) NOT NULL DEFAULT 0;
DROP TABLE IF EXISTS fb_accounts_opening_legacy;
