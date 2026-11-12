# Рекламные ролики для TikTok и Instagram Reels

Вертикальное видео 1080×1920, 30 к/с, на трёх языках — из скриншотов гайдов сайта. Звуковая дорожка тихая:
музыку добавляют в самой соцсети (трендовый звук даёт больше охвата). Собирать новые ролики помогает скил
`promo-video` (`.claude/skills/promo-video/SKILL.md`) — достаточно попросить Claude «сделай ролик про …».

| Ролик | Сценарий | Длительность | О чём |
|-------|----------|--------------|-------|
| `out/pains.<язык>.mp4` | `scenarios/pains.js` | 29,8 с | Боли семейного бюджета → решения: быстрый расход, лимиты, календарь, семья, валюты, Claude |

## Как устроено

- `scenarios/<имя>.js` — сценарий: сцены (`hook`, `shot`, `chat`, `cta`), тексты на трёх языках, кадры и движение
  камеры по скриншоту. Поля сцен — в скиле.
- `engine.js` — движок: строит сцены и анимирует их Web Animations API.
- `index.html` — стили и холст; `index.html?scenario=pains&lang=ru` — предпросмотр в браузере (через локальный
  сервер из корня репозитория, например `python3 -m http.server`).
- `render.mjs` — покадровый рендер через puppeteer-core и ffmpeg; `grid.mjs` — сетка координат на скриншоте.

## Сборка

Нужны `npm ci`, Chromium (`CHROMIUM` — путь, по умолчанию `/usr/bin/chromium-browser`) и ffmpeg.

```bash
node marketing/promo-video/render.mjs pains            # все языки, ~1,5 мин на язык
node marketing/promo-video/render.mjs pains th         # один язык
node marketing/promo-video/render.mjs pains ru sheet   # конец каждой сцены одним листом — проверить глазами
node marketing/promo-video/render.mjs pains ru 7.2     # один кадр
node marketing/promo-video/grid.mjs site/content/guides/overview/overview.ru.webp   # сетка координат
```

Переснимете скриншоты гайдов — перерендерите ролики.
