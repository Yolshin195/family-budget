# ТЗ: связи операций с внешними записями (SMS банка)

**Статус:** сделано — разделы 1–7; решения по ходу — раздел 9.

Задача — дать внешним источникам (первым — приложению `family-messages-sync`, которое читает SMS KBank на Mac)
**служебную связь «внешняя запись → операция»**:

1. одна внешняя запись (SMS) никогда не даёт двух операций — повторная отправка возвращает уже созданную;
2. по операции однозначно видно, что она пришла из SMS или подтверждена им, и правка этого не стирает.

Плюс две маленькие правки: форма на сайте не стирает заметки позиций (Б2) и Claude дополняет операцию из SMS
вместо создания второй (Б3).

ТЗ приложения — `docs/mvp.md` в репозитории family-messages-sync; контракты ниже — общие для обоих.

**Решение (заменяет колонку `external_ref` из `docs/group-1.md` 4.1 / 5.5 и `docs/platforms.md` этап C):** связи
хранятся в отдельной таблице, а не колонкой в `fb_transactions`. Причины: у одной операции бывает несколько внешних
записей (SMS списания и SMS зачисления у перевода, позже строка выписки); связь должна пережить удаление операции,
чтобы то же SMS не завело её снова; ключ — `(project_id, provider, external_id)`, без счёта: соответствие «карта →
счёт» может поменяться, а guid SMS уникален сам по себе.

## 1. Модель данных

### 1.1. Таблица `fb_external_refs`

Миграция в `crates/features/transactions/migrations/` (`<timestamp>_external_refs.up.sql` и `.down.sql`):

```sql
-- Связи операций с записями внешних систем (SMS банка, позже выписки, Telegram, Apple Pay), docs/external-refs.md.
-- Внешнего ключа на fb_transactions нет намеренно: после удаления операции связь остаётся, и та же внешняя запись
-- не заводит операцию снова; «Отменить» удаление возвращает операцию с тем же id — связь снова указывает на неё.
CREATE TABLE fb_external_refs (
    id             UUID PRIMARY KEY,
    project_id     UUID        NOT NULL,
    provider       TEXT        NOT NULL CHECK (provider ~ '^[a-z0-9_]{1,40}$'),
    external_id    TEXT        NOT NULL CHECK (length(external_id) BETWEEN 1 AND 200),
    transaction_id UUID        NOT NULL,
    link           TEXT        NOT NULL CHECK (link IN ('CREATED', 'MATCHED')),
    received_at    TIMESTAMPTZ NULL,
    created_at     TIMESTAMPTZ NOT NULL DEFAULT now(),
    created_by     UUID        NOT NULL
);
CREATE UNIQUE INDEX fb_external_refs_key ON fb_external_refs (project_id, provider, external_id);
CREATE INDEX fb_external_refs_tx ON fb_external_refs (project_id, transaction_id);
```

`down`: `DROP TABLE fb_external_refs;`

| Колонка | Смысл |
| --- | --- |
| `provider` | Кто прислал. Сейчас только `imessage` (SMS и iMessage из приложения Messages на Mac). Зарезервированы: `telegram`, `bank_statement`, `apple_pay`, `device` (будущий `client_id`). Список в коде не ограничивается — формат по `CHECK` |
| `external_id` | Id записи у провайдера; сервер не разбирает. Для `imessage` — `message.guid` из `chat.db` (одинаковый на всех Mac с одним Apple ID) |
| `transaction_id` | Операция, которую создала или подтвердила запись. Может указывать на удалённую операцию |
| `link` | `CREATED` — внешняя запись создала операцию (`POST /api/transactions` с `external_ref`); `MATCHED` — прикреплена к уже существующей (`POST /api/transactions/{id}/external-refs`) |
| `received_at` | Когда внешняя система получила запись (время SMS), если известно |
| `created_by` | Пользователь запроса (у API-токена — его владелец) |

