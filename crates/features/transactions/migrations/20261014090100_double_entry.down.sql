-- Откат двойной записи: расходы и доходы возвращаются в позиции. Переводы и начальные остатки в
-- прежней модели не выразить — они удаляются (начальные остатки счетов не восстанавливаются).
DROP TRIGGER IF EXISTS fb_postings_balanced ON fb_postings;
DROP FUNCTION IF EXISTS fb_postings_balanced();

CREATE TABLE fb_transaction_lines
(
    transaction_id UUID           NOT NULL REFERENCES fb_transactions (id) ON DELETE CASCADE,
    position       INTEGER        NOT NULL,
    category_code  TEXT           NOT NULL,
    amount         NUMERIC(18, 4) NOT NULL CHECK (amount > 0),
    note           TEXT           NULL,
    PRIMARY KEY (transaction_id, position)
);
CREATE INDEX fb_transaction_lines_category_idx ON fb_transaction_lines (category_code);

DELETE FROM fb_transactions WHERE tx_kind IN ('TRANSFER', 'OPENING');
-- Возвраты (обратные знаки) в прежней модели невыразимы.
DELETE FROM fb_transactions t
WHERE EXISTS (SELECT 1 FROM fb_postings p WHERE p.transaction_id = t.id AND p.ref_kind = 'CATEGORY'
              AND (CASE t.tx_kind WHEN 'EXPENSE' THEN p.amount ELSE -p.amount END) < 0);

ALTER TABLE fb_transactions ADD COLUMN account_code TEXT NULL;
UPDATE fb_transactions t
SET account_code = (SELECT p.ref_code FROM fb_postings p WHERE p.transaction_id = t.id AND p.ref_kind = 'ACCOUNT' LIMIT 1);
ALTER TABLE fb_transactions ALTER COLUMN account_code SET NOT NULL;

INSERT INTO fb_transaction_lines (transaction_id, position, category_code, amount, note)
SELECT p.transaction_id, row_number() OVER (PARTITION BY p.transaction_id ORDER BY p.position), p.ref_code,
       abs(p.amount), p.note
FROM fb_postings p
WHERE p.ref_kind = 'CATEGORY';

ALTER TABLE fb_transactions DROP CONSTRAINT IF EXISTS fb_transactions_opening_no_plan;
ALTER TABLE fb_transactions DROP CONSTRAINT fb_transactions_tx_kind_check;
ALTER TABLE fb_transactions ADD CONSTRAINT fb_transactions_tx_kind_check CHECK (tx_kind IN ('EXPENSE', 'INCOME'));
DROP TABLE fb_postings;
