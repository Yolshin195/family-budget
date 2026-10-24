#!/usr/bin/env python3
"""Проверка сайта (site/) перед сборкой: всё — на трёх языках и одинаково устроено на каждом.

- у каждой страницы content/ есть три файла: page.md (ru), page.en.md, page.th.md;
- заголовок (front matter) у всех трёх одной формы: те же ключи и длины списков, тексты непустые,
  а служебные значения (дата, версия, статус, темы, шаблон…) совпадают;
- в тексте столько же заголовков и картинок; ссылки @/… ведут на страницу того же языка;
  скриншот с языком в имени (step-1.ru.png) — того же языка, и у него есть версии для двух других;
- у материалов блога (релизы, гайды, идеи, записи к следующему релизу) есть обязательные поля;
- строки интерфейса (config.toml [translations]) — одни и те же ключи на трёх языках, без пустых;
- в шаблонах нет текста на русском или тайском: всё, что видит читатель, — через trans();
- отзывы (data/reviews.toml) — с текстом на трёх языках;
- нигде не обещаем «бесплатно», пока не решён вопрос монетизации (ТЗ, раздел 11).

Запуск: python3 scripts/check_site.py (из корня репозитория). Нужен Python 3.11+ (tomllib).
"""
import re
import sys
import tomllib
from pathlib import Path

SITE = Path(__file__).resolve().parent.parent / "site"
CONTENT = SITE / "content"
LANGS = ["ru", "en", "th"]
DEFAULT = "ru"
# Значения этих ключей одинаковы на всех языках (тексты под остальными ключами переводятся).
INVARIANT = {"template", "page_template", "sort_by", "render", "transparent", "weight", "date", "updated",
             "taxonomies", "version", "status", "change", "level", "icon", "release", "paginate_by", "slug"}
STATUSES = {"discussing", "planned", "in_progress", "done"}
CHANGES = {"new", "improved", "fixed"}
LEVELS = {"beginner", "advanced"}
FREE = {"ru": re.compile(r"бесплат", re.I), "en": re.compile(r"\bfree\b", re.I), "th": re.compile("ฟรี")}

errors: list[str] = []


def err(path: Path | str, msg: str) -> None:
    errors.append(f"{Path(path).relative_to(SITE.parent) if isinstance(path, Path) else path}: {msg}")


def lang_of(path: Path) -> tuple[str, str]:
    """privacy.en.md → ('privacy', 'en'); _index.md → ('_index', 'ru')."""
    stem = path.name[: -len(".md")]
    for lang in LANGS:
        if lang != DEFAULT and stem.endswith(f".{lang}"):
            return stem[: -len(lang) - 1], lang
    return stem, DEFAULT


def split(path: Path) -> tuple[dict, str]:
    text = path.read_text(encoding="utf-8")
    m = re.match(r"\+\+\+\n(.*?)\n\+\+\+\n?(.*)", text, re.S)
    if not m:
        err(path, "нет заголовка +++ … +++")
        return {}, text
    try:
        return tomllib.loads(m.group(1)), m.group(2)
    except tomllib.TOMLDecodeError as e:
        err(path, f"ошибка TOML в заголовке: {e}")
        return {}, m.group(2)


def shape(value, key: str = "", where: str = ""):
    """Форма значения: тексты — «str», служебные значения — как есть."""
    if key in INVARIANT:
        return value
    if isinstance(value, dict):
        return {k: shape(v, k, f"{where}.{k}") for k, v in value.items()}
    if isinstance(value, list):
        return [shape(v, key, where) for v in value]
    if isinstance(value, str):
        return "str"
    return value


def empty_strings(value, where: str = "") -> list[str]:
    if isinstance(value, dict):
        return [p for k, v in value.items() for p in empty_strings(v, f"{where}.{k}" if where else k)]
    if isinstance(value, list):
        return [p for i, v in enumerate(value) for p in empty_strings(v, f"{where}[{i}]")]
    return [where] if isinstance(value, str) and not value.strip() else []