Правила:

- Одна пара `(project_id, provider, external_id)` — одна связь. У операции связей сколько угодно.
- Удаление операции (`DELETE`, «Удалить» с «Отменить») связи **не трогает**.
- Правка операции (форма, `PUT`, MCP `update_transaction`) связи не видит и не меняет.
- Удаление проекта стирает связи: в `purge` транзакций добавить `DELETE FROM fb_external_refs WHERE project_id = $1`.
- Связь у начального остатка (`OPENING`) — нельзя.

### 1.2. Домен и порт

`crates/features/transactions/src/domain.rs`:

```rust
pub enum RefLink { Created, Matched }           // "CREATED" | "MATCHED"

pub struct ExternalRef {
    pub id: Uuid,
    pub project_id: Uuid,
    pub provider: String,
    pub external_id: String,
    pub transaction_id: Uuid,
    pub link: RefLink,
    pub received_at: Option<DateTime<Utc>>,
    pub created_at: DateTime<Utc>,
    pub created_by: Uuid,
}
```

Порт `TransactionRepository` (`ports.rs`), реализации — PostgreSQL и память:

```rust
/// Операция и связь `CREATED` в одной SQL-транзакции. Занятый ключ связи — `Conflict` с [`taken::EXTERNAL_REF`].
async fn insert_with_ref(&self, item: &Transaction, r: &ExternalRef) -> AppResult<()>;
/// Новая связь. Занятый ключ — `Conflict` с [`taken::EXTERNAL_REF`].
async fn insert_ref(&self, r: &ExternalRef) -> AppResult<()>;
async fn find_ref(&self, project_id: Uuid, provider: &str, external_id: &str) -> AppResult<Option<ExternalRef>>;
/// Связи операций (для страницы списка — одним запросом).
async fn refs_of(&self, project_id: Uuid, ids: &[Uuid]) -> AppResult<Vec<ExternalRef>>;
```

`Filter` (фильтр списка) — новое поле `external_provider: Option<String>`: только операции, у которых есть связь
с этим провайдером (`EXISTS (SELECT 1 FROM fb_external_refs r WHERE r.project_id = t.project_id AND
r.transaction_id = t.id AND r.provider = $n)`).

## 2. REST API

Права — как у остальной `/api/transactions`: запись — `transactions:write` (или `write`), чтение — `…:read`.
Формат ошибок — существующий: `{"error": "<КОД>", "message": "<текст на языке запроса>", "field": "<поле>"}`.

### 2.1. Общие типы

**`ExternalRefInput`** (запрос):

```json
{ "provider": "imessage", "external_id": "p:0/4F1C2D3E-...", "received_at": "2026-09-21T10:51:07+07:00" }
```

| Поле | Обязательно | Проверка | Ошибка (`422 VALIDATION`, `field`) |
| --- | --- | --- | --- |
| `provider` | да | после `trim` и перевода в нижний регистр — `^[a-z0-9_]{1,40}$` | `external_ref.provider` |
| `external_id` | да | после `trim` — 1..200 символов | `external_ref.external_id` |
| `received_at` | нет | RFC 3339 с поясом | `external_ref.received_at` |
| `occurred_at` | нет | RFC 3339 с поясом — время операции из текста SMS (раздел 10) | `external_ref.occurred_at` |
| `balance` | нет | остаток счёта после операции по данным банка, до 4 знаков после запятой (раздел 10) | `external_ref.balance` |
| `account_code` | нет | счёт остатка — один из счетов операции; нет — счёт операции | `external_ref.account_code` |

В `POST /api/transactions/{id}/external-refs` те же поля лежат на верхнем уровне тела, `field` — без префикса.

**`ExternalRefDto`** (ответ):

```json
{
  "provider": "imessage",
  "external_id": "p:0/4F1C2D3E-...",
  "transaction_id": "7b1e…",
  "link": "CREATED",
  "received_at": "2026-09-21T03:51:07Z",
  "created_at": "2026-09-21T04:02:10Z"
}
```

