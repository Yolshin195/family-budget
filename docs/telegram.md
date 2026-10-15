# Telegram: бот (этап 1) и Mini App (этап 2) — план реализации

Цель минимальной версии: быстро завести расход из Telegram, увидеть расходы за сегодня, получить напоминание
о плановом платеже и предупреждение, что лимит почти исчерпан. Второй этап — весь веб внутри Telegram (Mini App).
Фото чека и LLM — позже, но точки расширения закладываются сейчас (порт `Assistant`, см. «Задел на будущее»).

## Принятые решения

| Вопрос | Решение |
| --- | --- |
| Где живёт бот | Новый крейт фичи `crates/features/telegram` в том же бинарнике. Фича ни от кого не зависит: всё нужное — через свои порты, адаптеры в `crates/app/src/wiring/wire_telegram.rs` |
| Сколько ботов | Один. Он единственный получатель обновлений Telegram; свободный текст и фото, которые бот не понял, позже уйдут в порт `Assistant` (LLM-сервис) |
| Права в боте и у LLM | Ровно права пользователя: роль в проекте, 2FA проекта, «чужой проект → 404». Проверки — общей функцией ядра, той же, что в middleware |
| Вход | Telegram **привязывается** к существующему пользователю (Rauthy). Регистрации «только через Telegram» нет |
| 2FA проекта | При привязке запоминается, был ли второй фактор у сессии, из которой привязывали (`mfa`). Привязка без 2FA в проектах «Требовать 2FA» видит только список проектов |
| Гость демо | Привязать Telegram нельзя (`projects::Service::is_guest`) |
| Часовой пояс | У пользователя (`fb_users.timezone`, IANA), по умолчанию — `DEFAULT_TIMEZONE` из окружения |
| Кому напоминания | Всем привязанным участникам проекта, у кого включены уведомления; у VIEWER — без кнопок действий |
| Получение обновлений | Long polling по умолчанию (на VDS не нужен входящий адрес); webhook — настройкой |
| Клиенты Mini App | Сначала iOS, Android, Desktop, macOS (встроенный браузер, cookie первой стороны). Telegram Web (iframe) — отдельный шаг 2.6 |

---

## Этап 0. Подготовка ядра

Небольшие PR, которые нужны боту и заодно чинят веб.

### 0.1. Часовой пояс пользователя

Сейчас «сегодня» — это `chrono::Local::now()`, то есть часовой пояс сервера (в контейнере — UTC). Для Таиланда
(UTC+7) с 00:00 до 07:00 «сегодня» сервера — вчера.

- Миграция `projects`: `ALTER TABLE fb_users ADD COLUMN timezone TEXT NULL` (NULL — `DEFAULT_TIMEZONE`).
- `Cargo.toml` (workspace): `chrono-tz`.
- `kernel/src/config.rs`: `default_timezone: chrono_tz::Tz` из `DEFAULT_TIMEZONE` (по умолчанию `UTC`); `.env.example`, `compose.yaml`.
- `kernel/src/auth.rs`: `SessionInfo.timezone`, `Session.timezone`, метод `Session::today() -> NaiveDate`.
- `projects`: поле в профиле (`templates/projects/profile.html`, `service::update_profile`), чтение в `Memberships::session`.
- Заменить `chrono::Local::now().date_naive()` на `session.today()` во всех `adapters/web.rs` и `adapters/http.rs`
  (transactions, planning, limits, rates, budget). `planning::Service::skip` (строка 150) — получать `today` параметром.

### 0.2. Идемпотентность создания операций

Telegram повторяет непринятые обновления, позже LLM будет повторять запросы — без ключа появятся дубли.

- Миграция `transactions`: `client_ref TEXT NULL`, `UNIQUE (project_id, client_ref)`.
- `dto::TransactionInput.client_ref: Option<String>`; `Service::create` при конфликте возвращает уже созданную операцию.
- Бот передаёт `tg:<chat_id>:<message_id>`; REST — по желанию клиента.

### 0.3. Общая проверка прав

Сервисы фич права не проверяют — это делает HTTP-middleware (`kernel/src/auth.rs::require_user` — роль,
`kernel/src/web.rs::attach_page` — 2FA). Бот вызывает сервисы напрямую, поэтому правила выносятся в ядро:

- `kernel/src/auth.rs`: `pub fn authorize(info: &SessionInfo, mfa: bool, write: bool) -> AppResult<Scope>` —
  VIEWER не пишет, `require_2fa && !mfa` → 403. `require_user` и `attach_page` используют её же; тесты на обе ветки.
