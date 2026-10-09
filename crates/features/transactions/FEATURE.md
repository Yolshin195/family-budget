# transactions — Транзакции (написана вручную)

Шапка операции + позиции; сумма транзакции = сумма позиций. Описание — оригинал + язык + переводы ru/en/th.
Оплата вхождения плана хранится здесь: `plan_id` + `plan_date` (уникальны в проекте).

Таблицы: `fb_transactions`, `fb_transaction_lines` (FK на шапку, ON DELETE CASCADE).
Порты: `AccountsLookup` (exists, choices, titles), `CategoriesLookup` (kind_of, choices, titles) → `app/wiring/wire_transactions.rs`.
Публичный API для других фич: `lines`, `payments`, `turnover`.

REST (`/api/transactions`): GET/POST `/`, GET/PUT/DELETE `/{id}`, PUT `/{id}/translation`.
Страницы (`/transactions`): список за месяц с фильтрами, форма с позициями, перевод описания.

Тесты: `cargo test -p transactions`.
