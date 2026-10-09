# Фича `limits` — Лимиты

Лимиты

- **Тип:** journal
- **Крейт:** `crates/features/limits` (Cargo-флаг приложения `limits`)
- **Зависит от:** `categories` — через порты `CategoriesLookup`, связка в `crates/app/src/wiring/wire_limits.rs`
- **Таблица:** `fb_limits`
- **Сгенерирована** из `feature.json` по эталону v1

## Поля

| Поле | Тип | Обязательное | Описание |
|---|---|---|---|
| valid_from | date | да | Действует с даты |
| category_code | ref → `categories` | да | Категория (лимит на всё поддерево) |
| limit_period | enum (WEEK, MONTH, YEAR) | да | Период лимита |
| amount | decimal | да | Сумма лимита в базовой валюте проекта |

## Эндпоинты (под JWT)

| Метод | Путь | Действие |
|---|---|---|
| POST | /limits | создать |
| GET | /limits?from=&to=&page=0&size=100 | список за период по `valid_from` |
| GET | /limits/{id} | получить |
| PUT | /limits/{id} | изменить |
| DELETE | /limits/{id} | удалить |

## Файлы

```
crates/app/src/wiring/wire_limits.rs
crates/features/limits/Cargo.toml
crates/features/limits/FEATURE.md
crates/features/limits/feature.json
crates/features/limits/migrations/20261009025307_create_fb_limits.down.sql
crates/features/limits/migrations/20261009025307_create_fb_limits.up.sql
crates/features/limits/src/adapters/http.rs
crates/features/limits/src/adapters/memory.rs
crates/features/limits/src/adapters/mod.rs
crates/features/limits/src/adapters/postgres.rs
crates/features/limits/src/domain.rs
crates/features/limits/src/dto.rs
crates/features/limits/src/lib.rs
crates/features/limits/src/ports.rs
crates/features/limits/src/service.rs
crates/features/limits/src/tests.rs
```

Вне крейта фича затрагивает только строки с `@f:limits` в `crates/app/src/wiring/mod.rs`
и свои записи в блоках `@arch:*` файла `crates/app/Cargo.toml`.

## Команды

```bash
cargo test -p limits
python3 <skill>/scripts/arch.py disable limits | enable limits | remove limits
python3 <skill>/scripts/arch.py upgrade   # после правки feature.json
```
