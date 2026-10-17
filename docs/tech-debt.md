# Технический долг — family-budget (17.10.2026, коммит 140fde9)

Состояние: ~31 тыс. строк Rust в 13 крейтах; `cargo test --workspace` — 219 тестов, все зелёные; clippy — 16 предупреждений;
`cargo fmt --check` — 1575 расхождений (131 из 152 файлов); TODO/FIXME нет; `unwrap`/`panic` в рабочем коде нет
(только memory-адаптеры и тесты); `f64` для денег не используется.

Серьёзность: 🔴 высокая · 🟡 средняя · ⚪ низкая. Трудоёмкость: S — до дня, M — несколько дней, L — неделя и больше.

## 🔴 Высокая

| № | Область | Где | Проблема и риск | Тр. | Что сделать |
|---|---|---|---|---|---|
| 1 | Безопасность: вход | `kernel/src/config.rs:91-94,119`; `compose.yaml:98` | Без `APP_ENV` включается режим Dev, а с ним «Вход для разработки» — под **любым** email без пароля, даже при настроенном OIDC. Подпись — известным из репозитория `DEV_SECRET`. На проде защищает только строка `APP_ENV=prod` в `.env` | S | Не стартовать без явного `APP_ENV`; dev-вход — только без OIDC и только для `demo:*`; убрать `:-dev` из compose |
| 2 | Безопасность: 2FA | `projects/src/service.rs:824`; `mcp/src/service.rs:184-202` | У API-токена всегда `mfa: true`. «Требовать 2FA» не отзывает старые (и бессрочные) токены; `token_caller` в MCP не проверяет `require_2fa` | S | Хранить `mfa` в `fb_api_tokens`; при включении 2FA отзывать токены без MFA; тест |
| 3 | Двойная запись | `transactions/src/service.rs:333,351-360` | Категория из прежней версии транзакции не проверяется по типу: можно превратить доход в расход на категории дохода, и итоги бюджета разъедутся | S | Для «старых» ссылок пропускать только проверку архива, тип проверять всегда; тест |
| 4 | MCP: пагинация | `app/src/wiring/wire_mcp.rs:90-94` | `page = offset / size`: при `limit=50, offset=30` возвращаются строки 0..50. Ассистент пропускает или дублирует операции; фейк в `mcp/tests.rs:94` баг маскирует | S | Передавать настоящие `limit/offset` в SQL; тест на сам адаптер |
| 5 | MCP: поиск категорий | `wire_transactions.rs:80-84`; `mcp/src/service.rs:432,1447`; `mcp/src/tests.rs:1105` | Названия склеиваются через пробел, а поиск делит строку по `\u{1f}`: точное имя на другом языке («Food», «รถ») даёт «неоднозначно». Тест закрепляет это поведение | S | Склеивать через `\u{1f}`, исправить тест, оставить один механизм поиска |
| 6 | Справочники: REST | `categories/src/service.rs:51-57`; `Update*Dto.archived` в currencies/accounts/categories | `PUT` с `archived` обходит правила архива (активные подкатегории, архивный родитель); `PUT` без поля молча снимает архив | S | Убрать `archived` из `Update*Dto`, архив — только через `archive`/`restore` |
| 7 | Коды записей | `kernel/src/code.rs:5-14`; ссылки в `*/list.html`, `*/form.html` | Код может содержать `/ ? # &` и кириллицу и подставляется в URL без кодирования: запись `FOOD/CAFE` нельзя открыть, изменить или архивировать | S | `[A-Z0-9_]{1,40}` в `normalize` и `pattern` в форме, `urlencode` в шаблонах |
| 8 | CSS | `styles/app.css:336-340`; `tailwind.config.cjs` | Классы `.indent-*` собираются динамически (`indent-{{ depth }}`), и Tailwind их вырезает: в `static/app.css` их нет. Дерево категорий и лимитов без отступов | S | `safelist: [{pattern: /^indent-/}]`, `make css` |
| 9 | i18n ошибок | `kernel/src/{error,code,web,auth}.rs`, `service.rs`/`domain.rs` всех фич | Все сообщения валидации и конфликтов — русские строки, en/th видят их по-русски (нарушает CLAUDE.md); тесты этого не ловят. В projects и mcp нет теста `templates_are_translated` | L | `AppError::Validation { field, key, args }`, перевод через `t()`; тест на кириллицу в литералах ошибок; добавить недостающие тесты |
| 10 | Тесты: PostgreSQL | все `adapters/postgres.rs` (~1900 строк SQL); `rates/src/tests.rs:219`; `ci.yml` | Нет ни одного выполняемого теста на БД: единственный пропускается без `TEST_DATABASE_URL`. Не проверены триггер баланса, миграции, маппинг wiring. Memory-адаптеры расходятся с SQL (`count_first`, `purge_sessions`, уникальность курсов, порядок) | M | CI-джоб с `postgres:16`; общий контрактный набор тестов для memory и PG |
| 11 | Тесты: middleware и веб | `kernel/src/auth.rs:426-569`, `web.rs:253-296`; `*/adapters/{web,http}.rs`; `crates/app` | CSRF, роли, блок 2FA, редиректы, хендлеры форм не покрыты; у `crates/app` тестов нет | M | `tower::ServiceExt::oneshot` на роутеры с фейками; smoke-тест сборки wiring |
| 12 | Деплой | `renovate.json`; `compose.yaml:10,33`; `scripts/deploy-vds.sh:723-833` | Renovate может поднять major PostgreSQL, а автообновление (`git pull` + `compose up`) применит его на старом томе: прод не стартует. Rauthy обновляется без проверки | S | Запрет или approval на major postgres и Rauthy; в `update` не трогать db/rauthy без флага |
| 13 | Бэкапы | `docs/deploy-vds.md:707-710`; `deploy-vds.sh:566,773` | Внешние копии (`rsync --ignore-existing`) не удаляются никогда — нарушается обещание «30 дней» в политике данных. Об ошибке бэкапа никто не узнаёт | S | `rclone sync`/`--delete` с той же ротацией; `notify` при ошибке; проверка свежести копии |
| 14 | Восстановление | `scripts/restore-check.sh` | `sh` без `pipefail` и `trap`: при сбое остаётся незашифрованная БД. База rauthy не проверяется; копия выбирается по mtime | M | bash, pipefail, trap с DROP; проверять обе базы; запуск по расписанию вне прода |

