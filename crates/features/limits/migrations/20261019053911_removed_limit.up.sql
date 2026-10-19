-- limits: removed_limit (up). «Снять лимит с даты» — правило без суммы: amount NULL. Лимит 0 — настоящий
-- нулевой лимит (тратить нельзя совсем), поэтому снятие не может быть суммой 0.
ALTER TABLE fb_limits
    ALTER COLUMN amount DROP NOT NULL,
    ADD CONSTRAINT fb_limits_amount_check CHECK (amount IS NULL OR amount >= 0);
