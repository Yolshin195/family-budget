# Фича `rates` — Курсы валют

Курсы валют

- **Тип:** journal
- **Крейт:** `crates/features/rates` (Cargo-флаг приложения `rates`)
- **Зависит от:** `currencies` — через порты `CurrenciesLookup`, связка в `crates/app/src/wiring/wire_rates.rs`
- **Таблица:** `fb_rates`
- **Сгенерирована** из `feature.json` по эталону v2

## Поля

| Поле | Тип | Обязательное | Описание |
|---|---|---|---|
| rate_date | date | да | Дата курса |
| currency_code | ref → `currencies` | да | Валюта |
| rate | decimal | да | Сколько единиц базовой валюты за 1 единицу валюты |

## Эндпоинты (под JWT)

| Метод | Путь | Действие |
|---|---|---|
| POST | /rates | создать |
| GET | /rates?from=&to=&page=0&size=100 | список за период по `rate_date` |
| GET | /rates/{id} | получить |
| PUT | /rates/{id} | изменить |
| DELETE | /rates/{id} | удалить |

## Файлы

```
crates/app/src/wiring/wire_rates.rs
crates/features/rates/Cargo.toml
crates/features/rates/FEATURE.md
crates/features/rates/feature.json
crates/features/rates/migrations/20261009025303_create_fb_rates.down.sql
crates/features/rates/migrations/20261009025303_create_fb_rates.up.sql
crates/features/rates/src/adapters/http.rs
crates/features/rates/src/adapters/memory.rs
crates/features/rates/src/adapters/mod.rs
crates/features/rates/src/adapters/postgres.rs
crates/features/rates/src/domain.rs
crates/features/rates/src/dto.rs
crates/features/rates/src/lib.rs
crates/features/rates/src/ports.rs
crates/features/rates/src/service.rs
crates/features/rates/src/tests.rs
```

Вне крейта фича затрагивает только строки с `@f:rates` в `crates/app/src/wiring/mod.rs`
и свои записи в блоках `@arch:*` файла `crates/app/Cargo.toml`.

## Команды

```bash
cargo test -p rates
python3 <skill>/scripts/arch.py disable rates | enable rates | remove rates
python3 <skill>/scripts/arch.py upgrade   # после правки feature.json
```
