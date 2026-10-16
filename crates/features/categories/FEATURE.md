# Фича `categories` — Категории

Категории

- **Тип:** directory
- **Крейт:** `crates/features/categories` (Cargo-флаг приложения `categories`)
- **Таблица:** `fb_categories`
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
| category_kind | enum (EXPENSE, INCOME) | да | Расход или доход |
| parent_code | string | нет | Код родителя; пусто у корня |
| sort_order | i32 | да | Порядок среди соседей |

## Эндпоинты (под JWT)

| Метод | Путь | Действие |
|---|---|---|
| POST | /categories | создать |
| GET | /categories?page=0&size=100&include_archived=false | список |
| GET | /categories/{code} | получить |
| PUT | /categories/{code} | изменить (код не меняется) |
| DELETE | /categories/{code} | архивировать |

## Файлы

```
crates/features/categories/Cargo.toml
crates/features/categories/FEATURE.md
crates/features/categories/feature.json
crates/features/categories/migrations/20261009025305_create_fb_categories.down.sql
crates/features/categories/migrations/20261009025305_create_fb_categories.up.sql
crates/features/categories/src/adapters/http.rs
crates/features/categories/src/adapters/memory.rs
crates/features/categories/src/adapters/mod.rs
crates/features/categories/src/adapters/postgres.rs
crates/features/categories/src/domain.rs
crates/features/categories/src/dto.rs
crates/features/categories/src/lib.rs
crates/features/categories/src/ports.rs
crates/features/categories/src/service.rs
crates/features/categories/src/tests.rs
```

Вне крейта фича затрагивает только строки с `@f:categories` в `crates/app/src/wiring/mod.rs`
и свои записи в блоках `@arch:*` файла `crates/app/Cargo.toml`.

## Команды

```bash
cargo test -p categories
python3 <skill>/scripts/arch.py disable categories | enable categories | remove categories
python3 <skill>/scripts/arch.py upgrade   # после правки feature.json
```