- `pub async fn scope_for(memberships, user_id, project, mfa, write)` — то же для бота и планировщика.

### 0.4. Настройки

`kernel/src/config.rs` → `pub telegram: Option<Telegram>`:

| Переменная | Назначение |
| --- | --- |
| `TELEGRAM_BOT_TOKEN` / `_FILE` | токен бота (секрет). Не задан — фича `telegram` не запускает бота |
| `TELEGRAM_BOT_USERNAME` | имя бота для ссылок `t.me/<бот>?start=…` |
| `TELEGRAM_MODE` | `polling` (по умолчанию) или `webhook` |
| `TELEGRAM_WEBHOOK_SECRET` / `_FILE` | секрет заголовка `X-Telegram-Bot-Api-Secret-Token` (обязателен для webhook) |
| `DEFAULT_TIMEZONE` | см. 0.1 |

Это **другой** бот, не тот, что пишет об автообновлении в `scripts/deploy-vds.sh` (`TELEGRAM_CHAT_ID`).
`Config::validate`: в продакшене webhook только с секретом и HTTPS.

---

## Этап 1. Бот — минимальная версия

### 1.1. Что умеет

| Действие | Как |
| --- | --- |
| Привязка | «Профиль → Telegram» → «Привязать» → ссылка `t.me/<бот>?start=<код>` (одноразовый, 10 минут) → бот: «Привязано к Алексей, проект Семья» |
| Быстрый расход | Сообщение `450 кафе`, `450`, `12.5 usd такси обед`, `1 200,50 продукты` → черновик с кнопками → «Сохранить» → ответ с остатком лимита и кнопкой «Отменить» |
| Доход | `+5000 зарплата` или `/income 5000` |
| Расходы за сегодня | `/today`: итог в базовой валюте, список (категория, сумма, кто, описание); `/today me` — только свои |
| Ближайшие платежи | `/plans`: просроченные и на 7 дней вперёд, кнопки «Оплатил» / «Пропустить» |
| Лимиты | `/limits`: категории ≥ 80 % за месяц |
| Проект | `/project`: список проектов кнопками, выбранный — активный для ввода |
| Настройки | `/settings`: время утреннего напоминания, за сколько дней, лимиты вкл/выкл, пауза |
| Отвязка | `/unlink` в боте или «Отвязать» в профиле |
| Справка | `/start`, `/help` — на языке пользователя (ru/en/th) |
| Утреннее напоминание | В выбранное время (по умолчанию 09:00 по часовому поясу пользователя): платежи сегодня + просроченные, кнопки на каждый |
| Предупреждение о лимите | Сразу после расхода в боте и раз в 30 минут — по расходам из веба: «Кафе: 85 % месячного лимита, осталось 600 ฿» (≥ 80 %) и «превышен» (> 100 %), один раз на категорию, период и порог |

### 1.2. Сценарий быстрого расхода

1. Разбор текста (`domain::parse_quick`): сумма (пробелы и запятая в числе), необязательная валюта/символ,
   остальное — запрос категории и описание. Знак `+` — доход. Чистая функция, покрыта тестами.
2. `QuickDefaults` (порт → `transactions::Service::quick_defaults`): счёт по умолчанию, частые категории,
   дерево с полем `search` на трёх языках, `available` по лимитам.
3. Категория: одно совпадение по `search` — выбрана; несколько — кнопки совпадений; нет — кнопки частых
   категорий + «Все категории» (дерево по уровням, глубина ≤ 5). Валюта в тексте выбирает счёт этой валюты.
4. Черновик хранится в памяти процесса 15 минут (как `kernel::undo`), в кнопке — короткий id.
   Формат `callback_data` (≤ 64 байт): `c:<draft>:<i>` категория, `a:<draft>:<i>` счёт, `s:<draft>` сохранить,
   `x:<draft>` отмена, `u:<tx-uuid>` отменить операцию, `p:<plan-uuid>:<yyyymmdd>` оплатил, `k:…` пропустить.
5. «Сохранить» → `scope_for(…, write = true)` → `transactions::Service::create` с `client_ref` → ответ:
   «−450 ฿ · Кафе · Карта KBank. Кафе: осталось 600 ฿ из 4 000 ฿ (85 %)» + «Отменить».
6. «Отменить» → `Service::delete` (с той же проверкой прав, что в вебе: EDITOR+).