def diff(a, b, where: str = "") -> str | None:
    if isinstance(a, dict) and isinstance(b, dict):
        for k in sorted(set(a) | set(b)):
            if k not in a or k not in b:
                return f"ключ {where + '.' if where else ''}{k} есть не во всех языках"
            d = diff(a[k], b[k], f"{where}.{k}" if where else k)
            if d:
                return d
        return None
    if isinstance(a, list) and isinstance(b, list):
        if len(a) != len(b):
            return f"{where}: разная длина списка ({len(a)} и {len(b)})"
        for i, (x, y) in enumerate(zip(a, b, strict=True)):
            d = diff(x, y, f"{where}[{i}]")
            if d:
                return d
        return None
    return None if a == b else f"{where}: разные значения ({a!r} и {b!r}) — должны совпадать на всех языках"


def body_stats(body: str) -> dict:
    code_free = re.sub(r"```.*?```", "", body, flags=re.S)
    return {
        "заголовков": len(re.findall(r"^#{1,6} ", code_free, re.M)),
        "картинок": len(re.findall(r"!\[[^\]]*\]\(", code_free)),
        "абзацев": len([p for p in re.split(r"\n\s*\n", code_free.strip()) if p.strip()]),
    }


def check_body(path: Path, lang: str, body: str) -> None:
    for target in re.findall(r"\(@/([^)#\s]+)", body):
        if target.endswith(".md"):
            _, tlang = lang_of(Path(target))
            if tlang != lang:
                want = target if lang == DEFAULT else re.sub(r"(\.(en|th))?\.md$", f".{lang}.md", target)
                err(path, f"ссылка @/{target} ведёт на страницу другого языка — нужна @/{want}")
        if not (CONTENT / target).exists():
            err(path, f"ссылка @/{target}: такого файла нет в site/content")
    for link in re.findall(r"\]\((/[^)]*)\)", body):
        err(path, f"ссылка {link} от корня сайта: на GitHub Pages сайт лежит в подкаталоге — используйте @/путь.md")
    for img in re.findall(r"!\[[^\]]*\]\(([^)\s]+)", body):
        if re.match(r"(https?:|@/|/)", img):
            continue
        m = re.match(r"(.+)\.(ru|en|th)\.(\w+)$", img)
        if m:
            if m.group(2) != lang:
                err(path, f"картинка {img} — другого языка (нужна {m.group(1)}.{lang}.{m.group(3)})")
            for other in LANGS:
                if not (path.parent / f"{m.group(1)}.{other}.{m.group(3)}").exists():
                    err(path, f"нет картинки {m.group(1)}.{other}.{m.group(3)} рядом со статьёй")
        elif not (path.parent / img).exists():
            err(path, f"нет картинки {img} рядом со статьёй")


def check_blog_fields(path: Path, section: str, meta: dict) -> None:
    extra = meta.get("extra", {})
    topics = meta.get("taxonomies", {}).get("topics", [])
    if section in ("releases", "guides", "ideas", "next") and "date" not in meta:
        err(path, "нет date")
    if section in ("releases", "guides", "ideas"):
        if not meta.get("description"):
            err(path, "нет description — это подзаголовок статьи и текст карточки")
        if not topics:
            err(path, "нет [taxonomies] topics — хотя бы одна тема")
    if section == "releases" and not re.fullmatch(r"\d+\.\d+\.\d+", str(extra.get("version", ""))):
        err(path, "у релиза нет extra.version вида 0.2.0")
    if section == "ideas":
        if extra.get("status") not in STATUSES:
            err(path, f"extra.status — одно из {sorted(STATUSES)}")
        if extra.get("status") == "done" and not extra.get("release"):
            err(path, "у готовой идеи нужен extra.release — путь к релизу, например releases/0-2-0.md")
    if section == "guides" and "level" in extra and extra["level"] not in LEVELS:
        err(path, f"extra.level — одно из {sorted(LEVELS)}")
    if section == "next" and extra.get("change") not in CHANGES:
        err(path, f"extra.change — одно из {sorted(CHANGES)}")


