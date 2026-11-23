# 05. Реестр фич

**Как считать покрытие.** Правило покрыто, если его проверяет автотест, тело которого прочитано: в
`crates/features/mcp/src/tests.rs` или в тестах фичи, которую зовёт MCP (`transactions`, `planning`, `budget`,
`accounts`, `people`). Тест в другой фиче засчитан, но отмечен: MCP-слой там подменён фейком, и ошибка
адаптера `crates/app/src/wiring/wire_mcp.rs` его не видна. Сценарии `04-examples/*.feature` — спецификация,
прогона Gherkin нет (Q-03). 🤖 — поведение модели, сервером не проверить. Команда: `cargo test --workspace`.

| ID | Фича | Ценность | Актор | Цель | Компоненты | Правила | Статус | Покрытие | Приоритет | Вопросы | Источники |
| --- | --- | --- | --- | --- | --- | --- | --- | --- | --- | --- | --- |
| F-01 | Доступ под контролем пользователя | Ассистент делает только разрешённое, без удаления | участник, VIEWER, ассистент, Rauthy | C1 | `crate: mcp (страница «Ассистенты», adapters/web.rs)`, `crate: mcp (auth-слой, adapters/http.rs)`, `crate: mcp (service.rs check, visible)`, `table: fb_mcp_grants` | R1–R8 | частично | 10 сцен.; 7/8; без теста R8 🤖 | MVP | Q-04 | docs/mcp.md «Права»; docs/mcp-plan.md 2–3; tech-debt №34 |
| F-02 | Знание бюджета перед действием | Записи в нужные счета и категории | ассистент | C2 | `prompt: INSTRUCTIONS`, `tool: get_context` | R1–R2 | частично | 3 сцен.; 0/2; без теста R1 🤖, R2 | MVP | Q-03 | adapters/server.rs:21-43, 77; service.rs:471-530 |
| F-03 | Понимание названий на любом языке | «Кофе», «Food», «รถ» находят запись без кодов | участник, ассистент | C3 | `crate: mcp (domain.rs resolve)`, `crate: kernel (search)` | R1–R2 | есть | 3 сцен.; 2/2 | MVP | — | docs/mcp.md «Соглашения»; domain.rs:840-861 |
| F-04 | Расход и доход за секунды | Трата с голоса и сразу остаток лимита | участник | C4 | `tool: add_expense`, `tool: add_income`, `crate: transactions` | R1–R6 | частично | 11 сцен.; 5/6; без теста R2 | MVP | Q-05 | adapters/server.rs:138-174; service.rs:754-830; stage-0.md реш. 3 |
| F-05 | Чек по фото | Чек позициями без ручного ввода | участник | C5 | `tool: add_expense (items)`, `crate: transactions` | R1–R3 | частично | 3 сцен.; 2/3; без теста R3 🤖 | MVP | Q-03 | adapters/server.rs:138; docs/group-1.md |
| F-06 | Без дублей с SMS банка | Покупка из банка не записывается второй раз | участник, банк | C7 | `prompt: INSTRUCTIONS (SMS)`, `tool: list_transactions (from_sms)`, `tool: update_transaction` | R1–R2 | частично | 5 сцен.; 1/2; без теста R2 🤖 | MVP | Q-03, Q-06 | adapters/server.rs:37-42; docs/external-refs.md |
| F-07 | Место и «для кого» | Точные отчёты по местам и людям | участник | C6 | `tool: add_expense (place, for)`, `tool: list_places`, `tool: list_people`, `crate: places`, `crate: people` | R1–R3 | есть | 6 сцен.; 3/3 | MVP | — | adapters/server.rs:24-29; service.rs:758-767 |
| F-08 | Перевод и обмен валют | Перевод и обмен — одна запись | участник | C8 | `tool: add_transfer`, `crate: transactions` | R1–R3 | есть | 4 сцен.; 3/3 | MVP | — | adapters/server.rs:176; service.rs:883 |
| F-09 | Исправление операций | Ошибку можно поправить словами | участник | C9 | `tool: update_transaction`, `crate: transactions` | R1–R4 | частично | 4 сцен.; 2/4; без теста R2 🤖, R4 | MVP | Q-07 | adapters/server.rs:195; service.rs:894-995 |
| F-10 | Плановые платежи | Регулярные платежи — одной фразой | участник | C10 | `tool: list_upcoming_payments`, `tool: pay_planned`, `tool: skip_planned`, `tool: list_plans, add_plan, update_plan`, `crate: planning` | R1–R6 | частично | 8 сцен.; 5/6; без теста R6 | MVP | — | adapters/server.rs:214-297; stage-0.md реш. 4–5; tech-debt №33 |
| F-11 | Справочники через разговор | Валюты, счета, категории без формы | участник (OWNER, EDITOR) | C11 | `tool: add_/update_currency`, `tool: add_/update_account`, `tool: add_/update_category`, `tool: add_/update_place`, `tool: add_/update_person` | R1–R7 | частично | 7 сцен.; 7/7 | MVP | — | adapters/server.rs:299-513; tech-debt №18 |
| F-12 | Курсы валют | Верные итоги при любом направлении курса | участник | C12 | `tool: list_rates`, `tool: set_rate`, `crate: rates` | R1–R3 | есть | 5 сцен.; 3/3 | MVP | — | adapters/server.rs:515-545; service.rs:1376-1423 |
| F-13 | Лимиты | Ограничить траты словами | участник | C13 | `tool: list_limits`, `tool: set_limit`, `crate: limits`, `crate: budget` | R1–R4 | есть | 5 сцен.; 4/4 | MVP | — | adapters/server.rs:547-573; stage-0.md реш. 1–2 |
| F-14 | Ответы о деньгах | Ответ «сколько потратили» без приложения | участник | C14 | `tool: get_budget`, `tool: get_balances`, `tool: list_transactions`, `crate: budget` | R1–R5 | частично | 5 сцен.; 2/5; без теста R3, R4, R5 | MVP | — | adapters/server.rs:86-123; tech-debt №4, №25 |
| F-15 | Журнал действий ассистента | Видно, что и когда изменил ассистент | участник, журнал | C16 | `crate: mcp (service.rs audit_write)`, `table: fb_audit_log` | R1–R3 | частично | 3 сцен.; 1/3; без теста R2, R3 | следующий релиз | — | service.rs:442-446; tech-debt №49 |
| F-16 | Записи на трёх языках | Каждый читает записи на своём языке | участники семьи | C15 | `prompt: INSTRUCTIONS (языки)`, `tool: add_expense (translations)` | R1–R3 | частично | 3 сцен.; 0/3; без теста R1 🤖, R2, R3 | MVP | Q-11 | adapters/server.rs:30-32; CLAUDE.md «Три языка» |
| F-17 | Удаление с отменой | Убрать лишнее и вернуть случайное | участник | G1 | — | R1–R2 | нет | 2 сцен.; не сделано | потом | Q-08 | docs/mcp-plan.md 2–4 |
| F-18 | Импорт выписки банка | Месяц операций одним файлом без дублей | участник, банк | G2 | — | R1–R1 | нет | 1 сцен.; не сделано | потом | Q-09 | docs/mcp-plan.md 7, этап 5 |
| F-19 | Итоги месяца | Ассистент сам подводит итоги месяца | участник | G3 | — | R1–R1 | нет | 1 сцен.; не сделано | потом | Q-10 | docs/mcp-plan.md 7, этап 5 |