Черновик после перезапуска теряется → «Черновик устарел, отправьте сумму ещё раз». Сохранённая операция не теряется.

### 1.3. Оплата плана из напоминания

«Оплатил» создаёт операцию с `plan_id` + `plan_date`, ожидаемой суммой и счётом плана. Если нужна сумма
зачисления (перевод между валютами) или сумма другая — кнопка «Изменить» открывает в браузере
`/planning/occurrence/{plan}/{date}` (на этапе 2 — Mini App). «Пропустить» → `planning::Service::skip`.

### 1.4. Крейт `crates/features/telegram`

Создаётся `arch.py new` как `blank` и дальше ведётся вручную, как `site` (без `feature.json`).

```
crates/features/telegram/
  Cargo.toml                 kernel, axum, sqlx, reqwest (rustls, json), serde, chrono, chrono-tz, rust_decimal, uuid
  FEATURE.md
  migrations/<ts>_create_fb_tg.up.sql / .down.sql
  src/
    lib.rs                   api(ctx, deps), feature(api), start(api) — запуск polling и планировщика
    domain.rs                parse_quick, форматирование сумм и сообщений, правила порогов, ключи дедупликации,
                             проверка кода привязки (чистые функции)
    ports.rs                 TelegramApi, LinkRepository + порты к другим фичам (1.6)
    service.rs               use cases: handle_update, link, unlink, quick_expense, today, plans, limits,
                             daily_digest, limit_alerts
    i18n.rs                  тексты бота ru/en/th
    dto.rs                   типы Bot API (Update, Message, CallbackQuery, InlineKeyboardMarkup) — только нужные поля
    adapters/
      bot_api.rs             TelegramApi на reqwest: getUpdates, sendMessage, editMessageText,
                             answerCallbackQuery, setMyCommands; повтор при 429 по retry_after
      polling.rs             цикл getUpdates (timeout 50 с, offset), один на процесс
      webhook.rs             POST /telegram/webhook (публичный): секрет в заголовке, ответ 200 сразу
      scheduler.rs           тик раз в 5 минут: утренние напоминания; раз в 30 минут: лимиты
      postgres.rs            LinkRepository
      memory.rs              LinkRepository и FakeTelegramApi для тестов
      web.rs                 страница /telegram: привязать / отвязать / настройки
    tests.rs
  templates/telegram/settings.html
```

### 1.5. Таблицы

```sql
-- Привязка Telegram к пользователю: один Telegram — один пользователь.
CREATE TABLE fb_tg_links
(
    user_id        UUID PRIMARY KEY REFERENCES fb_users (id) ON DELETE CASCADE,
    tg_user_id     BIGINT      NOT NULL UNIQUE,
    chat_id        BIGINT      NOT NULL,
    project_id     UUID        NULL REFERENCES fb_projects (id) ON DELETE SET NULL, -- активный проект ввода
    mfa            BOOLEAN     NOT NULL DEFAULT false, -- был ли второй фактор у сессии, из которой привязали
    remind_at      TIME        NOT NULL DEFAULT '09:00',
    remind_days    SMALLINT    NOT NULL DEFAULT 0 CHECK (remind_days BETWEEN 0 AND 7),
    notify_plans   BOOLEAN     NOT NULL DEFAULT true,
    notify_limits  BOOLEAN     NOT NULL DEFAULT true,
    blocked_at     TIMESTAMPTZ NULL, -- бот заблокирован пользователем (403) — не писать
    linked_at      TIMESTAMPTZ NOT NULL DEFAULT now()
);

-- Одноразовые коды привязки (в БД — SHA-256), 10 минут.
CREATE TABLE fb_tg_link_codes
(
    code_hash  TEXT PRIMARY KEY,
    user_id    UUID        NOT NULL REFERENCES fb_users (id) ON DELETE CASCADE,
    mfa        BOOLEAN     NOT NULL,
    expires_at TIMESTAMPTZ NOT NULL
);

-- Отправленные уведомления: не слать повторно после перезапуска.
-- key: digest:<user>:<date> | limit:<project>:<category>:<period_start>:<80|100>:<user>
CREATE TABLE fb_tg_sent
(
    key     TEXT PRIMARY KEY,
    sent_at TIMESTAMPTZ NOT NULL DEFAULT now()
);
CREATE INDEX fb_tg_sent_at_idx ON fb_tg_sent (sent_at); -- чистка старше 60 дней

-- Смещение long polling: продолжить с того же места после перезапуска.
CREATE TABLE fb_tg_state
(
    id          SMALLINT PRIMARY KEY CHECK (id = 1),
    last_update BIGINT   NOT NULL
);
```