## 🟡 Средняя

| № | Область | Где | Проблема и риск | Тр. | Что сделать |
|---|---|---|---|---|---|
| 15 | CI: линтеры | `ci.yml`; нет `rustfmt.toml`, `[workspace.lints]` | В CI нет `fmt` и `clippy`; 1575 расхождений fmt, 16 предупреждений clippy | S (+M) | `rustfmt.toml` с нужным `max_width`, разовое форматирование отдельным PR, `clippy -D warnings` в CI |
| 16 | CI: статика | `Makefile:65`; `crates/app/static/*` | Собранные CSS и иконки лежат в git, CI их не сверяет — отсюда п. 8 | S | `npm run build && git diff --exit-code crates/app/static` |
| 17 | Атомарность удаления | `projects/src/service.rs:388-444,758`; `wire_projects.rs:96-137,177-186` | Удаление проекта или аккаунта идёт по фичам без общей транзакции: сбой оставляет полуудалённые данные. Создание проекта: 21 категория по одной, `Conflict` проглатывается | M | Общая транзакция или `ON DELETE CASCADE`; пакетный bootstrap |
| 18 | Атомарность счёта | `accounts/src/adapters/web.rs:152-221`; `mcp/src/service.rs:869` | Счёт и начальный остаток — два вызова из адаптера; в REST остатка нет; web, REST и MCP ведут себя по-разному | M | `create_with_opening` в сервисе, в одной транзакции |
| 19 | Гонки | `transactions/src/service.rs:143-166,196-210` | «Один OPENING на счёт» и «Сверить остаток» — чтение, потом вставка. Двойной клик удваивает остаток или корректировку | M | Частичный уникальный индекс / advisory lock / ключ идемпотентности |
| 20 | Переполнение сумм | `kernel/src/web.rs:663-684`; `transactions/src/domain.rs:209-251`; `main.rs` | `Decimal` `*`, `/`, `+` паникуют при переполнении, `CatchPanicLayer` нет; нет верхней границы суммы (`NUMERIC(18,4)` → 500) | S | `checked_*`, `MAX_AMOUNT`, `CatchPanicLayer` |
| 21 | Часовой пояс | `Dockerfile:42` (`TZ=Asia/Bangkok`); `chrono::Local` в ~15 местах | «Сегодня», текущий месяц, статус OVERDUE, даты MCP считаются по серверу | M | Пояс в профиле/проекте → `Session.tz`, `today(&Session)` |
| 22 | Курсы валют | `budget/src/domain.rs:376`; `planning/src/domain.rs:273`; `transactions/src/service.rs:228`; `budget/src/service.rs:92` | Три реализации «курса на дату»: без курса — 1:1 с пометкой, `None` или молча 1 (быстрый расход). Округление `round_dp(2)` каждой проводки, `minor_units` не учитывается | S | Одна функция в rates/kernel; округлять итог по валюте |
| 23 | Нагрузка на БД | `kernel/src/auth.rs:458,516`; `projects/src/service.rs:766-794`; `main.rs:53` | От 5 запросов на каждый запрос с авторизацией, MCP — до двух `session` на вызов; пул на 10 соединений зашит | M | Один JOIN для `SessionInfo` или кэш с TTL; пул в `Config` |
| 24 | Тяжёлые отчёты | `budget/src/service.rs:78-102,189`; `transactions/adapters/postgres.rs:292`; `rates/adapters/web.rs:132-152` | Быстрый расход строит весь бюджет (~15 запросов); поиск курса O(проводки × курсы); остатки — `SUM` по всей истории; `/rates` трижды грузит весь журнал | M | Лёгкий путь для лимитов, индекс курсов, `DISTINCT ON`, снимки остатков |
| 25 | Тихая обрезка | `transactions/adapters/web.rs:193-226` (25×200); `mcp/src/service.rs:120` (1000); limits/справочники (1000/10 000) | Данные сверх лимита молча выпадают из списков, итогов и `check_tree` | M | SQL-агрегаты и пагинация; признак `truncated` |
| 26 | Логика в адаптерах | `transactions/adapters/web.rs:162-356`; `planning/adapters/web.rs:179-203`; `wire_mcp.rs:119`; `mcp/src/service.rs:1019,1095` | Итоги, выписка и календарь считаются в хендлерах; upsert курса и лимита живёт в MCP | M | Перенести в сервисы; `INSERT … ON CONFLICT` в rates/limits |
| 27 | Дублирование справочников | `currencies`, `accounts`, `categories` (memory/postgres/http/service/web/шаблоны) | ~2,4 тыс. строк, ~70 % шаблонные; три `XNameLang` вместо `Lang`; копии уже расходятся (обработка ошибки архива) | L | `I18nName` в kernel, generic-репозиторий и сервис, макросы форм |
| 28 | Размер модулей | `mcp/src/service.rs` (1494), `projects/adapters/web.rs` (943), `kernel/src/web.rs` (829), `projects/src/service.rs` (828) | Сложно ревьюить и тестировать | M | Разбить по подобластям (tools/*, auth/tokens/invites, …) |
| 29 | Wiring | `wiring/mod.rs:306-335`; `wire_accounts.rs:59-84`; `wire_transactions.rs:118-131` | Циклы accounts↔transactions↔budget спрятаны за `OnceLock`, `let _ = set(..)`: если связка не задана — нули вместо ошибки | M | Проверка при старте; разорвать цикл read-портом |
| 30 | Исключения ролей | `kernel/src/auth.rs:558`; `kernel/src/web.rs:272` | Два разных списка путей для VIEWER и 2FA; `starts_with` без границы сегмента (`/projectsX`) | S | Одна константа, сравнение по сегменту |
| 31 | Миграции | `kernel/src/feature.rs:48-54`; `wiring/mod.rs:49-67` | Общая `_sqlx_migrations`, порядок — по регистрации фич, а не по времени; миграции читают чужие таблицы; `down.sql` не тестируются и теряют данные | M | Тест уникальности и порядка версий; up→down→up в PG-джобе или forward-only |
| 32 | Наблюдаемость | `main.rs:56-206`; `kernel/src/error.rs:92` | Нет `tracing-subscriber` — `TraceLayer` пишет в никуда; `println!`, нет request-id; логирование внутри `public_message()` | S | `tracing-subscriber` (JSON), request-id |
| 33 | MCP: плановые | `mcp/src/dto.rs:172-185`; `mcp/src/service.rs:740-749` | В `pay_planned` нет `to_amount` — плановый обмен валют не оплатить; у описания неверный язык | S | Добавить поле, передавать язык как в вебе |
| 34 | MCP: доступы | `mcp/migrations/…mcp_grants.up.sql`; `projects/src/service.rs:372-531` | У `fb_mcp_grants` нет FK; при исключении участника доступ остаётся и «оживает» при повторном добавлении | S | FK с `CASCADE`, удаление в `remove_member` |
| 35 | Лимит запросов | `kernel/src/throttle.rs:24`; `kernel/src/auth.rs:472`; `mcp/src/service.rs:174` | Лимит только после успешной аутентификации; при отравленном mutex отключается; лимит и «Отменить» — в памяти процесса | M | Лимит по IP до проверки; `PoisonError::into_inner`; хранение в БД перед масштабированием |
| 36 | Healthcheck | `Dockerfile`; `compose.yaml:88-119` | Нет healthcheck у app и rauthy; `depends_on: service_started` — вход отвечает 500, пока Rauthy поднимается | S | Подкоманда `healthcheck`, `service_healthy` |
| 37 | Секреты Rauthy | `compose.yaml:59-63`; `.env.example:64-68` | `${VAR:-dev…}` подставляет dev-ключ и при пустом значении из `.env.example` | S | `${VAR:?}` в prod; проверка в `deploy-vds.sh check` |
| 38 | ZAP | `ci.yml:114-117` | Сканируется `APP_ENV=dev` без входа — не прод и не основная поверхность атаки | M | prod-режим в CI, скан под демо-гостем |
| 39 | Пиннинг | workflows (36 из 38 `uses:` по тегу); `Dockerfile`; нет `rust-toolchain.toml` | Подмена тега action при `packages:write`; разные rustc в CI и образе | S | `pinGitHubActionDigests`, `pinDigests`, `rust-toolchain.toml` |
| 40 | Валюты | `currencies/src/domain.rs:78-92`; `service.rs:60` | `minor_units` и ISO-код проверяются только в форме; базовую валюту можно архивировать | S | Правила в домен, `CHECK` в БД, запрет архива базовой |
| 41 | Лендинг | `crates/features/site` и `site/content/{_index,privacy,terms,data-deletion}*` | Юридические тексты в двух копиях без сверки; `robots.txt` отстал (нет `/join/`, `/mcp`, `/assistants`) | M | Тест сверки до переезда; потом удалить фичу |
| 42 | Флаги фич | `crates/app/Cargo.toml`; `wiring/mod.rs:276,337` | `--no-default-features --features site` не собирается: заявленное включение и выключение фич не работает | S | Сделать projects/currencies обязательными или `cfg`; `cargo hack` в CI |
| 43 | GET с изменениями | `projects/adapters/web.rs:61,827` | `/projects/switch/{id}` и `/lang/{code}` меняют состояние через GET без CSRF | S | POST + CSRF |

## ⚪ Низкая

| № | Область | Где | Проблема | Тр. | Что сделать |
|---|---|---|---|---|---|
| 44 | Документация | `README.md` (134 теста, нет mcp), `docs/tz-v3.md`, `docs/mcp-plan.md`, `*/FEATURE.md`, `scripts/screenshot.mjs` | Разошлась с кодом | S | Обновить; не писать цифры вручную |
| 45 | Мусор | «ТЗ Семейная бухгалтерия.md» (v1); `specs/*.json` (копии `feature.json`); 28 `#[allow(dead_code…)]`; `kernel/src/web.rs` (`escape` без `'`, `money`, `nav_items`, `is_htmx`, `csrf_of`); `web/brand/overview.png` | Мёртвый код и устаревшие файлы | S | Удалить |
| 46 | «Что нового» | `site/content/releases/next/`; `release-notes.yml:48` | У PR #34 (символ валюты) нет записи; проверку проходит любая правка в `releases/` | S | Добавить запись; требовать новый файл в `next/` |
| 47 | `.dockerignore` | `.dockerignore` и `.containerignore` | С Docker в контекст сборки попадают `backups/` (дампы БД) и `site/` | S | Один белый список |
| 48 | Дубли в инфраструктуре | Makefile, `site.yml`, `ci.yml`, `docker-publish.yml`, `build-assets.mjs`/`build-site.mjs` | Версия Zola в трёх местах, тест-джоб в двух workflow, сборка спрайта ×2, `app.js` (793 строки) в каталоге артефактов; prod `clients.json` без MCP-клиента | S | Одно место для версий, reusable workflow |
| 49 | Журнал аудита | `mcp/src/service.rs:81-91,338` | В `mcp_write` нет `token_id`/`grant_id`; ошибка записи в журнал уходит в `eprintln` | S | Писать id, показывать details |
| 50 | IP за прокси | `kernel/src/auth.rs:191-193` | Берётся левый адрес `X-Forwarded-For` — подделывается клиентом | S | Правый / N-й справа |
| 51 | a11y | `kernel/templates/macros.html:5-34`; `categories/list.html`; `static/app.js:785` | `aria-describedby` на несуществующий id; дерево без `aria-level`; горячие клавиши по `e.key` (не работают в тайской раскладке) | S | Исправить; `e.code` |
| 52 | Индексы | `planning/migrations/…create_fb_plans.up.sql:27`; `transactions/adapters/postgres.rs:358` | У `fb_plan_skips` нет индекса по `project_id`; `count_first` сканирует все транзакции | S | Индексы |
| 53 | Демо-данные | `app/src/seed.rs:38-95`; `main.rs:97-110` | Демо ищется по названию «Семья»; сброс без транзакции, по таймеру от старта; `dec()` превращает опечатку в 0 | M | Флаг `demo`, сброс по расписанию с advisory-lock |
| 54 | Невыполненное по ТЗ | `docs/tz-v3.md:80-135`; `site/content/ideas/` | Выход без `id_token_hint`, нет тайского в Rauthy, GitHub/Facebook не проверены (README их обещает), Lighthouse/ASVS не проводились, в MCP нет удаления | L | Завести задачи, README — к проверенному |

## Что делать в первую очередь

1. **Быстрые исправления (S, высокая серьёзность):** 1, 2, 3, 4, 5, 6, 7, 8, 12, 13 — около 1–2 дней работы в сумме, каждое — отдельный небольшой PR с тестом.
2. **Защитная сетка:** 15 + 16 (линтеры и сверка статики в CI), затем 10 и 11 (тесты на PostgreSQL и middleware) — без них такие баги, как 4 и 8, будут повторяться.
3. **Системные долги:** 9 (i18n ошибок), 17–19 (атомарность и гонки), 21–22 (часовой пояс и курсы), 27–28 (дублирование и размер модулей).