**`TransactionDto`** — новое поле, всегда присутствует:

```json
"external_refs": [ /* ExternalRefDto, по created_at */ ]
```

### 2.2. `POST /api/transactions` — создание с внешней записью

Новое необязательное поле тела `TransactionInput.external_ref: ExternalRefInput`. Без него — всё как сейчас.

С ним:

1. Проверить `external_ref` (2.1) и что `tx_kind` не `OPENING` (иначе `422 VALIDATION`, `field: external_ref`).
2. Найти связь `(project_id, provider, external_id)`:
   - есть, операция существует → **ничего не создаётся**, ответ `200 OK` и существующая операция (`TransactionDto`).
     Тело запроса с существующей операцией **не сравнивается**: повтор с другими полями всё равно вернёт прежнюю;
   - есть, операция удалена → `409`, `{"error": "EXTERNAL_REF_DELETED", "field": "external_ref", …}`;
   - нет → обычная проверка и создание; операция и связь `CREATED` пишутся `insert_with_ref` одной SQL-транзакцией;
     ответ `201 Created`, `TransactionDto` с этой связью в `external_refs`.
3. Гонка: два одинаковых запроса одновременно — второй ловит нарушение уникального ключа связи, вся его SQL-транзакция
   откатывается, сервис перечитывает связь и отвечает как в п. 2 (`200` с операцией первого).

| Ответ | Когда | Тело |
| --- | --- | --- |
| `201` | создана новая операция | `TransactionDto` |
| `200` | операция по этой внешней записи уже есть | `TransactionDto` (существующая) |
| `409 EXTERNAL_REF_DELETED` | операция по этой записи была и удалена | ошибка |
| `422` | ошибка проверки (как сейчас, плюс поля `external_ref.*`) | ошибка |

### 2.3. `POST /api/transactions/{id}/external-refs` — прикрепить к существующей

Тело — `ExternalRefInput` (поля на верхнем уровне). Создаёт связь `MATCHED`. Операция не меняется.

| Ответ | Когда | Тело |
| --- | --- | --- |
| `201` | связь создана | `ExternalRefDto` |
| `200` | эта пара уже прикреплена к **этой** операции | `ExternalRefDto` (существующая) |
| `404 NOT_FOUND` | операции нет в проекте | ошибка |
| `409 EXTERNAL_REF_TAKEN` | пара прикреплена к **другой** операции (в т. ч. удалённой) | ошибка; узнать, к какой, — 2.4 |
| `422 VALIDATION` | проверка полей; операция — начальный остаток (`field` пуст) | ошибка |

Гонка — как в 2.2: нарушение ключа → перечитать связь → `200` или `409`.

### 2.4. `GET /api/transactions/external-refs?provider=…&external_id=…` — найти связь

Для клиента после `409` и для отладки. Оба параметра обязательны (`422`, если нет).

| Ответ | Тело |
| --- | --- |
| `200` | `ExternalRefDto` + `"transaction_exists": true|false` |
| `404 NOT_FOUND` | связи нет |

Маршрут статический — объявить в `router()` рядом с `/quick-defaults`, до `/{id}`.

### 2.5. `GET /api/transactions` и `GET /api/transactions/{id}`

- В каждой операции — `external_refs` (2.1). Для страницы списка — один запрос `refs_of` по id страницы.
- Новый параметр `external_provider` (`ListQuery`): только операции со связью этого провайдера. Пример:
  `GET /api/transactions?from=2026-09-20&to=2026-09-22&account_code=CARD_KBANK&external_provider=imessage`.

### 2.6. Коды ошибок

В `kernel::error::codes`:

```rust
/// Внешняя запись уже создала операцию, и та удалена: повторно не создаётся.
pub const EXTERNAL_REF_DELETED: &str = "EXTERNAL_REF_DELETED";
/// Внешняя запись уже прикреплена к другой операции.
pub const EXTERNAL_REF_TAKEN: &str = "EXTERNAL_REF_TAKEN";
```

Сейчас `Problem` всегда отдаётся как `422`. Нужен вариант с кодом и статусом `409` — например,
`AppError::ConflictWith(Problem)` (`status` → `409`, `code` → `p.code`, тело — как у `Invalid`).

Тексты — `i18n.rs` фичи, три языка:

| Ключ | ru | en | th |
| --- | --- | --- | --- |
| `err.external_ref_deleted` | Операция по этой записи уже была и удалена — повторно не создаётся | A transaction from this record existed and was deleted — it is not created again | รายการจากข้อมูลนี้เคยมีและถูกลบแล้ว — จะไม่สร้างซ้ำ |
| `err.external_ref_taken` | Эта запись уже прикреплена к другой операции | This record is already linked to another transaction | ข้อมูลนี้ผูกกับรายการอื่นแล้ว |
| `err.external_ref_opening` | Начальный остаток нельзя связать с внешней записью | An opening balance cannot be linked to an external record | ยอดยกมาไม่สามารถผูกกับข้อมูลภายนอกได้ |
| `err.external_provider` | Источник: латиница в нижнем регистре, цифры и «_», до 40 знаков | Source: lowercase Latin letters, digits and “_”, up to 40 characters | แหล่งที่มา: ตัวอักษรละตินพิมพ์เล็ก ตัวเลข และ “_” ไม่เกิน 40 ตัว |
| `err.external_id` | Id внешней записи: от 1 до 200 знаков | External record id: 1 to 200 characters | รหัสข้อมูลภายนอก: 1 ถึง 200 ตัวอักษร |

### 2.7. OpenAPI

`utoipa::path` у новых обработчиков, `ExternalRefInput`, `ExternalRefDto` в схемах; описание `external_ref` в
`TransactionInput` и `external_refs` в `TransactionDto`.

## 3. Сайт

- **Лента операций.** У операции со связью `imessage` — значок «SMS» (как прочие метки строки). Подсказка
  (`title`): `CREATED` — «Из SMS банка · 21.09 10:51», `MATCHED` — «Подтверждено SMS банка · 21.09 10:51»; время —
  `received_at` в том же поясе, что и другие времена на сайте (`with_timezone(&chrono::Local)`), без
  `received_at` — дата связи. Несколько связей — один значок, в подсказке строки через перенос.
- **Форма правки.** Под заголовком — та же строка только для чтения. Полей ввода нет.
- Связи других провайдеров на сайте пока не показываются.

Ключи `i18n.rs` (шаблоны — через `t("…")`, `templates_are_translated`):

| Ключ | ru | en | th |
| --- | --- | --- | --- |
| `tx.sms_badge` | SMS | SMS | SMS |
| `tx.sms_created` | Из SMS банка · {time} | From bank SMS · {time} | จาก SMS ธนาคาร · {time} |
| `tx.sms_matched` | Подтверждено SMS банка · {time} | Confirmed by bank SMS · {time} | ยืนยันด้วย SMS ธนาคาร · {time} |

После правки шаблонов — `make css`.

## 4. MCP

- **Вывод операции** (`tx_json`, `crates/features/mcp/src/service.rs`): при связи `imessage` — поле
  `"sms": {"link": "CREATED"|"MATCHED", "received_at": "…"}` (при нескольких — последняя по `created_at`). Нужен
  `external_refs` в `Tx` и в адаптере `wire_mcp.rs`.
- **`list_transactions`**: новый аргумент `from_sms: Option<bool>` — `true` → фильтр `external_provider = imessage`.
  Описание аргумента: «Only transactions created from or confirmed by a bank SMS».
