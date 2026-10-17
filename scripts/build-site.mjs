// Ассеты сайта (site/, Zola) — в site/static/assets (не коммитятся, собираются перед `zola build`):
// CSS из той же дизайн-системы, что приложение (styles/app.css + site/styles/site.css, Tailwind),
// спрайт иконок Lucide, шрифты, favicon, OG-картинка и скриншот «Обзора» из crates/app/static.
// Запуск: `npm run site:assets` (или `make site`).
import { readFileSync, writeFileSync, copyFileSync, existsSync, mkdirSync } from 'node:fs';
import { execFileSync } from 'node:child_process';
import { createRequire } from 'node:module';

const require = createRequire(import.meta.url);
const out = 'site/static/assets';
const build = 'site/.build';
mkdirSync(`${out}/fonts`, { recursive: true });
mkdirSync(build, { recursive: true });

// Шрифты и картинки — те же, что у приложения (их собирает scripts/build-assets.mjs)
const app = 'crates/app/static';
for (const f of ['inter-latin.woff2', 'inter-latin-ext.woff2', 'inter-cyrillic.woff2', 'noto-sans-thai.woff2']) copyFileSync(`${app}/${f}`, `${out}/fonts/${f}`);
for (const f of ['favicon.svg', 'favicon-32.png', 'apple-touch-icon.png', 'og.png', 'overview.webp']) copyFileSync(`${app}/${f}`, `${out}/${f}`);

// Спрайт иконок: имена из шаблонов и Markdown ({{ <icon name="…" /> }}); новую иконку — добавить сюда
const icons = [
  'wallet', 'sparkles', 'eye', 'users', 'zap', 'receipt-text', 'arrow-right-left', 'gauge', 'chart-pie', 'calendar-days',
  'languages', 'undo-2', 'mic', 'camera', 'message-circle', 'calendar-check', 'image', 'circle-check', 'shield-check', 'lock',
  'list-checks', 'key-round', 'monitor-smartphone', 'chevron-down', 'menu', 'hammer', 'rss', 'info', 'lightbulb',
  'triangle-alert', 'arrow-right', 'book-open', 'message-square-quote',
];
const lucide = require.resolve('lucide-static/package.json').replace(/package\.json$/, 'icons');
let sprite = '<svg xmlns="http://www.w3.org/2000/svg">\n';
for (const name of icons) {
  const file = `${lucide}/${name}.svg`;
  if (!existsSync(file)) throw new Error(`нет иконки Lucide: ${name}`);
  const inner = readFileSync(file, 'utf8').replace(/<!--[\s\S]*?-->/g, '').replace(/^[\s\S]*?<svg[^>]*>/, '').replace(/<\/svg>\s*$/, '').trim();
  sprite += `<symbol id="i-${name}" viewBox="0 0 24 24">${inner}</symbol>\n`;
}
writeFileSync(`${out}/icons.svg`, sprite + '</svg>\n');

// CSS: стили приложения (токены, компоненты) + стили сайта. Шрифты — по относительному пути: сайт может жить
// в подкаталоге (GitHub Pages: /family-budget/), а приложение отдаёт их из /static/.
const appCss = readFileSync('styles/app.css', 'utf8').replaceAll("url('/static/", "url('fonts/");
writeFileSync(`${build}/site.input.css`, appCss + '\n' + readFileSync('site/styles/site.css', 'utf8'));
execFileSync(require.resolve('tailwindcss/lib/cli.js'), ['-c', 'site/tailwind.config.cjs', '-i', `${build}/site.input.css`, '-o', `${out}/site.css`, '--minify'], { stdio: 'inherit' });
console.log('ассеты сайта собраны:', out);
