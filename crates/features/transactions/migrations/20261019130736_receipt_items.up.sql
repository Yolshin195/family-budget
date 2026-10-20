-- Фича: transactions. Позиции чека (группа 1, шаг 2; docs/group-1.md, разделы 4 и 7): количество × цена −
-- скидка = сумма позиции. Проводка на категорию ссылается на свою позицию: у расхода и дохода одна проводка
-- CATEGORY на позицию, её сумма — ±amount. Проводки не меняются, кроме простановки item_id.
CREATE TABLE fb_transaction_items
(
    id             UUID PRIMARY KEY,
    transaction_id UUID           NOT NULL REFERENCES fb_transactions (id) ON DELETE CASCADE,
    -- Порядок в чеке.
    position       INTEGER        NOT NULL,
    -- Название на языке ввода, без переводов (данные пользователя).
    item_name      TEXT           NULL,
    quantity       NUMERIC(18, 6) NOT NULL DEFAULT 1 CHECK (quantity > 0),
    unit           TEXT           NOT NULL DEFAULT 'pcs' CHECK (unit IN ('pcs', 'kg', 'l', 'g')),
    -- Цена с ценника; пусто — введена только сумма.
    price_per_unit NUMERIC(18, 6) NULL CHECK (price_per_unit >= 0),
    discount       NUMERIC(18, 4) NOT NULL DEFAULT 0 CHECK (discount >= 0),
    amount         NUMERIC(18, 4) NOT NULL CHECK (amount > 0),
    category_code  TEXT           NOT NULL,
    note           TEXT           NULL,
    UNIQUE (transaction_id, position)
);

ALTER TABLE fb_postings ADD COLUMN item_id UUID NULL REFERENCES fb_transaction_items (id);
CREATE INDEX fb_postings_item_idx ON fb_postings (item_id) WHERE item_id IS NOT NULL;

-- Служебные поля шапки (дубли и выборки по счёту и сумме — шаг 4): счёт (у перевода — списания), сумма
-- (у расхода и дохода — сумма позиций, у перевода — списано всего, у начального остатка — остаток) и возврат.
-- Пишутся приложением вместе с проводками.
ALTER TABLE fb_transactions
    ADD COLUMN account_code TEXT           NULL,
    ADD COLUMN total        NUMERIC(18, 4) NULL,
    ADD COLUMN refund       BOOLEAN        NOT NULL DEFAULT false;

-- Перенос: каждая проводка CATEGORY расхода и дохода → позиция «1 шт, сумма = модуль проводки, название =
-- уточнение»; проводке ставится item_id.
CREATE TEMPORARY TABLE fb_items_map ON COMMIT DROP AS
SELECT gen_random_uuid()                                                                AS item_id,
       p.transaction_id,
       p.position                                                                       AS posting_position,
       row_number() OVER (PARTITION BY p.transaction_id ORDER BY p.position)::INTEGER  AS item_position,
       p.note,
       abs(p.amount)                                                                    AS amount,
       p.ref_code
FROM fb_postings p
         JOIN fb_transactions t ON t.id = p.transaction_id
WHERE t.tx_kind IN ('EXPENSE', 'INCOME')
  AND p.ref_kind = 'CATEGORY';

INSERT INTO fb_transaction_items (id, transaction_id, position, item_name, amount, category_code)
SELECT item_id, transaction_id, item_position, note, amount, ref_code
FROM fb_items_map;

UPDATE fb_postings p
SET item_id = m.item_id
FROM fb_items_map m
WHERE p.transaction_id = m.transaction_id
  AND p.position = m.posting_position;

-- Шапка — из проводок. Счёт: первая проводка на счёт; у перевода — со знаком «−» (списание).
UPDATE fb_transactions t
SET account_code = a.ref_code,
    total        = CASE t.tx_kind
                       WHEN 'TRANSFER' THEN -a.amount
                       WHEN 'OPENING' THEN a.amount
                       ELSE (SELECT sum(abs(c.amount)) FROM fb_postings c WHERE c.transaction_id = t.id AND c.ref_kind = 'CATEGORY')
        END,
    -- Обычный расход уменьшает счёт, обычный доход — пополняет; иначе это возврат.
    refund       = CASE t.tx_kind
                       WHEN 'EXPENSE' THEN a.amount > 0
                       WHEN 'INCOME' THEN a.amount < 0
                       ELSE false
        END
FROM (SELECT DISTINCT ON (p.transaction_id) p.transaction_id, p.ref_code, p.amount
      FROM fb_postings p
               JOIN fb_transactions x ON x.id = p.transaction_id
      WHERE p.ref_kind = 'ACCOUNT'
        AND (x.tx_kind <> 'TRANSFER' OR p.amount < 0)
      ORDER BY p.transaction_id, p.position) a
WHERE a.transaction_id = t.id;
