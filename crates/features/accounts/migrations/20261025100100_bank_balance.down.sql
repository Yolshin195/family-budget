ALTER TABLE fb_accounts
    DROP CONSTRAINT IF EXISTS fb_accounts_bank_balance,
    DROP COLUMN IF EXISTS bank_balance_at,
    DROP COLUMN IF EXISTS bank_balance;
