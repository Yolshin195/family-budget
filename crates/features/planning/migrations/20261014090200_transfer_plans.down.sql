DELETE FROM fb_plans WHERE tx_kind = 'TRANSFER';
ALTER TABLE fb_plans DROP CONSTRAINT IF EXISTS fb_plans_target_check;
ALTER TABLE fb_plans DROP COLUMN to_account_code;
ALTER TABLE fb_plans ALTER COLUMN category_code SET NOT NULL;
ALTER TABLE fb_plans DROP CONSTRAINT fb_plans_tx_kind_check;
ALTER TABLE fb_plans ADD CONSTRAINT fb_plans_tx_kind_check CHECK (tx_kind IN ('EXPENSE', 'INCOME'));