Привязки удаляются вместе с пользователем (`ON DELETE CASCADE`); фича добавляется в очистку при удалении проекта
(`wiring/mod.rs`, `PurgeTargets`) — сбрасывает `project_id`.

### 1.6. Порты и связки

Порты объявляет `telegram` (потребитель), адаптеры — `crates/app/src/wiring/wire_telegram.rs`.

| Порт `telegram` | Что нужно | Поставщик |
| --- | --- | --- |
| `Access` | проекты пользователя, роль, базовая валюта, язык, часовой пояс; `scope_for(user, project, mfa, write)`; `is_guest`; участники проекта | `projects::Service` (`Memberships`) + `kernel::auth::scope_for` |
| `Expenses` | `quick_defaults`, `create`, `delete`, список за день (`list` с `from = to = сегодня`, `created_by`) | `transactions::Service` |
| `Plans` | `occurrences(from, to, today)`, `skip`, план по id | `planning::Service` |
| `Limits` | `month_limits(project, base, date, lang)` — остатки лимитов по дереву | `budget::Service` |
| `Rates` | курсы к базовой валюте на дату (итог «за сегодня») | `budget::Service::rates_on` |
| `Projects` (для планировщика) | проекты, где есть привязанные пользователи | `fb_tg_links` + `projects` |
| `Assistant` | «не понял сообщение» → пока `None` | `NoAssistant` в wiring (позже — HTTP к LLM-сервису) |

### 1.7. Изменения вне крейта

| Файл | Что |
| --- | --- |
| `Cargo.toml` (workspace) | `reqwest = { version = "0.12", default-features = false, features = ["json", "rustls-tls"] }` (уже в `Cargo.lock` через `openidconnect`), `chrono-tz` |
| `crates/app/Cargo.toml` | `telegram` в `@arch:deps` и `@arch:features` (`telegram = ["dep:telegram", "transactions", "planning", "budget"]`), в `default` |
| `crates/app/src/wiring/mod.rs` | строки `@f:telegram`: `telegram::api(ctx, Deps{…})`, `list.push(telegram::feature(…))`; поле `Wired.telegram` |
| `crates/app/src/wiring/wire_telegram.rs` | адаптеры портов из 1.6 |
| `crates/app/src/main.rs` | после `assemble`: `telegram::start(wired.telegram.clone())` — `tokio::spawn` polling и планировщика (как чистка сессий) |
| `crates/kernel/src/auth.rs` | 0.1, 0.3 |
| `crates/kernel/src/config.rs` | 0.4 |
| `crates/features/projects` | timezone (0.1); ссылка «Telegram» на странице профиля → `/telegram` |
| `crates/features/transactions` | `client_ref` (0.2) |
| `crates/features/site` | `/privacy`: данные, которые проходят через Telegram |
| `.env.example`, `compose.yaml`, `docs/deploy-vds.md` | переменные 0.4; как создать бота у @BotFather, `setMyCommands` |
| `README.md` | раздел «Telegram» |

### 1.8. Безопасность

- Токен бота и секрет webhook — только из окружения / `*_FILE`; в логи не пишутся (запросы к Bot API логируются без URL).
- Webhook: сравнение секрета за постоянное время (`kernel::crypto::ct_eq`), тело ≤ 64 КБ.
- Код привязки: 128 бит, в БД SHA-256, одноразовый, 10 минут; привязка только из браузерной сессии (не API-токена), не гостем.
- Журнал безопасности (`fb_audit_log`): `telegram_linked`, `telegram_unlinked` — через порт к `projects`.
- Каждое действие бота — `scope_for` с ролью и 2FA; чужой `callback_data` (id операции другого проекта) → «не найдено».
- Лимит частоты: свой `kernel::throttle::RateLimiter` с ключом `tg:<tg_user_id>`.
- Чаты: только личные (`chat.type == "private"`); в группах бот молчит.
- 403 от Telegram (бот заблокирован) → `blocked_at`, уведомления не шлются до следующего `/start`.

### 1.9. Тесты (`cargo test -p telegram`, без БД и сети)