**Итого:** 19 фич: есть — 5, частично — 11, нет — 3, расходится — 0. Правил в сделанных фичах — 64, покрыто автотестами — 47 (73 %), из них в тестах других фич — 16; 🤖 — 6.

## Карточки фич

✅ — есть автотест (тело прочитано), ❌ — нет, 🤖 — поведение модели.

### F-01. Доступ под контролем пользователя — частично

Ассистент делает только разрешённое, без удаления. Сценарии: [F-01-access.feature](04-examples/F-01-access.feature). Вопросы: Q-04. Известная проблема: Доступ остаётся в базе после исключения.

| Правило | Покрытие | Тест | Замечание |
| --- | --- | --- | --- |
| R1 Без доступа — только подсказка, где его открыть | ✅ | `assistant_without_grant_sees_only_setup_hint` — `crates/features/mcp/src/tests.rs:752` | скрытыми проверены 2 инструмента, не весь список |
| R2 По умолчанию — просмотр и операции | ✅ | `directory_tools_need_their_right_and_are_hidden_without_it` — `crates/features/mcp/src/tests.rs:1172` | права передаются явно; флажки страницы по умолчанию не проверяются |
| R3 Права ассистента не выше роли (инвариант) | ✅ | `viewer_role_limits_grant_and_calls` — `crates/features/mcp/src/tests.rs:792` |  |
| R4 Ничего нельзя удалить или архивировать (инвариант) | ✅ | `every_tool_except_context_needs_a_right_and_none_deletes` — `crates/features/mcp/src/tests.rs:2067` | по именам инструментов; архив — update_never_archives_and_changes_only_passed_fields |
| R5 Права меняет только человек в браузере (инвариант) | ✅ | `every_tool_except_context_needs_a_right_and_none_deletes` — `crates/features/mcp/src/tests.rs:2067` | только по именам инструментов |
| R6 Проект с 2FA — доступ после второго фактора | ✅ | `project_with_2fa_needs_grant_saved_with_second_factor` — `crates/features/mcp/src/tests.rs:810` |  |
| R7 У каждого ассистента свой доступ | ✅ | `each_assistant_has_its_own_grant` — `crates/features/mcp/src/tests.rs:842` |  |
| R8 Тексты из данных — не команды (инвариант) | 🤖 | — |  |

