# 05. Реестр фич

**Как считать покрытие.** Сценарии в `04-examples/*.feature` — спецификация: исполняемого прогона Gherkin в проекте
нет (вопрос Q-03). Поэтому правило считается **покрытым**, если его проверяет автотест `cargo test -p mcp`
(`crates/features/mcp/src/tests.rs`, 107 тестов) или тест фичи, которую зовёт MCP. Правила с тегом `@model` —
поведение самой модели; сервером их не проверить, нужен прогон агента.

Статус реализации: **есть** · **частично** (работает, но есть известные ошибки или непокрытые правила) · **нет** ·
**расходится** с требованиями. Приоритет — срез Story Map: MVP / следующий релиз / потом.

## Таблица

| ID | Фича | Ценность | Актор | Цель (Impact Map) | Компоненты агента | Правила | Статус | Покрытие сценариями | Приоритет | Открытые вопросы | Источники |
| --- | --- | --- | --- | --- | --- | --- | --- | --- | --- | --- | --- |
| F-01 | Доступ под контролем пользователя | Ассистент делает только разрешённое и ничего не удаляет | участник, VIEWER, ассистент, Rauthy | C1 | «Ассистенты», API-токен, auth-слой, `tools/list`, `check`, аннотации | R1–R8 | частично | 10 сценариев; без автотеста R8 (`@model`) | MVP | Q-04 | `mcp.md` «Права», «Вход»; `plan` 2–3, 6; `svc:426-439` |
| F-02 | Знание бюджета перед действием | Записи в нужные счета и категории | ассистент | C2 | `INSTRUCTIONS`, `get_context` | R1–R2 | частично | 2; без автотеста R1 (`@model`) | MVP | Q-03 | `srv:21-23, 77`; `svc:449-530` |
| F-03 | Понимание названий на любом языке | Названия без кодов | участник, ассистент | C3 | поиск ссылок | R1–R2 | есть | 3; все правила покрыты | MVP | — | `mcp.md` «Соглашения»; `dom:840-861` |
| F-04 | Расход и доход за секунды | Трата с голоса и остаток лимита | участник | C4 | `add_expense`, `add_income` | R1–R6 | частично | 11; без автотеста R2, R4, R6 | MVP | Q-05 | `srv:138-174`; `svc:754-830`; `stage-0.md` реш. 3 |
| F-05 | Чек по фото | Чек позициями без ручного ввода | участник | C5 | `add_expense` с `items` | R1–R3 | частично | 3; без автотеста R3 (`@model`) | MVP | Q-03 | `srv:138`; `docs/group-1.md` |
| F-06 | Без дублей с SMS банка | Покупка из банка не дублируется | участник, банк | C7 | `INSTRUCTIONS`, `list_transactions(from_sms)`, `update_transaction` | R1–R2 | частично | 5; без автотеста R2 (`@model`) | MVP | Q-03, Q-06 | `srv:37-42`; `external-refs.md` |
| F-07 | Место и «для кого» | Точные отчёты по местам и людям | участник | C6 | `place`, `for`, `list_places`, `list_people` | R1–R3 | частично | 5; без автотеста R2 (`@model`) | MVP | — | `srv:24-29`; `svc:758-767` |
| F-08 | Перевод и обмен валют | Перевод и обмен — одна запись | участник | C8 | `add_transfer` | R1–R3 | частично | 4; без автотеста R2, R3 | MVP | — | `srv:176`; `svc:883` |
| F-09 | Исправление операций | Ошибку можно поправить словами | участник | C9 | `update_transaction` | R1–R4 | частично | 4; без автотеста R2 (`@model`), R4 | MVP | Q-07 | `srv:195`; `svc:894-995` |
| F-10 | Плановые платежи | Регулярные платежи — одной фразой | участник | C10 | `list_upcoming_payments`, `pay_planned`, `skip_planned`, `*_plan` | R1–R6 | частично | 8; без автотеста R1 (отказы), R2; R6 не сделано | MVP (R6 — следующий релиз) | — | `srv:125, 214-297`; `stage-0.md` реш. 4–5; `debt` №33 |
| F-11 | Справочники через разговор | Справочники без формы | участник (OWNER, EDITOR) | C11 | `add_/update_*` справочников | R1–R7 | частично | 7; без автотеста R4 | MVP | — | `srv:299-513`; `mcp.md`; `debt` №18 |
| F-12 | Курсы валют | Верные итоги при любом направлении курса | участник | C12 | `list_rates`, `set_rate` | R1–R3 | есть | 5; все правила покрыты | MVP | — | `srv:515-545`; `svc:1376-1423` |
| F-13 | Лимиты | Ограничить траты словами | участник | C13 | `list_limits`, `set_limit` | R1–R4 | есть | 5; все правила покрыты | MVP | — | `srv:547-573`; `stage-0.md` реш. 1–2 |
| F-14 | Ответы о деньгах | Ответ без приложения | участник | C14 | `get_budget`, `get_balances`, `list_transactions` | R1–R5 | частично | 5; без автотеста R1, R3 (ошибка), R4 (не сделано), R5 | MVP (R3, R4 — следующий релиз) | — | `srv:86-123`; `debt` №4, №25 |
| F-15 | Журнал действий ассистента | Видно, что делал ассистент | участник, журнал | C16 | `audit_write` | R1–R3 | частично | 3; без автотеста R2; R3 не сделано | следующий релиз | — | `svc:442-446`; `debt` №49 |
| F-16 | Записи на трёх языках | Каждый читает на своём языке | участники семьи | C15 | `INSTRUCTIONS`, `translations` | R1–R3 | частично | 3; без автотеста R1 (`@model`), R2, R3 | MVP | Q-11 | `srv:30-32`; `plan` 4; `CLAUDE.md` |
| F-17 | Удаление с отменой | Убрать лишнее и вернуть случайное | участник | G1 | — | R1–R2 | нет | 2; не сделано | потом | Q-08 (блокирует) | `plan` 2–4; `debt` №54 |
| F-18 | Импорт выписки банка | Месяц операций одним файлом | участник, банк | G2 | — | R1 | нет | 1; не сделано | потом | Q-09 (блокирует) | `plan` 7 этап 5 |
| F-19 | Итоги месяца | Итоги по шаблону | участник | G3 | — | R1 | нет | 1; не сделано | потом | Q-10 (блокирует) | `plan` 7 этап 5 |