- Через MCP связи не создаются и не меняются.
- **Абзац в `INSTRUCTIONS`** (`crates/features/mcp/src/adapters/server.rs`), перед последним предложением про
  данные:

> Transactions with `sms.link: CREATED` were created automatically from a bank SMS: amount, account and date are
> facts, category and description are placeholders. Before add_expense or add_income for a purchase that may already
> have reached the bank, call list_transactions with from_sms: true for that account and the purchase date ±1 day.
> If one has the same amount and direction and still has a placeholder category, update it with update_transaction
> (category, place, for, description, translations, items) instead of adding a new one, and tell the user that you
> updated the transaction from the bank SMS of that date and time. Add a separate transaction only if the user says
> it is a different purchase.

## 5. Б2. Форма не стирает заметки позиций

Ошибка: форма правки отправляет у позиций `note: None` (`transactions/src/adapters/web.rs`, `to_input`), а в
режиме формы (`replace_items`) `domain.rs` `item()` не берёт прежние значения — заметки, добавленные через API или
Claude, пропадают при любом сохранении на сайте.

- В режиме формы позиция с тем же `id` сохраняет прежнюю `note`, если форма её не прислала (`None`). Остальные поля
  — из формы, как сейчас. Новая позиция (без `id`) — без заметки.
- `PUT` с правкой по `id` и MCP `update_transaction` уже сохраняют заметку — не менять.

## 6. Тесты

Use cases `transactions` — на памяти и PostgreSQL (`kernel::testing`, как существующие):

| # | Сценарий | Ожидается |
| --- | --- | --- |
| Т1 | `create` с `external_ref` | `201`-ветка, у операции одна связь `CREATED` |
| Т2 | повтор с тем же `external_ref` и другой суммой | `200`-ветка, та же операция, операций в проекте одна |
| Т3 | тот же `external_id` у другого `provider` | новая операция |
| Т4 | тот же `external_ref` в другом проекте | новая операция (изоляция) |
| Т5 | удалить операцию → повтор | `EXTERNAL_REF_DELETED` |
| Т6 | `delete_with_undo` → `undo_delete` → повтор | `200`-ветка, та же операция |
| Т7 | `create` с `external_ref` и ошибкой проверки (нет категории) | ни операции, ни связи |
| Т8 | `attach` к ручной операции | связь `MATCHED`; повтор — без новой строки; та же пара к другой операции — `EXTERNAL_REF_TAKEN` |
| Т9 | `attach` к начальному остатку, к чужой операции | `422`; `404` |
| Т10 | две связи у одной операции (перевод) | обе в `external_refs` |
| Т11 | правка формой и `update` по `id` | связи на месте |
| Т12 | фильтр `external_provider` в `list` и `count` | только связанные |
| Т13 | `find_ref` | `transaction_exists` верен до и после удаления |
| Т14 | `purge_project` | связей проекта нет, чужие на месте |
| Т15 | гонка: два `create` с одним `external_ref` параллельно (PostgreSQL) | одна операция, оба получают её |
| Т16 | Б2: позиция с заметкой → правка формой с тем же `id` | заметка на месте; новая позиция — без заметки |

HTTP (`adapters/http.rs`): коды ответов 2.2–2.4, формат ошибок `409`. MCP: `from_sms`, поле `sms` в выводе.
Шаблоны: `templates_are_translated`.

## 7. Документация и «Что нового»

- `docs/group-1.md` (4.1, 5.5, таблица 6) и `docs/platforms.md` (этап C): колонку `external_ref` заменяет таблица
  `fb_external_refs` — ссылка на этот документ и ключ `(project_id, provider, external_id)`. `client_id` и
  «возможный дубль» остаются в шаге 4.
- `site/content/releases/next/` — по три файла (формат — `site/README.md`):
  - `bank-sms-badge` (`change = "new"`): «Операции, которые пришли из SMS банка или подтверждены им, отмечены
    значком SMS. Одно SMS никогда не даёт двух операций»;
  - `item-notes-kept` (`change = "fixed"`): «Правка операции на сайте больше не стирает заметки к позициям,
    добавленные через Claude или API».
