-- Откат позиций чека: уточнение позиции остаётся в проводке (note), количество, цена и скидка теряются.
ALTER TABLE fb_transactions
    DROP COLUMN refund,
    DROP COLUMN total,
    DROP COLUMN account_code;
DROP INDEX fb_postings_item_idx;
ALTER TABLE fb_postings DROP COLUMN item_id;
DROP TABLE fb_transaction_items;
