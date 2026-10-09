# Фича `accounts` — Счета

Счета

- **Тип:** directory
- **Крейт:** `crates/features/accounts` (Cargo-флаг приложения `accounts`)
- **Зависит от:** `currencies` — через порты `CurrenciesLookup`, связка в `crates/app/src/wiring/wire_accounts.rs`
- **Таблица:** `fb_accounts`
- **Сгенерирована** из `feature.json` по эталону v1

## Поля

| Поле | Тип | Обязательное | Описание |
|---|---|---|---|
| code | string | да | Уникальный код в пределах пользователя |
| name | string | да | Название |
| archived | bool | — | Архив (вместо удаления) |
| name_lang | enum (RU, EN, TH) | да | Язык оригинала названия |
| name_ru | string | нет | Перевод названия: ru |
| name_en | string | нет | Перевод названия: en |
| name_th | string | нет | Перевод названия: th |
| account_kind | enum (CASH, CARD, BANK, SAVINGS) | да | Тип счёта |
| currency_code | ref → `currencies` | да | Валюта счёта |
| opening_balance | decimal | да | Начальный остаток |
| holder_user_id | uuid | нет | Держатель счёта (участник проекта) |

## Эндпоинты (под JWT)

| Метод | Путь | Действие |
|---|---|---|
| POST | /accounts | создать |
| GET | /accounts?page=0&size=100&include_archived=false | список |
| GET | /accounts/{code} | получить |
| PUT | /accounts/{code} | изменить (код не меняется) |
| DELETE | /accounts/{code} | архивировать |

## Файлы

```
crates/app/src/wiring/wire_accounts.rs
crates/features/accounts/Cargo.toml
crates/features/accounts/FEATURE.md
crates/features/accounts/feature.json
crates/features/accounts/migrations/20261009025304_create_fb_accounts.down.sql
crates/features/accounts/migrations/20261009025304_create_fb_accounts.up.sql
crates/features/accounts/src/adapters/http.rs
crates/features/accounts/src/adapters/memory.rs
crates/features/accounts/src/adapters/mod.rs
crates/features/accounts/src/adapters/postgres.rs
crates/features/accounts/src/domain.rs
crates/features/accounts/src/dto.rs
crates/features/accounts/src/lib.rs
crates/features/accounts/src/ports.rs
crates/features/accounts/src/service.rs
crates/features/accounts/src/tests.rs
```

Вне крейта фича затрагивает только строки с `@f:accounts` в `crates/app/src/wiring/mod.rs`
и свои записи в блоках `@arch:*` файла `crates/app/Cargo.toml`.

## Команды

```bash
cargo test -p accounts
python3 <skill>/scripts/arch.py disable accounts | enable accounts | remove accounts
python3 <skill>/scripts/arch.py upgrade   # после правки feature.json
```