def check_content() -> set[str]:
    groups: dict[tuple[Path, str], dict[str, Path]] = {}
    for path in sorted(CONTENT.rglob("*.md")):
        base, lang = lang_of(path)
        groups.setdefault((path.parent, base), {})[lang] = path
    topics: set[str] = set()
    for (folder, base), files in groups.items():
        missing = [lang for lang in LANGS if lang not in files]
        if missing:
            have = next(iter(files.values()))
            err(have, f"нет перевода: {', '.join(f'{base}.{m}.md' if m != DEFAULT else f'{base}.md' for m in missing)}")
            continue
        rel = folder.relative_to(CONTENT).parts
        section = "next" if rel[:2] == ("releases", "next") else (rel[0] if rel else "")
        parsed = {lang: split(files[lang]) for lang in LANGS}
        ref_meta, ref_body = parsed[DEFAULT]
        ref_stats = body_stats(ref_body)
        for lang in LANGS:
            meta, body = parsed[lang]
            path = files[lang]
            for p in empty_strings(meta):
                err(path, f"пустой текст: {p}")
            if lang != DEFAULT:
                d = diff(shape(ref_meta), shape(meta))
                if d:
                    err(path, f"заголовок отличается от {files[DEFAULT].name}: {d}")
                stats = body_stats(body)
                for k, v in ref_stats.items():
                    if stats[k] != v:
                        ref = files[DEFAULT].name
                        err(path, f"в тексте {stats[k]} {k}, а в {ref} — {v}: переводы должны совпадать по устройству")
            check_body(path, lang, body)
            if not base.startswith("_index"):
                check_blog_fields(path, section, meta)
            text = path.read_text(encoding="utf-8")
            if FREE[lang].search(text):
                err(path, "слово «бесплатно» (free / ฟรี): не обещаем, пока не решена монетизация")
        topics.update(ref_meta.get("taxonomies", {}).get("topics", []))
    return topics


def check_translations(topics: set[str]) -> None:
    cfg = tomllib.loads((SITE / "config.toml").read_text(encoding="utf-8"))
    tables = {DEFAULT: cfg.get("translations", {})}
    for lang in LANGS:
        if lang != DEFAULT:
            tables[lang] = cfg.get("languages", {}).get(lang, {}).get("translations", {})
    keys = set().union(*tables.values())
    for lang, table in tables.items():
        for key in sorted(keys - set(table)):
            err("site/config.toml", f"[translations] {lang}: нет ключа {key}")
        for key, value in table.items():
            if not str(value).strip():
                err("site/config.toml", f"[translations] {lang}: пустой {key}")
    for t in sorted(topics):
        if f"topic_{t}" not in keys:
            err("site/config.toml", f"тема «{t}» без названия: добавьте topic_{t} на трёх языках")


def check_templates() -> None:
    letters = re.compile(r"[Ѐ-ӿ฀-๿]")
    for path in sorted((SITE / "templates").rglob("*")):
        if not path.is_file():
            continue
        text = re.sub(r"\{#.*?#\}", "", path.read_text(encoding="utf-8"), flags=re.S)
        for i, line in enumerate(text.splitlines(), 1):
            if letters.search(line):
                err(path, f"строка {i}: текст в шаблоне — вынесите его в [translations] (config.toml) или в контент")


def check_reviews() -> None:
    path = SITE / "data" / "reviews.toml"
    data = tomllib.loads(path.read_text(encoding="utf-8"))
    for i, r in enumerate(data.get("reviews", [])):
        where = f"отзыв {i + 1}"
        if not r.get("author") or "date" not in r:
            err(path, f"{where}: нужны author и date")
        if r.get("lang") not in LANGS:
            err(path, f"{where}: lang — язык оригинала, одно из {LANGS}")
        for lang in LANGS:
            if not str(r.get("text", {}).get(lang, "")).strip():
                err(path, f"{where}: нет текста text.{lang}")


def main() -> int:
    topics = check_content()
    check_translations(topics)
    check_templates()
    check_reviews()
    if errors:
        print(f"Сайт: {len(errors)} ошибок — всё на сайте должно быть на трёх языках (site/README.md):",
              file=sys.stderr)
        for e in errors:
            print(f"  {e}", file=sys.stderr)
        return 1
    print("Сайт: три языка, структура и ссылки — в порядке.")
    return 0


if __name__ == "__main__":
    sys.exit(main())
