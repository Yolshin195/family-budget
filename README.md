# Семейная бухгалтерия — демо

Общий и раздельный бюджет нескольких людей: проекты с участниками, справочники (валюты, счета, дерево категорий)
на трёх языках, транзакции с позициями, лимиты по дереву, календарь плановых платежей и бюджет за неделю / месяц / год.

Стек: Rust 2024, Axum 0.8, Askama 0.16, sqlx 0.8 (PostgreSQL), Tailwind CSS 3, utoipa (Swagger).
Архитектура — эталон `rust-hex-api` v1: Cargo workspace, одна фича = один крейт, порты и адаптеры, связки в `crates/app/src/wiring`.

## Запуск

Одной командой в Podman (приложение + PostgreSQL 16):

```bash
make            # = make up: собрать образ и запустить в фоне
make logs       # логи приложения
make down       # остановить; make clean — ещё и удалить данные БД
make help       # все команды
```

Нужен `podman compose` (Podman 4+ с установленным `podman-compose` или `docker-compose`). С Docker: `make ENGINE=docker`.
Параметры в `.env` рядом с `compose.yaml`: `APP_PORT`, `DB_PASSWORD`, `JWT_SECRET`, `SEED_DEMO`.

Без контейнера для приложения:

```bash
make dev        # PostgreSQL в контейнере + cargo run локально
```

Откройте http://localhost:8080 — при пустой БД создаются демо-данные (отключить: `SEED_DEMO=0`):

| Пользователь | Пароль | Язык | Роль в проекте «Семья» |
| --- | --- | --- | --- |
| aleksey@demo.local | demo123 | ru | OWNER |
| anna@demo.local | demo123 | th | EDITOR |

Swagger: http://localhost:8080/docs. Токен для API: `POST /api/auth/login`, затем `Authorization: Bearer …`
и `X-Project-Id: <id проекта>` (без заголовка — личный проект).

## Стили

Скомпилированный `crates/app/static/app.css` лежит в репозитории и вшит в бинарник — Node для запуска не нужен.
После правки шаблонов или классов:

```bash
npm install
npm run css        # или npm run css:watch
```

## Структура

```
crates/kernel            ядро: ошибки, auth (JWT + проект + роль), i18n (ru/en/th), web (PageCtx, base.html, макросы)
crates/features/
  projects               пользователи, проекты, участники, вход/регистрация        (вручную)
  currencies             валюты                                                    (directory)
  rates                  курсы к базовой валюте по датам                           (journal)
  accounts               счета                                                     (directory)
  categories             дерево категорий: цикл, тип, глубина ≤ 5                  (directory + правила)
  transactions           транзакции с позициями, переводы описания, оплата планов  (вручную)
  limits                 лимиты WEEK/MONTH/YEAR на поддерево                       (journal)
  planning               планы, правила повторения, календарь, пропуски           (вручную)
  budget                 отчёт план/факт/лимит по дереву, без своих таблиц         (вручную)
crates/app               main, wiring (адаптеры портов), seed (демо-данные), static/app.css
```

У каждой фичи: `adapters/http.rs` (REST под `/api/<фича>`), `adapters/web.rs` + `templates/` (страницы под `/<фича>`).
Фичи `projects`, `transactions`, `planning`, `budget` написаны вручную (нет `feature.json`) — `arch.py upgrade` их пропускает.

## Отличия от эталона v1 (кандидаты в v2)

- Данные изолированы по **проекту** (`project_id`), а не по пользователю: `kernel::auth::Scope { user_id, project_id, role }`.
- У `Feature` есть веб-часть `web: Option<Web>` (страницы, публичные страницы, пункт меню); REST смонтирован под `/api`.
- Переводимые поля справочников: `name` + `name_lang` + `name_ru/en/th`.
- Порты справочников умеют `choices` (выпадающие списки), не только `exists`.

## Тесты

```bash
cargo test --workspace     # без БД: in-memory адаптеры и фейковые порты
```
