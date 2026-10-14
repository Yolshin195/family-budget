//! Доменная модель «Сайт»: воронка конверсии без персональных данных, sitemap и robots.

use chrono::NaiveDate;
use kernel::i18n::Lang;

/// Воронка за день: визиты лендинга → регистрации → первый расход. Только счётчики.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct Funnel {
    pub visits: u64,
    pub signups: i64,
    pub first_expenses: i64,
}

impl Funnel {
    /// Конверсия в процентах (0, если знаменатель пуст).
    pub fn rate(part: i64, whole: i64) -> f64 {
        if whole <= 0 { 0.0 } else { (part as f64) * 100.0 / (whole as f64) }
    }
}

/// Публичные страницы — для sitemap и hreflang.
pub const PUBLIC_PATHS: [&str; 4] = ["/", "/privacy", "/terms", "/data-deletion"];

/// Страницы приложения закрыты от индексации.
pub const PRIVATE_PREFIXES: [&str; 15] = [
    "/budget", "/transactions", "/planning", "/limits", "/categories", "/accounts", "/currencies", "/rates", "/projects", "/profile", "/onboarding", "/auth", "/api", "/docs", "/demo",
];

/// Адрес страницы на языке: `/?lang=th`, `/privacy?lang=en`.
pub fn localized_url(base: &str, path: &str, lang: Lang) -> String {
    format!("{base}{path}?lang={}", lang.code())
}

/// sitemap.xml: каждая публичная страница на трёх языках с hreflang-альтернативами.
pub fn sitemap(base: &str, lastmod: NaiveDate) -> String {
    let mut out = String::from(
        "<?xml version=\"1.0\" encoding=\"UTF-8\"?>\n<urlset xmlns=\"http://www.sitemaps.org/schemas/sitemap/0.9\" xmlns:xhtml=\"http://www.w3.org/1999/xhtml\">\n",
    );
    for path in PUBLIC_PATHS {
        for lang in Lang::ALL {
            out.push_str(&format!("  <url>\n    <loc>{}</loc>\n    <lastmod>{lastmod}</lastmod>\n", localized_url(base, path, lang)));
            for alt in Lang::ALL {
                out.push_str(&format!("    <xhtml:link rel=\"alternate\" hreflang=\"{}\" href=\"{}\"/>\n", alt.code(), localized_url(base, path, alt)));
            }
            out.push_str(&format!("    <xhtml:link rel=\"alternate\" hreflang=\"x-default\" href=\"{base}{path}\"/>\n  </url>\n"));
        }
    }
    out.push_str("</urlset>\n");
    out
}

/// robots.txt: лендинг и правовые страницы открыты, приложение закрыто.
pub fn robots(base: &str) -> String {
    let mut out = String::from("User-agent: *\nAllow: /$\nAllow: /privacy\nAllow: /terms\nAllow: /data-deletion\nAllow: /static/\n");
    for p in PRIVATE_PREFIXES {
        out.push_str(&format!("Disallow: {p}\n"));
    }
    out.push_str(&format!("\nSitemap: {base}/sitemap.xml\n"));
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sitemap_has_all_languages_and_alternates() {
        let s = sitemap("https://budget.example", NaiveDate::from_ymd_opt(2026, 10, 13).unwrap());
        assert_eq!(s.matches("<url>").count(), PUBLIC_PATHS.len() * 3);
        assert!(s.contains("<loc>https://budget.example/?lang=th</loc>"));
        assert!(s.contains("hreflang=\"x-default\" href=\"https://budget.example/privacy\""));
    }

    #[test]
    fn robots_closes_the_app() {
        let r = robots("https://budget.example");
        assert!(r.contains("Disallow: /budget") && r.contains("Disallow: /api") && r.contains("Allow: /privacy"));
        assert!(r.ends_with("Sitemap: https://budget.example/sitemap.xml\n"));
        assert_eq!(Funnel::rate(1, 4), 25.0);
        assert_eq!(Funnel::rate(1, 0), 0.0);
    }
}