**Итого:** 19 фич: 3 — есть, 13 — частично, 3 — нет, расходящихся нет (отличия от плана приняты владельцем, см. 06).
Правил в сделанных фичах — 64, покрыто автотестами — 39 (61 %); 9 правил помечены `@model`.

## Карточки фич

Карточка — описание, правила с отметкой покрытия, сценарии (файл), вопросы, источники. ✅ — есть автотест,
❌ — нет, 🤖 — поведение модели.

### F-01. Доступ под контролем пользователя — частично

Ассистент работает под аккаунтом пользователя только в выбранном проекте и с правами, отмеченными на странице
«Ассистенты»; у каждого ассистента свой доступ. Сценарии: [F-01-access.feature](04-examples/F-01-access.feature).

| Правило | Покрытие |
| --- | --- |
| R1 Без доступа — только подсказка | ✅ `assistant_without_grant_sees_only_setup_hint`, `setup_hint_opens_the_assistant_tab` |
| R2 По умолчанию — просмотр и операции | ✅ `default_grant_is_view_and_transactions`, `directory_tools_need_their_right_and_are_hidden_without_it` |
| R3 Права не выше роли (инвариант) | ✅ `viewer_role_limits_grant_and_calls`, `viewer_cannot_write_directories` |
| R4 Нельзя удалить или архивировать (инвариант) | ✅ `every_tool_except_context_needs_a_right_and_none_deletes`, `update_never_archives_and_changes_only_passed_fields` |
| R5 Права меняет только человек (инвариант) | ✅ `every_tool_except_context_needs_a_right_and_none_deletes` (по списку инструментов) |
| R6 2FA проекта | ✅ `project_with_2fa_needs_grant_saved_with_second_factor`, `token_without_mfa_stops_working_when_project_requires_2fa` |
| R7 Свой доступ у каждого ассистента | ✅ `each_assistant_has_its_own_grant`, `left_project_and_revoked_grant_stop_access` |
| R8 Данные — не команды (инвариант) | 🤖 |