### F-02. Знание бюджета перед действием — частично

Записи в нужные счета и категории. Сценарии: [F-02-context.feature](04-examples/F-02-context.feature). Вопросы: Q-03.

| Правило | Покрытие | Тест | Замечание |
| --- | --- | --- | --- |
| R1 Контекст — до первой записи в разговоре | 🤖 | — |  |
| R2 Контекст содержит всё для записи | ❌ | — | context_lists_currencies_and_all_permissions (tests.rs:2001) проверяет только валюты и права; базовая валюта, язык, дата, остатки лимитов, частые категории — без проверки |

### F-03. Понимание названий на любом языке — есть

«Кофе», «Food», «รถ» находят запись без кодов. Сценарии: [F-03-names.feature](04-examples/F-03-names.feature). Вопросы: нет.

| Правило | Покрытие | Тест | Замечание |
| --- | --- | --- | --- |
| R1 Поиск по коду или названию на ru/en/th | ✅ | `amounts_dates_and_lookup` — `crates/features/mcp/src/tests.rs:2079` | сквозной вариант — references_by_name_in_ru_en_th |
| R2 Неоднозначное название не угадывается | ✅ | `ambiguous_or_unknown_category_asks_for_a_code` — `crates/features/mcp/src/tests.rs:977` |  |

### F-04. Расход и доход за секунды — частично

Трата с голоса и сразу остаток лимита. Сценарии: [F-04-expense-income.feature](04-examples/F-04-expense-income.feature). Вопросы: Q-05. Известная проблема: «Сегодня» считается по поясу сервера.

| Правило | Покрытие | Тест | Замечание |
| --- | --- | --- | --- |
| R1 Сумма + категория или позиции, не то и другое | ✅ | `expense_with_items_and_conflicting_arguments` — `crates/features/mcp/src/tests.rs:992` |  |
| R2 Счёт не в базовой валюте — предупреждение | ❌ | — | предупреждение есть (service.rs:798), тест его не вызывает |
| R3 Нет курса — записать и предупредить | ✅ | `expense_without_rate_is_flagged` — `crates/features/mcp/src/tests.rs:2099` |  |
| R4 После расхода — остаток лимита | ✅ | `expense_by_name_in_any_language_uses_default_account_and_is_audited` — `crates/features/mcp/src/tests.rs:953` | слабо: только наличие фразы, значение из фейка; расчёт — budget month_limits_and_rates_for_quick_expense |
| R5 Дата по умолчанию — сегодня, формат ГГГГ-ММ-ДД | ✅ | `amounts_dates_and_lookup` — `crates/features/mcp/src/tests.rs:2079` | отказ на неверную дату — expense_with_items_and_conflicting_arguments:1018 |
| R6 Доход — только в категорию дохода | ✅ | `category_kind_must_match` — `crates/features/transactions/src/tests.rs:303` | в mcp add_income не вызывается |

