#!/usr/bin/env python3
"""Превью страницы сайта для пользователя: три языка страницы + нужные ассеты с относительными ссылками
и страница-«оглавление», готовые к публикации инструментом Artifact (localhost пользователю недоступен).

    python3 .claude/skills/run-site/preview.py /articles/roadmap/ [папка, по умолчанию site/.build/preview]

Перед запуском: npm run site:assets. Печатает JSON для публикации: file_path, root, files.
Ссылки на страницы вне превью ведут на сайт на GitHub Pages (адрес — из git remote).
"""
import html
import json
import os
import re
import shutil
import subprocess
import sys
from pathlib import Path

ROOT = Path(subprocess.check_output(["git", "rev-parse", "--show-toplevel"], text=True).strip())
SENTINEL = "http://preview.invalid/"
LANGS = [("ru", "", "Русский"), ("en", "en/", "English"), ("th", "th/", "ไทย")]

LAUNCHER = """<title>{title}</title>
<meta name="claude:viewport-fit-sides" content="cover">
<style>
  /* Layout: одна узкая колонка — заголовок, карточки языков, примечание. Цвета и шрифт — как у сайта. */
  @font-face {{ font-family: "Inter"; src: url("assets/fonts/inter-latin.woff2") format("woff2"); font-weight: 100 900; font-display: swap; unicode-range: U+0000-00FF, U+2000-206F, U+20AC, U+2122; }}
  @font-face {{ font-family: "Inter"; src: url("assets/fonts/inter-cyrillic.woff2") format("woff2"); font-weight: 100 900; font-display: swap; unicode-range: U+0400-045F, U+0490-0491, U+04B0-04B1, U+2116; }}
  @font-face {{ font-family: "Noto Sans Thai"; src: url("assets/fonts/noto-sans-thai.woff2") format("woff2"); font-weight: 100 900; font-display: swap; unicode-range: U+0E01-0E5B, U+200C-200D, U+25CC; }}
  :root {{
    --bg: #f8f8f6; --surface: #ffffff; --line: #e4e4e0; --fg: #1c1c1a; --muted: #5e5e58;
    --accent: #4338ca; --accent-soft: #eef0fd; --accent-text: #3730a3; --warn-soft: #fdf3e2; --warn: #a15c07;
    --font: "Inter", "Noto Sans Thai", system-ui, -apple-system, "Segoe UI", sans-serif;
  }}
  @media (prefers-color-scheme: dark) {{ :root:not([data-theme="light"]) {{
    --bg: #111110; --surface: #1b1b1a; --line: #2f2f2c; --fg: #ededea; --muted: #a3a39c;
    --accent: #6366f1; --accent-soft: #23234a; --accent-text: #a5b4fc; --warn-soft: #3a2a0f; --warn: #f5b454; color-scheme: dark;
  }} }}
  :root[data-theme="dark"] {{
    --bg: #111110; --surface: #1b1b1a; --line: #2f2f2c; --fg: #ededea; --muted: #a3a39c;
    --accent: #6366f1; --accent-soft: #23234a; --accent-text: #a5b4fc; --warn-soft: #3a2a0f; --warn: #f5b454; color-scheme: dark;
  }}
  body {{ background: var(--bg); color: var(--fg); font-family: var(--font); font-size: 16px; line-height: 1.55; }}
  .wrap {{ max-width: 44rem; margin: 0 auto; padding-block: 48px 64px; padding-left: max(16px, env(safe-area-inset-left, 0px)); padding-right: max(16px, env(safe-area-inset-right, 0px)); display: grid; gap: 28px; }}
  .eyebrow {{ display: flex; flex-wrap: wrap; align-items: center; gap: 8px; margin: 0; font-size: 13px; color: var(--muted); }}
  .branch {{ font-family: ui-monospace, SFMono-Regular, Menlo, monospace; font-size: 12px; padding: 2px 8px; border-radius: 999px; background: var(--warn-soft); color: var(--warn); }}
  h1 {{ margin: 8px 0 0; font-size: clamp(28px, 6vw, 40px); line-height: 1.15; letter-spacing: -0.02em; font-weight: 650; text-wrap: balance; }}
  .lead {{ margin: 10px 0 0; font-size: 18px; color: var(--muted); max-width: 36rem; }}
  .langs {{ display: grid; gap: 12px; }}
  .lang {{ display: grid; grid-template-columns: auto 1fr auto; align-items: center; gap: 16px; padding: 18px 20px; border: 1px solid var(--line); border-radius: 12px; background: var(--surface); color: inherit; text-decoration: none; transition: border-color .15s; }}
  .lang:hover {{ border-color: var(--accent); }}
  .lang:focus-visible {{ outline: 2px solid var(--accent); outline-offset: 2px; }}
  .code {{ width: 44px; height: 44px; border-radius: 10px; display: grid; place-items: center; font-weight: 650; font-size: 14px; background: var(--accent-soft); color: var(--accent-text); }}
  .lang-name {{ display: block; font-size: 13px; color: var(--muted); }}
  .lang-title {{ display: block; font-weight: 600; line-height: 1.35; min-width: 0; }}
  .arrow {{ color: var(--accent-text); font-size: 20px; }}
  .note {{ margin: 0; padding: 14px 16px; border-radius: 10px; border: 1px solid var(--line); font-size: 14px; color: var(--muted); }}
  .note strong {{ color: var(--fg); font-weight: 600; }}
  @media (prefers-reduced-motion: reduce) {{ .lang {{ transition: none; }} }}
</style>

<main class="wrap">
  <header>
    <p class="eyebrow">{site} · превью ветки <span class="branch">{branch}</span></p>
    <h1>{heading}</h1>
    <p class="lead">Страница сайта из ветки, ещё не опубликованная. Выберите язык — внутри переключатель RU · EN · TH в шапке тоже работает.</p>
  </header>
  <nav class="langs" aria-label="Язык страницы">
{cards}
  </nav>
  <p class="note"><strong>В превью только эта страница.</strong> Ссылки на другие разделы ведут на сайт на GitHub Pages ({pages}), а «Войти» и «Создать бюджет» — в приложение.</p>
</main>
"""

