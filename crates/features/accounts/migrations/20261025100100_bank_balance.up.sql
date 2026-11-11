-- Остаток счёта по данным банка (Outstanding Balance из SMS банка) и момент, на который он верен. Пишет только
-- сервер по связи с SMS (docs/external-refs.md, раздел 5); правка счёта его не меняет.
ALTER TABLE fb_accounts
    ADD COLUMN bank_balance    NUMERIC(18, 4) NULL,
    ADD COLUMN bank_balance_at TIMESTAMPTZ    NULL,
    ADD CONSTRAINT fb_accounts_bank_balance CHECK ((bank_balance IS NULL) = (bank_balance_at IS NULL));