### F-05. Чек по фото — частично

Чек позициями без ручного ввода. Сценарии: [F-05-receipt.feature](04-examples/F-05-receipt.feature). Вопросы: Q-03.

| Правило | Покрытие | Тест | Замечание |
| --- | --- | --- | --- |
| R1 Позиция: категория, количество × цена или сумма | ✅ | `receipt_with_prices` — `crates/features/transactions/src/tests.rs:686` | тест mcp только передаёт поля, фейк не считает |
| R2 Скидка на чек делится пропорционально | ✅ | `receipt_discount_is_split` — `crates/features/transactions/src/domain.rs:1192` |  |
| R3 Итог операции совпадает с итогом чека | 🤖 | — |  |

### F-06. Без дублей с SMS банка — частично

Покупка из банка не записывается второй раз. Сценарии: [F-06-bank-sms.feature](04-examples/F-06-bank-sms.feature). Вопросы: Q-03, Q-06. Известная проблема: Дубль ловит только инструкция модели.

| Правило | Покрытие | Тест | Замечание |
| --- | --- | --- | --- |
| R1 Операция из SMS помечена, категория — заглушка | ✅ | `sms_transactions_are_marked_and_filtered` — `crates/features/mcp/src/tests.rs:2457` | заглушка категории не проверяется |
| R2 Дополнить операцию из SMS, а не дублировать | 🤖 | — |  |

### F-07. Место и «для кого» — есть

Точные отчёты по местам и людям. Сценарии: [F-07-place-recipient.feature](04-examples/F-07-place-recipient.feature). Вопросы: нет.

| Правило | Покрытие | Тест | Замечание |
| --- | --- | --- | --- |
| R1 Место подставляет категорию и счёт | ✅ | `place_fills_category_and_account` — `crates/features/mcp/src/tests.rs:2115` | наследование от сети не проверено |
| R2 Место и человек — только с согласия (инвариант) | ✅ | `place_fills_category_and_account` — `crates/features/mcp/src/tests.rs:2115` | люди — recipients_in_transactions:2339 |
| R3 По умолчанию покупка — для всех | ✅ | `recipients_in_transactions` — `crates/features/mcp/src/tests.rs:2292` |  |

### F-08. Перевод и обмен валют — есть

Перевод и обмен — одна запись. Сценарии: [F-08-transfer.feature](04-examples/F-08-transfer.feature). Вопросы: нет.

| Правило | Покрытие | Тест | Замечание |
| --- | --- | --- | --- |
| R1 Перевод одной валюты — одна операция | ✅ | `transfer_rules_and_type_change` — `crates/features/transactions/src/tests.rs:440` | тест mcp проверяет только счёт назначения |
| R2 Обмен — с полученной суммой | ✅ | `exchange_is_one_transaction` — `crates/features/transactions/src/tests.rs:416` |  |
| R3 Комиссия — часть суммы с категорией | ✅ | `exchange_with_fee` — `crates/features/transactions/src/domain.rs:1338` |  |

### F-09. Исправление операций — частично

Ошибку можно поправить словами. Сценарии: [F-09-edit.feature](04-examples/F-09-edit.feature). Вопросы: Q-07.

| Правило | Покрытие | Тест | Замечание |
| --- | --- | --- | --- |
| R1 Меняются только переданные поля | ✅ | `update_changes_only_passed_fields` — `crates/features/mcp/src/tests.rs:1029` | счёт и дата не проверяются |
| R2 Перед правкой — подтверждение пользователя | 🤖 | — | сервер ставит destructiveHint — reading_tools_are_read_only_and_edits_ask_for_confirmation |
| R3 Позиции чека правятся по id | ✅ | `items_are_edited_by_id` — `crates/features/transactions/src/tests.rs:713` |  |
| R4 Начальный остаток не правится как операция | ❌ | — | отказ есть (service.rs:899-900), теста нет |

### F-10. Плановые платежи — частично

Регулярные платежи — одной фразой. Сценарии: [F-10-planned.feature](04-examples/F-10-planned.feature). Вопросы: нет. Известная проблема: Плановый обмен валют не оплатить.

