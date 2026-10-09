# planning — Плановые и регулярные операции (написана вручную)

План: название (ru/en/th), тип, счёт, категория, ожидаемая сумма, правило ONCE/DAILY/WEEKLY/MONTHLY/YEARLY × N,
даты начала и окончания. Вхождения вычисляются на лету; статусы EXPECTED / OVERDUE / PAID / SKIPPED.
PAID — есть транзакция с этим `plan_id` + `plan_date` (порт `PaymentsLookup`), SKIPPED — запись в `fb_plan_skips`.

Таблицы: `fb_plans`, `fb_plan_skips`.
Порты: `PaymentsLookup`, `AccountsLookup`, `CategoriesLookup` → `app/wiring/wire_planning.rs`.

REST (`/api/planning`): `/plans` CRUD, `/plans/{id}/skips/{date}` POST/DELETE, `/occurrences?from&to`.
Страницы (`/planning`): календарь месяца, карточка вхождения («Оплатил» → форма транзакции из плана, «Пропустить»), список и форма планов.

Тесты: `cargo test -p planning` (правила повторения, статусы, проверки ссылок).