- `parse_quick`: суммы с пробелами и запятой, валюта, `+`, пустой текст, огромные числа, тайский/русский текст.
- Привязка: просроченный и повторный код, гость, повторная привязка другого Telegram.
- Права: VIEWER не сохраняет расход, 2FA-проект без `mfa`, чужой проект → «не найдено».
- Быстрый расход через `FakeTelegramApi` и in-memory порты: кнопки, сохранение, «Отменить», повтор того же update (идемпотентность).
- Планировщик с фиксированным «сейчас»: время напоминания в часовом поясе пользователя, дедупликация, пороги 80/100 %.
- `callback_data` всегда ≤ 64 байт.

### 1.10. Порядок PR

1. Этап 0 (0.1–0.4) — можно четырьмя PR.
2. Крейт `telegram`: таблицы, привязка, страница `/telegram`, `/start`, `/help`, `/project`, `/unlink`.
3. Быстрый расход и «Отменить», `/today`.
4. Планировщик: утренние напоминания, `/plans`, «Оплатил» / «Пропустить».
5. Предупреждения о лимитах (сразу после расхода + периодическая проверка), `/limits`, `/settings`.
6. Webhook-режим, документация развёртывания, `/privacy`.

---

## Этап 2. Mini App: весь веб внутри Telegram

Mini App — это те же страницы приложения (Askama + htmx), открытые во встроенном браузере Telegram.
Отдельного фронтенда нет. Нужны: вход без Rauthy, разрешение показа во фрейме Telegram, режим интерфейса `tg`.

### 2.1. Вход через `initData`

```
Кнопка меню бота / t.me/<бот>/app?startapp=…
  → GET /tg                         публичная страница-загрузчик (tg.html + tg.js)
  → tg.js: POST /auth/telegram      тело: initData (подписан Telegram), заголовок X-Requested-With
  → сервер: проверка HMAC-SHA256    secret = HMAC("WebAppData", bot_token); hash = HMAC(secret, data_check_string)
            auth_date не старше 10 минут, user.id есть в fb_tg_links
  → новая серверная сессия           fb_sessions (mfa = fb_tg_links.mfa), Set-Cookie как при обычном входе
  → редирект на start_param          tx_new → /transactions/new, plans → /planning, иначе /budget
Не привязан → страница «Привяжите Telegram»: кнопка открывает сайт во внешнем браузере (openLink) → /telegram
```

Сессия создаётся при каждом открытии Mini App: встроенный браузер может не хранить cookie между запусками.
CSRF и `Origin` работают без изменений — страница открыта с нашего домена.

### 2.2. Куда что добавляется

| Файл | Изменение |
| --- | --- |
| `crates/features/telegram/src/domain.rs` | `verify_init_data(raw, bot_token, now) -> Result<TgUser>` — чистая функция, тесты на подделку, просрочку, порядок полей |
| `crates/features/telegram/src/adapters/web.rs` | публичные `GET /tg`, `POST /auth/telegram` (в `Web.public`) |
| `crates/features/telegram/templates/telegram/tg.html` | загрузчик: логотип, «Входим…», `<noscript>` |
| `crates/features/telegram/src/ports.rs` | порт `Sessions::open(user_id, mfa, meta) -> raw_id` |
| `crates/features/projects/src/service.rs` | `pub async fn telegram_login(user_id, mfa, meta)` поверх существующего `open_session` + запись `login` (`method = telegram`) в журнал |
| `crates/app/src/wiring/wire_telegram.rs` | адаптер `Sessions` → `projects::Service::telegram_login` |
| `crates/app/static/telegram-web-app.js` | скрипт Telegram, положенный к себе (CSP `script-src 'self'`); обновление — в `scripts/build-assets.mjs` |
| `crates/app/static/tg.js` | загрузчик (обмен `initData` на сессию) и мост: тема, BackButton, MainButton, `openLink`, высота экрана |
| `crates/app/src/assets.rs` | `asset!("telegram-web-app.js", …)`, `asset!("tg.js", …)` |
| `crates/kernel/src/web.rs` | CSP: `frame-ancestors 'self' https://web.telegram.org` вместо `'none'`, без `X-Frame-Options` (нужно для Telegram Web; мобильным не мешает) — только при включённом Telegram; тест `content_security_policy` |
| `crates/kernel/src/auth.rs` | `Via::Session` + признак `telegram` у сессии (для режима `tg` и списка устройств) |
| `crates/features/projects` (миграция) | `fb_sessions.client TEXT NULL` (`telegram`) — показывать в «Устройствах» как «Telegram» |
| `crates/kernel/templates/base.html` | `<html data-client="tg">` при сессии из Telegram; подключение `telegram-web-app.js` и `tg.js` только в этом режиме |
| `styles/app.css` → `make css` | режим `[data-client=tg]`: без бокового меню и «Выйти», нижняя панель остаётся; отступы `safe-area`; цвета из `--tg-theme-*` в существующие токены светлой и тёмной темы |
| `crates/features/telegram/src/service.rs` | кнопка меню бота (`setChatMenuButton` → `APP_BASE_URL/tg`), кнопки «Открыть в приложении» в сообщениях бота (`web_app` / `startapp`) |