| Правило | Покрытие | Тест | Замечание |
| --- | --- | --- | --- |
| R1 Оплатить только ожидаемое вхождение плана | ✅ | `unknown_plan_occurrence_is_rejected` — `crates/features/transactions/src/tests.rs:585` | в mcp — только удачная оплата |
| R2 Сумма и счёт оплаты могут отличаться | ✅ | `calendar_statuses` — `crates/features/planning/src/tests.rs:108` | только сумма; счёт не проверяется |
| R3 Пропущенное вхождение больше не ожидается | ✅ | `calendar_statuses` — `crates/features/planning/src/tests.rs:108` |  |
| R4 Расход/доход — категория, перевод — счёт | ✅ | `transfer_plan` — `crates/features/planning/src/tests.rs:148` |  |
| R5 Новое правило дат не теряет прошлые оплаты | ✅ | `rule_change_keeps_payment_history` — `crates/features/planning/src/tests.rs:217` |  |
| R6 Плановый обмен оплачивается с суммой | ❌ | — | не сделано: в PayPlannedArgs нет to_amount |

### F-11. Справочники через разговор — частично

Валюты, счета, категории без формы. Сценарии: [F-11-directories.feature](04-examples/F-11-directories.feature). Вопросы: нет. Известная проблема: Счёт и остаток — не одна транзакция.

| Правило | Покрытие | Тест | Замечание |
| --- | --- | --- | --- |
| R1 Каждый справочник — своё право | ✅ | `directory_tools_need_their_right_and_are_hidden_without_it` — `crates/features/mcp/src/tests.rs:1172` |  |
| R2 Код — UPPER_SNAKE, уникальный | ✅ | `account_with_currency_by_name_and_opening_balance_is_audited` — `crates/features/mcp/src/tests.rs:1388` |  |
| R3 Начальный остаток — ещё право на операции | ✅ | `narrow_token_opens_only_its_directory` — `crates/features/mcp/src/tests.rs:1352` | при изменении — opening_balance_on_update_requires_transactions_write |
| R4 Валюту счёта с операциями не сменить | ✅ | `balance_rules` — `crates/features/accounts/src/tests.rs:73` | в mcp теста нет |
| R5 Подкатегория — того же вида, что родитель | ✅ | `update_never_archives_and_changes_only_passed_fields` — `crates/features/mcp/src/tests.rs:1758` |  |
| R6 Изменение никогда не архивирует (инвариант) | ✅ | `update_never_archives_and_changes_only_passed_fields` — `crates/features/mcp/src/tests.rs:1758` |  |
| R7 Места и люди без перевода, код не ALL | ✅ | `member_codes` — `crates/features/people/src/domain.rs:167` | только код ALL; «без перевода» не проверяется |

### F-12. Курсы валют — есть

Верные итоги при любом направлении курса. Сценарии: [F-12-rates.feature](04-examples/F-12-rates.feature). Вопросы: нет.

| Правило | Покрытие | Тест | Замечание |
| --- | --- | --- | --- |
| R1 Курс — ровно одно число: прямое или обратное | ✅ | `rate_can_be_set_as_inverse_rate` — `crates/features/mcp/src/tests.rs:1534` |  |
| R2 Курс той же даты заменяет прежний | ✅ | `rate_is_upserted_by_currency_and_date_and_names_the_base_currency` — `crates/features/mcp/src/tests.rs:1498` |  |
| R3 Отличие больше 5 раз — предупреждение | ✅ | `rate_is_upserted_by_currency_and_date_and_names_the_base_currency` — `crates/features/mcp/src/tests.rs:1498` | граница «ровно 5 раз» не проверена |

### F-13. Лимиты — есть

Ограничить траты словами. Сценарии: [F-13-limits.feature](04-examples/F-13-limits.feature). Вопросы: нет.

| Правило | Покрытие | Тест | Замечание |
| --- | --- | --- | --- |
| R1 Лимит на весь период, с подкатегориями | ✅ | `limit_is_upserted_by_category_period_and_valid_from` — `crates/features/mcp/src/tests.rs:1602` | подкатегории — budget tree_rollup_and_available_from_ancestor |
| R2 Ноль — настоящий нулевой лимит | ✅ | `zero_limit_means_no_spending` — `crates/features/budget/src/domain.rs:733` |  |
| R3 Снятие с даты, прошлое сохраняется | ✅ | `zero_limit_and_removed_limit` — `crates/features/mcp/src/tests.rs:1684` | прошлое — budget removed_limit_keeps_the_past |
| R4 Одно правило категории на дату начала | ✅ | `limit_is_upserted_by_category_period_and_valid_from` — `crates/features/mcp/src/tests.rs:1602` |  |