CARD = """    <a class="lang" href="{href}" lang="{lang}">
      <span class="code" aria-hidden="true">{code}</span>
      <span><span class="lang-name">{name}</span><span class="lang-title">{title}</span></span>
      <span class="arrow" aria-hidden="true">→</span>
    </a>"""


def pages_url() -> str:
    """https://github.com/Owner/repo(.git) или git@github.com:Owner/repo → https://owner.github.io/repo/."""
    remote = subprocess.check_output(["git", "remote", "get-url", "origin"], text=True, cwd=ROOT).strip()
    m = re.search(r"github\.com[:/]+([^/]+)/([^/.]+)", remote)
    return f"https://{m.group(1).lower()}.github.io/{m.group(2)}/" if m else "https://github.com/"


def text_of(page: str, pattern: str) -> str:
    m = re.search(pattern, page, re.S)
    return html.unescape(re.sub(r"<[^>]+>", "", m.group(1))).strip() if m else ""


def main() -> int:
    if len(sys.argv) < 2:
        print(__doc__, file=sys.stderr)
        return 2
    page = "/" + sys.argv[1].strip("/") + "/" if sys.argv[1].strip("/") else "/"
    out = Path(sys.argv[2]) if len(sys.argv) > 2 else ROOT / "site/.build/preview"
    if not (ROOT / "site/static/assets/site.css").exists():
        print("Нет ассетов сайта: сначала npm run site:assets", file=sys.stderr)
        return 1
    zola = subprocess.check_output([str(ROOT / ".claude/skills/run-site/zola.sh")], text=True).strip()
    build = ROOT / "site/.build/preview-src"
    subprocess.run([zola, "build", "--force", "--base-url", SENTINEL.rstrip("/"), "-o", str(build)],
                   cwd=ROOT / "site", check=True, stdout=subprocess.DEVNULL)
    shutil.rmtree(out, ignore_errors=True)
    external = pages_url()

    pages = {f"{prefix}{page.lstrip('/')}": f"{lang}/index.html" for lang, prefix, _ in LANGS}
    sources = {dst: (build / src / "index.html").read_text(encoding="utf-8") for src, dst in pages.items()}
    assets: set[str] = set()
    for text in sources.values():
        assets |= {u.split("?")[0].split("#")[0] for u in re.findall(r"http://preview\.invalid/(assets/[^\"'\s<>)]*)", text)}
    css = (build / "assets/site.css").read_text(encoding="utf-8")
    assets |= {"assets/" + u.strip("'\"") for u in re.findall(r"url\(([^)]+)\)", css) if not u.startswith(("data:", "http"))}

    def rewrite(text: str, dst: str) -> str:
        here = os.path.dirname(dst)

        def fix(m: re.Match) -> str:
            url = m.group(1)
            path = re.split(r"[?#]", url)[0]
            frag = "#" + url.split("#", 1)[1] if "#" in url else ""
            target = pages.get(path) or (path if path in assets else None)
            if target is None:
                return external + url
            if target == dst and frag:
                return frag
            return os.path.relpath(target, here) + frag

        return re.sub(r"http://preview\.invalid/([^\"'\s<>)]*)", fix, text)

    for dst, text in sources.items():
        (out / dst).parent.mkdir(parents=True, exist_ok=True)
        (out / dst).write_text(rewrite(text, dst), encoding="utf-8")
    for a in sorted(assets):
        (out / a).parent.mkdir(parents=True, exist_ok=True)
        shutil.copy(build / a, out / a)

    titles = {dst: text_of(t, r"<h1[^>]*>(.*?)</h1>") or text_of(t, r"<title>(.*?)</title>") for dst, t in sources.items()}
    ru_title = titles["ru/index.html"]
    short = ru_title.split(":")[0].strip()
    branch = subprocess.check_output(["git", "rev-parse", "--abbrev-ref", "HEAD"], text=True, cwd=ROOT).strip()
    site = text_of(sources["ru/index.html"], r'class="logo-text">(.*?)</span>') or "Сайт"
    cards = "\n".join(CARD.format(href=f"{lang}/index.html", lang=lang, code=lang.upper(), name=name,
                                  title=html.escape(titles[f"{lang}/index.html"]))
                      for lang, _, name in LANGS)
    launcher = out / "preview.html"
    launcher.write_text(LAUNCHER.format(title=html.escape(f"Превью «{short}»"), site=html.escape(site),
                                        branch=html.escape(branch), heading=html.escape(ru_title),
                                        cards=cards, pages=external), encoding="utf-8")

    files = sorted(str(p.relative_to(out)) for p in out.rglob("*") if p.is_file() and p != launcher)
    print(json.dumps({"file_path": str(launcher), "root": str(out), "files": files}, ensure_ascii=False, indent=2))
    return 0


if __name__ == "__main__":
    sys.exit(main())