### 2.3. Режим интерфейса `tg` (`tg.js` + CSS)

- Тема: `themeParams` → CSS-переменные; `colorScheme` → `data-theme`, подписка на `themeChanged`.
- BackButton: показывается, если `history.length > 1` или есть хлебные крошки; нажатие — `history.back()`.
- MainButton: на формах с `data-tg-main="Сохранить"` дублирует кнопку отправки (форма «Расход» за 5 секунд).
- Внешние ссылки (кабинет Rauthy, правовые страницы) — `Telegram.WebApp.openLink`; выход в режиме `tg` скрыт.
- Высота: `viewportStableHeight` → `--app-height`; `expand()` при старте; `enableClosingConfirmation()` на несохранённой форме.
- htmx-запросы и формы не меняются.

### 2.4. Ссылки из бота в Mini App

`start_param` — короткие ключи: `tx_new`, `tx_new_<category>`, `today`, `plans`, `occ_<plan>_<yyyymmdd>`,
`limits`. Сопоставление ключа и адреса — в `domain.rs`, только из списка разрешённых маршрутов (не произвольный URL).
Кнопки «Изменить» и «Открыть» в сообщениях бота (1.3) ведут сюда, а не во внешний браузер.

### 2.5. Проверка

- Тесты: `verify_init_data` (правильная подпись, подмена поля, `auth_date` старше 10 минут, чужой токен), `start_param` → адрес,
  CSP с Telegram и без, вход непривязанного пользователя, гость, 2FA-проект (`mfa` из привязки).
- Вручную: iOS, Android, Telegram Desktop — вход, расход, календарь, лимиты, тёмная тема, «Назад».
- Для разработки нужен публичный HTTPS (Telegram открывает Mini App только по https): туннель (`cloudflared`)
  к `make dev` или тестовый стенд; отдельный бот для разработки.

### 2.6. Telegram Web (отдельный шаг)

web.telegram.org открывает Mini App во фрейме: cookie `SameSite=Lax` туда не отправляется, Safari режет cookie
третьей стороны. Решение — для сессий `client = telegram` cookie `SameSite=None; Secure; Partitioned`
(`kernel/src/auth.rs::session_cookie`). Делается после проверки на мобильных; до этого в Telegram Web — сообщение
«Откройте в приложении Telegram или на сайте».

---

## Задел на будущее (в этой версии не делается)

- **Фото чека:** бот принимает фото/файл → порт `ReceiptRecognizer` → черновик операции с позициями
  (`TransactionInput.lines` уже поддерживает чек по категориям) → подтверждение кнопками или в Mini App.
  Совет пользователю: отправлять «файлом» — фото Telegram сжимает. Скачивание ботом ≤ 20 МБ.
- **LLM:** отдельный сервис за портом `Assistant`. Права = права пользователя: делегированный токен
  «пользователь + проект», ~5 минут, без права выпускать токены и привязывать Telegram; изменения — после
  подтверждения кнопкой; `via=assistant` в журнале. До этого — REST с паритетом вебу (сейчас у `projects` только
  `GET /api/projects`, нет `reconcile`, остановки плана) и тест на паритет маршрутов `web.rs` ↔ `http.rs`.

## Ограничения минимальной версии

- Черновики расхода — в памяти процесса (15 минут); после перезапуска — «отправьте ещё раз».
- Один экземпляр приложения: long polling и планировщик без блокировки. Для нескольких — `pg_try_advisory_lock`
  на тик планировщика и webhook вместо polling.
- Только личные чаты; общий семейный чат с ботом — позже.
- Telegram Bot API: до ~30 сообщений в секунду всего и ~1 в секунду в один чат — для семьи достаточно,
  рассылка идёт последовательно с повтором по `retry_after`.
- Сообщения и суммы проходят через серверы Telegram (обычные чаты не шифруются сквозным шифрованием) — отражено в `/privacy`.
