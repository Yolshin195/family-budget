DROP INDEX IF EXISTS fb_transactions_opening_uniq;
ALTER TABLE fb_transactions DROP CONSTRAINT IF EXISTS fb_transactions_opening_account;
ALTER TABLE fb_transactions DROP COLUMN IF EXISTS opening_account;
