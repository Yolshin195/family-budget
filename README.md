# Семейная бухгалтерия

Общий и раздельный бюджет нескольких людей: проекты с участниками, справочники (валюты, счета, дерево категорий)
на трёх языках, транзакции по двойной записи (расход с позициями, доход, перевод и обмен валют между счетами одной
транзакцией), лимиты по дереву, календарь плановых платежей, бюджет за неделю / месяц / год,
быстрый расход за 5 секунд с любой страницы, лендинг, вход через Rauthy (email, GitHub, Facebook, код 2FA по email),
MCP-сервер для Claude: расходы голосом и по фото чека с телефона, бюджет и плановые платежи — под своим аккаунтом.

Стек: Rust 2024, Axum 0.8, Askama 0.16, sqlx 0.8 (PostgreSQL), Tailwind CSS 3, htmx 2, utoipa (Swagger),
`openidconnect` (вход через Rauthy 0.37). Архитектура — эталон `rust-hex-api` v2: Cargo workspace, одна фича = один
крейт, порты и адаптеры, связки в `crates/app/src/wiring`.

Что сделано по ТЗ v3 (разделы 9–13) и что осталось — [docs/tz-v3.md](docs/tz-v3.md).
ТЗ на реализацию группы 1 (карточка операции: позиции чека, время, места, «Для кого») — [docs/group-1.md](docs/group-1.md).
Схема БД для https://app.diagrams.net — [docs/db-schema.drawio](docs/db-schema.drawio) (File → Open from → Device).
Сайт (предварительная версия на GitHub Pages) — https://yolshin195.github.io/family-budget/.

## Запуск

Одной командой в Podman (приложение + PostgreSQL 16 + Rauthy):

```bash
cp .env.example .env    # секреты и адреса; для локального запуска значения по умолчанию рабочие
make                    # собрать образ и запустить всё в фоне
make rauthy-secrets     # секрет клиента family-budget и пароли демо-пользователей Rauthy
                        # → OIDC_CLIENT_SECRET в .env, затем:
make restart
```

