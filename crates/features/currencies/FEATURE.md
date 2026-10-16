# Фича `currencies` — Валюты

Валюты

- **Тип:** directory
- **Крейт:** `crates/features/currencies` (Cargo-флаг приложения `currencies`)
- **Таблица:** `fb_currencies`
- **Сгенерирована** из `feature.json` по эталону v2

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
| symbol | string | да | Символ, например ฿ |
| minor_units | i32 | да | Знаков после запятой |

## Эндпоинты (под JWT)

| Метод | Путь | Действие |
|---|---|---|
| POST | /currencies | создать |
| GET | /currencies?page=0&size=100&include_archived=false | список |
| GET | /currencies/{code} | получить |
| PUT | /currencies/{code} | изменить (код не меняется) |
| DELETE | /currencies/{code} | архивировать |

## Файлы

```
crates/features/currencies/Cargo.toml
crates/features/currencies/FEATURE.md
crates/features/currencies/feature.json
crates/features/currencies/migrations/20261009025302_create_fb_currencies.down.sql
crates/features/currencies/migrations/20261009025302_create_fb_currencies.up.sql
crates/features/currencies/src/adapters/http.rs
crates/features/currencies/src/adapters/memory.rs
crates/features/currencies/src/adapters/mod.rs
crates/features/currencies/src/adapters/postgres.rs
crates/features/currencies/src/domain.rs
crates/features/currencies/src/dto.rs
crates/features/currencies/src/lib.rs
crates/features/currencies/src/ports.rs
crates/features/currencies/src/service.rs
crates/features/currencies/src/tests.rs
```

Вне крейта фича затрагивает только строки с `@f:currencies` в `crates/app/src/wiring/mod.rs`
и свои записи в блоках `@arch:*` файла `crates/app/Cargo.toml`.

## Команды

```bash
cargo test -p currencies
python3 <skill>/scripts/arch.py disable currencies | enable currencies | remove currencies
python3 <skill>/scripts/arch.py upgrade   # после правки feature.json
```
