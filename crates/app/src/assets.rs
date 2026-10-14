//! Статика, вшитая в бинарник (Node для запуска не нужен). Собирается `npm run build`
//! (`make assets`): Tailwind CSS, htmx, шрифты, спрайт иконок Lucide, favicon, иконки PWA, OG-картинка.
//! В шаблонах адрес берётся через `asset_url(..)` — с хешем содержимого в имени и кэшем на год.

use kernel::web::{StaticAsset, register_assets};

macro_rules! asset {
    ($name:literal, $mime:literal) => {
        StaticAsset { name: $name, bytes: include_bytes!(concat!("../static/", $name)), mime: $mime }
    };
}

pub fn register() {
    register_assets(vec![
        asset!("app.css", "text/css; charset=utf-8"),
        asset!("app.js", "text/javascript; charset=utf-8"),
        asset!("boot.js", "text/javascript; charset=utf-8"),
        asset!("htmx.min.js", "text/javascript; charset=utf-8"),
        asset!("icons.svg", "image/svg+xml"),
        asset!("favicon.svg", "image/svg+xml"),
        asset!("favicon-32.png", "image/png"),
        asset!("apple-touch-icon.png", "image/png"),
        asset!("icon-192.png", "image/png"),
        asset!("icon-512.png", "image/png"),
        asset!("icon-maskable-512.png", "image/png"),
        asset!("og.png", "image/png"),
        asset!("overview.webp", "image/webp"),
        asset!("inter-latin.woff2", "font/woff2"),
        asset!("inter-latin-ext.woff2", "font/woff2"),
        asset!("inter-cyrillic.woff2", "font/woff2"),
        asset!("noto-sans-thai.woff2", "font/woff2"),
    ]);
}