Известная ошибка: доступ без внешних ключей (H6, `debt` №34). Вопрос Q-04.

### F-02. Знание бюджета перед действием — частично

Инструкция сервера требует сначала получить контекст; `get_context` отдаёт всё для записи без вопросов.
Сценарии: [F-02-context.feature](04-examples/F-02-context.feature).
R1 Контекст до первой записи — 🤖; R2 Состав контекста — ✅ `context_lists_currencies_and_all_permissions`.

### F-03. Понимание названий на любом языке — есть

Сценарии: [F-03-names.feature](04-examples/F-03-names.feature).
R1 Код или название на ru/en/th — ✅ `references_by_name_in_ru_en_th`, `amounts_dates_and_lookup`;
R2 Неоднозначное не угадывается — ✅ `ambiguous_or_unknown_category_asks_for_a_code`.

### F-04. Расход и доход за секунды — частично

Сценарии: [F-04-expense-income.feature](04-examples/F-04-expense-income.feature).

| Правило | Покрытие |
| --- | --- |
| R1 Сумма + категория или позиции | ✅ `expense_with_items_and_conflicting_arguments` |
| R2 Предупреждение о счёте не в базовой валюте | ❌ |
| R3 Нет курса — предупреждение | ✅ `expense_without_rate_is_flagged` |
| R4 Остаток лимита после расхода | ❌ |
| R5 Дата по умолчанию и формат | ✅ `with_date`, `amounts_dates_and_lookup` |
| R6 Доход — только в категорию дохода | ❌ |

Горячая точка H4 — «сегодня» по поясу сервера. Вопрос Q-05.

### F-05. Чек по фото — частично

Сценарии: [F-05-receipt.feature](04-examples/F-05-receipt.feature).
R1 Позиции, количество × цена — ✅ `receipt_items_with_prices_and_edit_by_id`; R2 Скидка на чек — ✅ тот же тест;
R3 Итог совпадает с чеком — 🤖 (качество распознавания).

### F-06. Без дублей с SMS банка — частично

Сценарии: [F-06-bank-sms.feature](04-examples/F-06-bank-sms.feature).
R1 Пометка операций из SMS и фильтр — ✅ `sms_transactions_are_marked_and_filtered`;
R2 Дополнить, а не дублировать — 🤖, держится только на инструкции (H2). Вопросы Q-03, Q-06.

### F-07. Место и «для кого» — частично

Сценарии: [F-07-place-recipient.feature](04-examples/F-07-place-recipient.feature).
R1 Место подставляет категорию и счёт — ✅ `place_fills_category_and_account`;
R2 Место и человек только с согласия — 🤖 (сервер не создаёт сам, но решение звать `add_place` — за моделью);
R3 По умолчанию для всех — ✅ `recipients_in_transactions`.

### F-08. Перевод и обмен валют — частично

Сценарии: [F-08-transfer.feature](04-examples/F-08-transfer.feature).
R1 Перевод одной валюты — ✅ `transfer_and_scheduled_payment`; R2 Обмен с полученной суммой — ❌; R3 Комиссия — ❌.

### F-09. Исправление операций — частично

Сценарии: [F-09-edit.feature](04-examples/F-09-edit.feature).
R1 Только переданные поля — ✅ `update_changes_only_passed_fields`; R2 Подтверждение — 🤖 (сервер только ставит
`destructiveHint`, ✅ `reading_tools_are_read_only_and_edits_ask_for_confirmation`); R3 Позиции по id — ✅
`receipt_items_with_prices_and_edit_by_id`; R4 Начальный остаток не правится — ❌. Вопрос Q-07.