### F-14. Ответы о деньгах — частично

Ответ «сколько потратили» без приложения. Сценарии: [F-14-answers.feature](04-examples/F-14-answers.feature). Вопросы: нет. Известная проблема: Страницы со смещением пропускают операции.

| Правило | Покрытие | Тест | Замечание |
| --- | --- | --- | --- |
| R1 Итоги — в базовой валюте, с подкатегориями | ✅ | `report_converts_currency_and_rolls_up` — `crates/features/budget/src/tests.rs:164` | в mcp — фейк бюджета |
| R2 Фильтр категории включает подкатегории | ✅ | `list_filters_by_category_subtree_and_text` — `crates/features/mcp/src/tests.rs:1104` |  |
| R3 Страницы идут подряд без пропусков | ❌ | — | ошибка: wire_mcp.rs:193; фейк tests.rs:195 её скрывает |
| R4 Обрезанный список помечен | ❌ | — | не сделано: SCAN_LIMIT service.rs:182 |
| R5 Остаток на дату — на конец дня | ❌ | — | теста нет |

### F-15. Журнал действий ассистента — частично

Видно, что и когда изменил ассистент. Сценарии: [F-15-audit.feature](04-examples/F-15-audit.feature). Вопросы: нет. Известная проблема: Нет id доступа и токена в событии.

| Правило | Покрытие | Тест | Замечание |
| --- | --- | --- | --- |
| R1 Каждое изменение — событие журнала | ✅ | `account_with_currency_by_name_and_opening_balance_is_audited` — `crates/features/mcp/src/tests.rs:1388` | id операции в событии не проверяется |
| R2 Чтение в журнал не пишется | ❌ | — | теста нет |
| R3 В событии — id доступа или токена | ❌ | — | не сделано: service.rs:442-445 |

### F-16. Записи на трёх языках — частично

Каждый читает записи на своём языке. Сценарии: [F-16-languages.feature](04-examples/F-16-languages.feature). Вопросы: Q-11. Известная проблема: Итоги и ошибки инструментов по-английски.

| Правило | Покрытие | Тест | Замечание |
| --- | --- | --- | --- |
| R1 Описание на языке пользователя + 2 перевода | 🤖 | — | сервер: description_lang — expense_by_name…:967; переводы пишет модель |
| R2 Названия справочников — на трёх языках | ❌ | — | сервер не требует всех трёх |
| R3 Ошибки — на языке пользователя | ❌ | — | ошибки инструментов MCP — английские строки; тайский есть только в ошибках домена |

### F-17. Удаление с отменой — нет

Убрать лишнее и вернуть случайное. Сценарии: [F-17-delete-undo.feature](04-examples/F-17-delete-undo.feature). Вопросы: Q-08. Известная проблема: Не сделано: решение не принято.

| Правило | Покрытие | Тест | Замечание |
| --- | --- | --- | --- |
| R1 Удаление — отдельное право + право записи (инвариант) | ❌ | — |  |
| R2 Отмена кодом 2 минуты, содержимое в журнале | ❌ | — |  |

### F-18. Импорт выписки банка — нет

Месяц операций одним файлом без дублей. Сценарии: [F-18-statement-import.feature](04-examples/F-18-statement-import.feature). Вопросы: Q-09. Известная проблема: Не сделано: нет ТЗ.

| Правило | Покрытие | Тест | Замечание |
| --- | --- | --- | --- |
| R1 Записанная операция не создаётся второй раз | ❌ | — |  |

### F-19. Итоги месяца — нет

Ассистент сам подводит итоги месяца. Сценарии: [F-19-month-summary.feature](04-examples/F-19-month-summary.feature). Вопросы: Q-10. Известная проблема: Не сделано: состав итогов не описан.

| Правило | Покрытие | Тест | Замечание |
| --- | --- | --- | --- |
| R1 Цифры итогов совпадают с «Бюджетом» | ❌ | — |  |