Откройте http://localhost:8080 — лендинг. «Посмотреть демо» — вход гостем в демо-семью (только просмотр).
«Войти» ведёт в Rauthy (http://localhost:8443): демо-пользователи `aleksey@demo.local` (OWNER) и `anna@demo.local`
(EDITOR, тайский) с паролями из `make rauthy-secrets`. Админка Rauthy: http://localhost:8443/auth/v1/admin
(`admin@localhost`, пароль — в логах Rauthy при первом старте).

| Команда | Что делает |
| --- | --- |
| `make logs` / `make ps` | логи приложения / состояние контейнеров |
| `make down` / `make clean` | остановить / остановить и удалить данные |
| `make dev` | PostgreSQL и Rauthy в контейнерах + `cargo run` локально (Swagger: http://localhost:8080/docs) |
| `make db-roles` | пользователи БД без суперправ для базы, созданной до обновления (см. ниже) |
| `make assets` | пересобрать CSS, htmx, шрифты, иконки Lucide, favicon (Node — в контейнере) |
| `make test` | тесты, БД не нужна |
| `make lint` / `make fmt` | линтеры, как в CI: rustfmt, clippy, shellcheck, actionlint, hadolint, ruff / отформатировать Rust-код |
| `make audit` | `cargo audit` + `cargo deny` |
| `make backup` / `make restore-check` | зашифрованные копии баз `family_budget` и `rauthy` / проверка восстановления |

С Docker: `make ENGINE=docker`.

### Без Rauthy (только разработка)

При `APP_ENV=dev` и пустом `OIDC_ISSUER` страница `/auth/login` предлагает «Вход для разработки» под демо-пользователями.
В продакшене (`APP_ENV=prod`) этот вход выключен, а приложение не стартует без `SESSION_SECRET`, OIDC-клиента и HTTPS.

### Обновление существующей установки (до ТЗ v3)

1. `make down && git pull && make` — миграция `projects` добавит `rauthy_sub`, сессии, токены, журнал и удалит пароли.
   Пользователи, созданные до Rauthy, получают `rauthy_sub = legacy:<id>`; демо-пользователи переносятся автоматически.
   Реальному пользователю нужно проставить `sub` из Rauthy вручную (Rauthy → Users → id):
   `UPDATE fb_users SET rauthy_sub = '<id>' WHERE email = '<email>';`
2. `make db-roles` — пользователи PostgreSQL `fb_app` и `rauthy` без прав суперпользователя, база `rauthy`,
   передача таблиц приложения пользователю `fb_app`.

## Продакшен

Пошаговое развёртывание на одном VDS (Docker, Caddy с HTTPS, Rauthy, резервные копии, обновление и откат) —
[docs/deploy-vds.md](docs/deploy-vds.md). Автоматически, с готовым образом из GHCR:
`sudo bash scripts/deploy-vds.sh install vds.conf` (образец настроек — `deploy/vds.conf.example`).

- Секреты — только из окружения или файлов секретов (`NAME_FILE=/run/secrets/...`): `SESSION_SECRET`,
  `OIDC_CLIENT_SECRET`, `RAUTHY_*`, SMTP. `APP_ENV=prod`, `SEED_DEMO=0`, Swagger выключен.
- Приложение и Rauthy (`auth.<домен>`) — за обратным прокси с TLS 1.2+; `TRUST_PROXY=1`, `RAUTHY_PROXY_MODE=true`,
  `RAUTHY_COOKIE_MODE=host`, redirect `https://<домен>/auth/callback` в клиенте Rauthy.
- Админка Rauthy — только администратору, с passkey. GitHub и Facebook — Upstream Auth Providers в админке Rauthy.
- Резервные копии: `make backup` по cron ежедневно (age-шифрование; хранятся три копии: свежая, 1–6 и 7–12 дней
  назад), `make restore-check` раз в месяц.

### Готовый образ (GitHub Container Registry)

Пуш в `main`, меняющий код или сборку (`crates/`, `Cargo.toml`, `Cargo.lock`, `Dockerfile`), собирает образ для
linux/amd64 и linux/arm64 и публикует его в
[ghcr.io/yolshin195/family-budget](https://github.com/Yolshin195/family-budget/pkgs/container/family-budget)
(`.github/workflows/docker-publish.yml`). Теги: `latest`, версия из `Cargo.toml` и неизменяемый `sha-<коммит>` — для
отката на конкретную сборку. Теги ставятся, только если прошли тесты. Пересобрать без изменений кода (например, ради
свежих исправлений безопасности Debian) — «Run workflow» в Actions. Образ другой ветки — тоже «Run workflow»:
тег `<ветка>`, `latest` не меняется.

```bash
podman pull ghcr.io/yolshin195/family-budget:latest
# образ собран этим репозиторием (подписанная аттестация происхождения):
gh attestation verify oci://ghcr.io/yolshin195/family-budget:latest -R Yolshin195/family-budget
```

Чтобы запускать готовый образ вместо локальной сборки, у сервиса `app` в `compose.yaml` уберите `build: .` и укажите
`image: ghcr.io/yolshin195/family-budget:<тег>`.

## Сайт: лендинг и блог

`site/` — статический сайт на Zola: лендинг, блог («Что нового» по версиям, гайды со скриншотами, идеи), «О проекте»,
отзывы и правовые страницы — всё на русском, английском и тайском. Пока публикуется
на [GitHub Pages](https://yolshin195.github.io/family-budget/) (`.github/workflows/site.yml`) отдельно от приложения
и закрыт от индексации. Команды: `make site-check`, `make site`, `make site-serve`. Как писать статьи и записи
о релизах — [site/README.md](site/README.md), правила для Claude — [CLAUDE.md](CLAUDE.md).

## MCP для Claude

`https://<домен>/mcp` — бюджет в Claude (claude.ai, приложение на телефоне, Claude Desktop, Claude Code): контекст,
операции, остатки, бюджет, плановые платежи. Вход — личный API-токен (Claude Code) или OAuth через Rauthy
(claude.ai); проект и права ассистента — «Профиль → Безопасность → Ассистенты (MCP)». Удаления через ассистента нет.
Подключение и настройка клиента в Rauthy — [docs/mcp.md](docs/mcp.md).

## Безопасность (ТЗ, раздел 13)

Серверные сессии (`__Host-session`, в БД — хеш), CSRF-токен в каждой форме и htmx-запросе + проверка `Origin`,
CSP без `unsafe-inline`/`unsafe-eval` (все скрипты — файлы со своего домена), HSTS, `frame-ancestors 'none'`,
`nosniff`, `Referrer-Policy`, `Permissions-Policy`, CORS только для своего домена, тело ≤ 64 КБ, таймаут 15 с,
лимит запросов на сессию и токен, личные API-токены с правами, журнал безопасности (1 год), требование 2FA от
участников проекта (по `amr`), «чужой проект → 404» во всех фичах. CI на каждый PR: тесты, `cargo audit`,
`cargo deny`, Trivy, OWASP ZAP baseline (`.github/workflows/ci.yml`); раз в неделю — `cargo audit`, `cargo deny` и
Trivy по опубликованному образу (`.github/workflows/security-scan.yml`); Renovate (`renovate.json`).

API: личный токен из «Профиль → Безопасность» → `Authorization: Bearer fbt_…` (проект фиксирован токеном).
Вход по паролю через API (`POST /api/auth/login`) убран.

## Интерфейс (ТЗ, раздел 9)

Боковое меню 240 px (сворачивается до 64 px, на 768–1279 px свёрнуто по умолчанию), на телефоне — нижняя панель из
4 разделов и круглая «+». Дизайн-токены — CSS-переменные в `styles/app.css` (светлая и тёмная тема), иконки Lucide,
свои шрифты Inter и Noto Sans Thai. Горячие клавиши: N — расход, I — доход, Esc — закрыть, ? — список клавиш.
Удаление и архив — без «Вы уверены?»: тост 5 секунд с «Отменить».

Скомпилированная статика (`crates/app/static`) лежит в репозитории и вшита в бинарник (адреса с хешем, кэш на год) —
Node для запуска не нужен. После правки шаблонов: `make css`; скриншот «Обзора» для лендинга: `npm run screenshot`.

## Структура

```
crates/kernel            ядро: ошибки, auth (сессия / API-токен → проект → роль, CSRF, лимиты), config,
                         crypto, i18n (ru/en/th), web (PageCtx, каркас, тосты, статика, заголовки, 404/500)
crates/features/
  projects               пользователи (Rauthy), проекты, участники, сессии, API-токены, журнал, профиль (вручную)
  currencies             валюты                                                    (directory)
  rates                  курсы к базовой валюте по датам                           (journal)
  accounts               счета: остаток и начальный остаток — из журнала операций  (directory → вручную)
  categories             дерево категорий: цикл, тип, глубина ≤ 5                  (directory + правила)
  transactions           двойная запись: проводки, переводы, обмен, выписка, сверка (вручную)
  limits                 лимиты WEEK/MONTH/YEAR на поддерево                       (journal)
  planning               планы (в т. ч. переводы), повторения, календарь, пропуски  (вручную)
  budget                 отчёт план/факт/лимит по дереву, без своих таблиц         (вручную)
  site                   лендинг, правовые страницы, sitemap, robots, без таблиц   (blank → вручную)
crates/app               main, wiring (адаптеры портов), seed (демо-данные), assets, static/
deploy/                  postgres/init-roles.sh, rauthy/bootstrap (клиент и демо-пользователи стенда)
scripts/                 build-assets.mjs, screenshot.mjs, backup.sh, restore-check.sh,
                         build-site.mjs, check_site.py, guide-screenshots.mjs (сайт)
site/                    сайт на Zola: лендинг, блог, гайды, идеи, правовые страницы (site/README.md)
```

У каждой фичи: `adapters/http.rs` (REST под `/api/<фича>`), `adapters/web.rs` + `templates/` (страницы).

## Тесты

```bash
cargo test --workspace     # без БД: in-memory адаптеры и фейковые порты; 134 теста
```

Покрыто: правила домена, «чужой проект → 404» и «Отменить» в каждой фиче, вход через OIDC (state, nonce, aud, iss,
срок, подпись), сессии и API-токены, CSRF и Origin, 2FA, быстрый расход, полнота переводов шаблонов.