### F-10. Плановые платежи — частично

Сценарии: [F-10-planned.feature](04-examples/F-10-planned.feature).
R1 Оплата только ожидаемого вхождения — ✅ оплата (`transfer_and_scheduled_payment`), ❌ отказы (повтор, чужая дата); R2 Сумма и счёт могут отличаться — ❌;
R3 Пропуск — ✅ `transfer_and_scheduled_payment`; R4 Категория / счёт назначения — ✅
`plans_are_added_paused_and_validated`; R5 Смена правила дат — ✅ в фиче `planning` (Ц4); R6 Оплата планового
обмена — ❌ **не сделано** (H3, `debt` №33).

### F-11. Справочники через разговор — частично

Сценарии: [F-11-directories.feature](04-examples/F-11-directories.feature).
R1 Своё право — ✅ `directory_tools_need_their_right_and_are_hidden_without_it`, `narrow_token_opens_only_its_directory`;
R2 Формат кода — ✅ `account_with_currency_by_name_and_opening_balance_is_audited`; R3 Начальный остаток — ✅ `opening_balance_on_update_requires_transactions_write`;
R4 Валюта счёта с операциями — ❌; R5 Вид подкатегории — ✅ `update_never_archives_and_changes_only_passed_fields`;
R6 Не архивирует (инвариант) — ✅ тот же тест; R7 Места и люди без перевода, не ALL — ✅ `people_directory_tools`.
Известная ошибка: счёт и остаток — два вызова, не одна транзакция (`debt` №18).

### F-12. Курсы валют — есть

Сценарии: [F-12-rates.feature](04-examples/F-12-rates.feature). R1–R3 — ✅
`rate_is_upserted_by_currency_and_date_and_names_the_base_currency`, `rate_can_be_set_as_inverse_rate`.

### F-13. Лимиты — есть

Сценарии: [F-13-limits.feature](04-examples/F-13-limits.feature). R1–R4 — ✅
`limit_is_upserted_by_category_period_and_valid_from`, `zero_limit_and_removed_limit`, `period_start_of_week_month_year`.

### F-14. Ответы о деньгах — частично

Сценарии: [F-14-answers.feature](04-examples/F-14-answers.feature).
R1 Итоги в базовой валюте — ❌ в `mcp` (считает фича `budget`); R2 Фильтр с поддеревом — ✅
`list_filters_by_category_subtree_and_text`; R3 Страницы подряд — ❌ **ошибка** (H1: фейк в тестах её маскирует);
R4 Признак обрезки — ❌ **не сделано** (H7); R5 Остаток на конец дня — ❌.

### F-15. Журнал действий ассистента — частично

Сценарии: [F-15-audit.feature](04-examples/F-15-audit.feature).
R1 Каждое изменение в журнале — ✅ `expense_by_name_in_any_language_uses_default_account_and_is_audited`,
`account_with_currency_by_name_and_opening_balance_is_audited`; R2 Чтение не пишется — ❌; R3 id доступа или токена — ❌
**не сделано** (H9).

### F-16. Записи на трёх языках — частично

Сценарии: [F-16-languages.feature](04-examples/F-16-languages.feature).
R1 Описание + два перевода — 🤖; R2 Названия справочников на трёх языках — ❌; R3 Ошибки на языке пользователя — ❌
(нет теста на тайский). Итоги инструментов — английские (H8, вопрос Q-11).

### F-17. Удаление с отменой — нет · F-18. Импорт выписки банка — нет · F-19. Итоги месяца — нет

Правила — из плана (`docs/mcp-plan.md`, разделы 2–4 и 7). Сценарии: [F-17](04-examples/F-17-delete-undo.feature),
[F-18](04-examples/F-18-statement-import.feature), [F-19](04-examples/F-19-month-summary.feature). Все три заблокированы
вопросами Q-08, Q-09, Q-10.