- Гайд не нужен: значок появляется только у тех, кто подключил приложение на Mac (у него — свой README).

## 8. Готово, когда

- `cargo test --workspace` (память и PostgreSQL), `make css`, `make site-check`.
- Контракты 2.1–2.5 совпадают с OpenAPI (`/api/docs`).
- На стенде: `curl` по 2.2 дважды → `201`, затем `200` с тем же `id`; удалить → `409`; значок «SMS» в ленте.
- Вне рамок (следующий шаг): флаг «не проверено» (`needs_review`), отвязка связи на сайте («Это другая покупка»),
  сопоставление с ручными операциями на сервере, отдельное право `autoimport:write`, пакетное создание.

## 9. Как сделано: решения сверх ТЗ

- Сервис: `create` по-прежнему возвращает операцию; `create_once` — операцию и признак «создана» (`201` / `200`).
  Форма, MCP и быстрый ввод `external_ref` не передают.
- Два ключа словаря, которых не было в 2.6: `err.external_received_at` (время не в RFC 3339 с поясом) и
  `err.external_ref_not_found` (`404` поиска связи).
- `409 EXTERNAL_REF_TAKEN` при прикреплении — без `field` (поля лежат на верхнем уровне тела); в гонке при создании —
  `field: external_ref`.
- Время связи и SMS хранится с точностью до микросекунд (как `TIMESTAMPTZ`): ответ на запрос совпадает с тем, что
  потом читается из базы.
- MCP: `sms.received_at` — местное время сервера с поясом (RFC 3339), как остальные времена сайта.
- Гонка из Т15 проверена на PostgreSQL: второй запрос ловит нарушение `fb_external_refs_key`, его SQL-транзакция
  откатывается целиком, ответ — операция первого.

## 10. Время операции и остаток банка из SMS (25.10.2026)

Две проблемы из использования: операции из SMS в ленте стояли не в порядке сообщений (лента внутри дня шла по времени
внесения, а SMS обрабатываются пачкой и с задержкой), и не было видно, сходится ли учёт с банком.

- **Время операции** — `fb_transactions.occurred_at` (docs/group-1.md, шаг 1, первая часть). `family-messages-sync`
  передаёт время из текста SMS (`25/10/26 14:27`; получено в ту же минуту — время получения с секундами) и в теле
  операции (`occurred_at`), и в записи (`external_ref.occurred_at`).
- **Прикрепление** (2.3) больше не оставляет операцию нетронутой: время из записи (`occurred_at`, без него —
  `received_at`) того же дня становится временем операции — ручная операция встаёт на место покупки. Другой день —
  время не меняется.
- **Остаток банка** (`balance`, `account_code`) не хранится в связи: сервис записывает его в счёт —
  `fb_accounts.bank_balance`, `bank_balance_at` — на момент записи (`occurred_at` → `received_at` → время операции).
  Более старый, чем записанный, не перезаписывает новый (SMS обрабатываются не по порядку). Счёт не из операции —
  `422`, ничего не создаётся и не прикрепляется.
- **Сверка** — остаток по операциям на тот же момент: операции до его дня, операции дня не позже момента и операции дня
  без времени (`balance_at`). Карточка счёта: «В банке · время», «Сходится» или «В операциях на N больше / меньше, чем
  в банке» и «Сверить остаток» с остатком и датой из SMS; та же строка над лентой выписки; MCP — `bank_balance`,
  `bank_balance_at`, `bank_difference` в `get_balances` и `get_context`.
- Совместимость: старая бухгалтерия новые поля игнорирует. Если время из SMS по часам Mac пришлось на другой день по
  часам бухгалтерии (`422`, `field: occurred_at`), приложение повторяет создание без времени.
