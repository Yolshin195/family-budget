# Сайт: лендинг, блог, гайды, идеи

Статический сайт на [Zola](https://www.getzola.org/) 0.23 (шаблоны — Tera 2). Пока живёт отдельно от приложения на
GitHub Pages (`.github/workflows/site.yml`) и закрыт от индексации; приложение работает как раньше на
family-budget-555.ru со своим лендингом. Позже сайт переедет на основной домен, приложение — на `budget.`, вход — на `auth.`.

**Всё — на трёх языках: русский, английский, тайский.** Без исключений: страницы, записи, подписи к картинкам,
скриншоты, строки интерфейса сайта. Перевод — в том же PR, «переведу потом» не бывает. Проверяет
`scripts/check_site.py` (в CI и `make site-check`).

## Устройство

```
site/
  config.toml           языки, адрес приложения, флаг preview, строки интерфейса [translations] на трёх языках
  content/
    _index.md           лендинг: тексты — в [extra] (у .en.md и .th.md — та же структура)
    blog/               общая лента: релизы, гайды и идеи вместе (своих страниц нет)
    releases/           «Что нового»: страница на версию — 0-1-0.md, .en.md, .th.md
      next/             записи «к следующему релизу», по одной на PR
    guides/<slug>/      гайд: index.md, index.en.md, index.th.md + скриншоты <имя>.ru|en|th.webp
    ideas/              идеи и планы со статусами
    about.md  feedback.md  privacy.md  terms.md  data-deletion.md
  data/reviews.toml     отзывы для страницы «Отзывы» — оригинал и переводы
  templates/            Tera: base, index (лендинг), разделы, статьи, темы, partials/, components.html
  styles/site.css       стили сайта поверх styles/app.css (дизайн-система приложения)
```

Русский — язык по умолчанию: `page.md` → `/page/`, `page.en.md` → `/en/page/`, `page.th.md` → `/th/page/`.
Адреса — латиницей и одинаковые на всех языках (имя файла).

## Команды

| Команда | Что делает |
| --- | --- |
| `make site-check` | проверка трёх языков, структуры и ссылок (Python в контейнере) |
| `make site` | проверка, ассеты (CSS, иконки, шрифты) и сборка в `site/public` |
| `make site-serve` | сайт с автообновлением на http://localhost:1111 |

Без контейнеров: `python3 scripts/check_site.py`, `npm run site:assets`, затем `zola serve` в `site/`.
Ассеты (`site/static/assets`) и `site/public` не коммитятся — их собирают `npm run site:assets` и `zola build`.

## Как писать

Тон — как на лендинге: просто, по делу, на «вы», без канцелярита и обещаний «бесплатно» (вопрос монетизации не решён).
Названия кнопок и разделов — **точно как в интерфейсе** приложения на этом языке (тексты интерфейса — в `i18n.rs` фич).
Внутренние ссылки — только `@/путь.md` на файл **того же языка**: из `.en.md` — на `@/guides/x/index.en.md`.
Ссылки от корня (`/blog/`) не работают — на GitHub Pages сайт лежит в подкаталоге.

У всех трёх файлов одной страницы одинаковое устройство: те же поля заголовка, столько же заголовков `##`,
абзацев и картинок. Служебные поля (`date`, `version`, `status`, `change`, `level`, темы) совпадают.

### Запись к следующему релизу — в каждом PR, который заметит пользователь

Три файла в `content/releases/next/` с именем по сути изменения, например `rates-any-direction.md`, `.en.md`, `.th.md`:

```toml
+++
title = "Курс валюты — в любую сторону"
date = 2026-10-16

[taxonomies]
topics = ["currencies"]

[extra]
change = "improved"   # new — новое, improved — улучшено, fixed — исправлено
+++
Одно-два предложения: что изменилось для пользователя и зачем.
```

Без заголовков и картинок — это строка в блоке «В разработке» на странице «Что нового». Отдельной страницы у записи
нет. Изменение не видно пользователю (рефакторинг, тесты, CI) — вместо записи строка `Changelog: none` в описании PR:
это проверяет `.github/workflows/release-notes.yml`.

### Статья о релизе — когда меняется версия в Cargo.toml

Соберите записи из `releases/next/` в `content/releases/<версия через дефисы>.md` (и `.en.md`, `.th.md`), а записи удалите:

```toml
+++
title = "Версия 0.2: …"
description = "Главное в одном предложении — подзаголовок и текст карточки."
date = 2026-11-01

[taxonomies]
topics = ["limits", "claude"]

[extra]
version = "0.2.0"
+++
Вступление: что главное в выпуске.

## Новое
## Улучшено
## Исправлено
```

### Гайд — когда появилась возможность, о которой стоит рассказать

Папка `content/guides/<slug>/` с `index.md`, `index.en.md`, `index.th.md` и скриншотами. Поля: `title`, `description`,
`date`, `[taxonomies] topics`, `[extra] level = "beginner" | "advanced"`. Шаги — заголовки `## 1. …`; при трёх и больше
заголовках сверху появляется оглавление.

Скриншоты — на каждом языке свои: `![подпись](limits.ru.webp)` в `index.md`, `limits.en.webp` в `index.en.md`.
Снимаются скриптом с демо-данных (приложение запущено локально: `make dev`):

```bash
APP_URL=http://localhost:8080 node scripts/guide-screenshots.mjs site/content/guides/first-budget \
  overview=/budget quick-expense=/budget@n accounts=/accounts limits=/limits
```

Интерфейс изменился — переснимите той же командой. Выноска в тексте: `{% <note kind="tip"> %}Текст{% </note> %}`
(`info`, `tip`, `warn`). Фигурные скобки Tera в тексте статьи — внутри `{% raw %}…{% endraw %}`.

### Идея — что планируем

`content/ideas/<slug>.md` (+ `.en.md`, `.th.md`): `title`, `description`, `date`, темы и
`[extra] status = "discussing" | "planned" | "in_progress" | "done"`. Идея вышла — `status = "done"` и
`release = "releases/0-2-0.md"`: на странице появится ссылка на релиз.

### Темы

Сквозные темы материалов — `[taxonomies] topics`. Сейчас: `start`, `family`, `accounts`, `currencies`, `limits`,
`planning`, `claude`, `security`, `site`. Новая тема — ключ `topic_<slug>` на трёх языках в `config.toml`.

### Отзывы

`data/reviews.toml` — вручную и только с согласия автора: `author`, `date`, `lang` (язык оригинала) и `text.ru`,
`text.en`, `text.th`. Под переводом сайт сам пишет «Перевод с языка: …».

### Строки интерфейса и шаблоны

Любой текст в шаблонах — через `trans(key="…", lang=lang)` с ключом в `config.toml` на трёх языках; русского или
тайского текста в `templates/` быть не должно. Лендинг: тексты — в `content/_index*.md`, разметка — `templates/index.html`.
Комментарии под статьями: место — `templates/partials/comments.html`, включаются `extra.comments` (сервис выберем позже).

## Публикация

- PR: `site.yml` проверяет три языка, собирает сайт и внутренние ссылки.
- `main`: то же + публикация на GitHub Pages. Один раз: Settings → Pages → Source: **GitHub Actions**.

### Переезд на основной домен (позже)

`extra.preview = false` (снимает noindex и плашку), `base_url` и `extra.app_url` — новые адреса, доставка на сервер
вместо Pages. Лендинг и правовые страницы приложения (`crates/features/site`) тогда заменяются редиректами на сайт.
