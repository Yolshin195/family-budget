# transactions — Транзакции (написана вручную)

Шапка операции + позиции; сумма транзакции = сумма позиций. Описание — оригинал + язык + переводы ru/en/th.
Оплата вхождения плана хранится здесь: `plan_id` + `plan_date` (уникальны в проекте).

Таблицы: `fb_transactions`, `fb_transaction_lines` (FK на шапку, ON DELETE CASCADE).
Порты: `AccountsLookup` (exists, choices, titles, infos — валюта и символ), `CategoriesLookup` (kind_of, choices,
titles, infos — названия на трёх языках), `BudgetView` (остатки лимитов за месяц и курсы, из фичи budget,
подключается после её сборки) → `app/wiring/wire_transactions.rs`.
Публичный API для других фич: `lines`, `payments`, `turnover`, `purge_project`, `first_expenses_between`.

Быстрый расход (ТЗ, раздел 10): `GET /api/transactions/quick-defaults?date` — счёт по умолчанию (последний этого
пользователя в проекте), до 6 частых категорий за 30 дней (дополняются корнями дерева), дерево категорий с поиском
на ru/en/th, остатки лимитов, курсы счетов к базовой валюте. Всё — из позиций транзакций, новых таблиц нет
(индекс `fb_transactions_quick_idx`). Сохранение — обычный `POST /api/transactions`; «Отменить» — `DELETE`.
Диалог — в ядре (`kernel/templates/base.html` + `app/static/app.js`).

REST (`/api/transactions`): GET/POST `/`, GET `/quick-defaults`, GET/PUT/DELETE `/{id}`, PUT `/{id}/translation`.
Страницы (`/transactions`): список за месяц с фильтрами, форма с позициями (предупреждение о лимите),
перевод описания, удаление с «Отменить» (`/transactions/undo/{token}`).

Тесты: `cargo test -p transactions`.
