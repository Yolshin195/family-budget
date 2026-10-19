-- limits: removed_limit (down). Правила «лимит снят» удаляются: без них лимит снова действует дальше.
DELETE FROM fb_limits WHERE amount IS NULL;
ALTER TABLE fb_limits
    DROP CONSTRAINT fb_limits_amount_check,
    ALTER COLUMN amount SET NOT NULL;
