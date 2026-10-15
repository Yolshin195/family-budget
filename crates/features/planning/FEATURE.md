# planning — Плановые и регулярные операции (написана вручную)

План: название (ru/en/th), тип `EXPENSE`/`INCOME`/`TRANSFER`, счёт, категория (у перевода — счёт зачисления
`to_account_code`), ожидаемая сумма в валюте счёта, правило ONCE/DAILY/WEEKLY/MONTHLY/YEARLY × N, даты начала и
окончания. Плановый перевод (например, в копилку) не меняет доходы и расходы бюджета, «Оплатил» открывает форму
перевода (сумму зачисления при обмене вводят при оплате). Вхождения вычисляются на лету; статусы EXPECTED / OVERDUE / PAID / SKIPPED.
PAID — есть транзакция с этим `plan_id` + `plan_date` (порт `PaymentsLookup`), SKIPPED — запись в `fb_plan_skips`.

Таблицы: `fb_plans`, `fb_plan_skips`.
Порты: `PaymentsLookup`, `AccountsLookup`, `CategoriesLookup` → `app/wiring/wire_planning.rs`.

REST (`/api/planning`): `/plans` CRUD, `/plans/{id}/skips/{date}` POST/DELETE, `/occurrences?from&to`.
Страницы (`/planning`): календарь месяца (на телефоне — список по дням), карточка вхождения («Оплатил» → форма
транзакции из плана, «Пропустить» с «Отменить»), список планов («Остановить» / «Возобновить»), форма плана,
удаление плана с «Отменить» (план возвращается вместе с пропусками).

Тесты: `cargo test -p planning` (правила повторения, статусы, проверки ссылок).
