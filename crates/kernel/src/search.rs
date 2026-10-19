//! Поиск записи по коду или названию на любом языке (техдолг #5) — один механизм для MCP, быстрого
//! ввода (`static/app.js`, функция `fold` — то же правило) и справочников мест и людей.
//! Сравнение без учёта регистра и диакритики: «cafe» находит «Café», «елка» — «Ёлка».
use unicode_normalization::UnicodeNormalization;

/// Разделитель названий одной записи в строке поиска: «Еда␟Food␟อาหาร». Не пробел: иначе точное
/// название из нескольких слов («Car wash») не отличить от склейки соседних.
pub const SEP: char = '\u{1f}';

/// Названия через [`SEP`] — без пустых и повторов, в исходном виде (их можно показать).
pub fn join<'a>(names: impl IntoIterator<Item = &'a str>) -> String {
    let mut out: Vec<&str> = Vec::new();
    for n in names.into_iter().map(str::trim).filter(|n| !n.is_empty()) {
        if !out.contains(&n) {
            out.push(n);
        }
    }
    out.join(&SEP.to_string())
}

/// Строка для сравнения: нижний регистр, без диакритики латиницы и кириллицы (Café → cafe, ё → е),
/// пробелы схлопнуты. Тайские знаки гласных и тона остаются — без них разные слова совпадают.
pub fn fold(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    let mut space = false;
    for c in s.trim().to_lowercase().nfd() {
        if ('\u{300}'..='\u{36f}').contains(&c) {
            continue;
        }
        if c.is_whitespace() {
            space = true;
            continue;
        }
        if space {
            out.push(' ');
            space = false;
        }
        out.push(if c == 'đ' { 'd' } else { c });
    }
    out
}

/// Что нашлось по запросу.
#[derive(Debug, PartialEq)]
pub enum Found<'a, T> {
    One(&'a T),
    None,
    /// Несколько подходят — пусть уточнят (в порядке списка).
    Many(Vec<&'a T>),
}

/// Запись по коду или названию: точный код → единственное точное название на любом языке →
/// единственное совпадение по части названия. `names` — названия записи через [`SEP`].
pub fn find<'a, T>(query: &str, items: &'a [T], code: impl Fn(&T) -> &str, names: impl Fn(&T) -> String) -> Found<'a, T> {
    let q = query.trim();
    if q.is_empty() {
        return Found::None;
    }
    if let Some(x) = items.iter().find(|x| code(x).eq_ignore_ascii_case(q)) {
        return Found::One(x);
    }
    let q = fold(q);
    let folded: Vec<String> = items.iter().map(|x| fold(&names(x))).collect();
    let exact: Vec<&T> = items.iter().zip(&folded).filter(|(_, n)| n.split(SEP).any(|n| n.trim() == q)).map(|(x, _)| x).collect();
    if let [one] = exact[..] {
        return Found::One(one);
    }
    let partial: Vec<&T> = items.iter().zip(&folded).filter(|(_, n)| n.contains(&q)).map(|(x, _)| x).collect();
    match partial[..] {
        [] => Found::None,
        [one] => Found::One(one),
        _ => Found::Many(partial),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fold_ignores_case_and_diacritics_but_keeps_thai() {
        assert_eq!(fold("  Café   Amazon "), "cafe amazon");
        assert_eq!(fold("Ёлка"), "елка");
        assert_eq!(fold("Phở Đà Nẵng"), "pho da nang");
        assert_eq!(fold("ร้านกาแฟ"), "ร้านกาแฟ");
        assert_eq!(fold(&join(["Car wash", "ล้างรถ"])), format!("car wash{SEP}ล้างรถ"), "разделитель не пробел");
    }

    #[test]
    fn join_skips_empty_and_repeats() {
        assert_eq!(join(["Перекусы", " Перекусы ", "", "Snacks"]), format!("Перекусы{SEP}Snacks"));
    }

    #[test]
    fn find_prefers_code_then_exact_name_then_part() {
        let items = [("CAR", join(["Машина", "Car", "รถ"])), ("CARWASH", join(["Мойка", "Car wash", "ล้างรถ"])), ("COFFEE", join(["Кофейня", "Café"]))];
        let pick = |q: &str| match find(q, &items, |x| x.0, |x| x.1.clone()) {
            Found::One(x) => Ok(x.0),
            Found::None => Err(0),
            Found::Many(v) => Err(v.len()),
        };
        assert_eq!(pick("carwash"), Ok("CARWASH"), "код без учёта регистра");
        assert_eq!(pick("รถ"), Ok("CAR"), "точное тайское название важнее части «ล้างรถ»");
        assert_eq!(pick("car"), Ok("CAR"));
        assert_eq!(pick("cafe"), Ok("COFFEE"), "без диакритики");
        assert_eq!(pick("КОФЕЙНЯ"), Ok("COFFEE"), "без регистра");
        assert_eq!(pick("мойк"), Ok("CARWASH"), "часть названия");
        assert_eq!(pick("ca"), Err(3), "неоднозначно");
        assert_eq!(pick("самолёт"), Err(0));
        assert_eq!(pick(" "), Err(0));
    }
}
