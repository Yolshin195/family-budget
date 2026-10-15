// Сборка статики для crates/app/static (результат коммитится — для `cargo run` Node не нужен):
// htmx, свои шрифты (Inter, Noto Sans Thai), спрайт иконок Lucide, favicon и иконки PWA, OG-картинка.
// Запуск: `npm run assets` (или `make assets` — в контейнере Node).
import { readFileSync, writeFileSync, copyFileSync, existsSync, mkdirSync } from 'node:fs';
import { createRequire } from 'node:module';
import sharp from 'sharp';

const require = createRequire(import.meta.url);
const out = 'crates/app/static';
mkdirSync(out, { recursive: true });
const pkg = (name) => require.resolve(`${name}/package.json`).replace(/package\.json$/, '');

// htmx — без CDN (CSP: скрипты только со своего домена)
copyFileSync(`${pkg('htmx.org')}dist/htmx.min.js`, `${out}/htmx.min.js`);

// Шрифты: только нужные подмножества (латиница, кириллица, тайский), переменная насыщенность
const fonts = [
  ['@fontsource-variable/inter', 'inter-latin-wght-normal.woff2', 'inter-latin.woff2'],
  ['@fontsource-variable/inter', 'inter-latin-ext-wght-normal.woff2', 'inter-latin-ext.woff2'],
  ['@fontsource-variable/inter', 'inter-cyrillic-wght-normal.woff2', 'inter-cyrillic.woff2'],
  ['@fontsource-variable/noto-sans-thai', 'noto-sans-thai-thai-wght-normal.woff2', 'noto-sans-thai.woff2'],
];
for (const [p, file, name] of fonts) copyFileSync(`${pkg(p)}files/${file}`, `${out}/${name}`);

// Спрайт иконок: один набор (Lucide), линия задаётся в CSS (1.5 px), размер 20 px
const icons = [
  'wallet', 'panel-left', 'chevrons-up-down', 'users', 'plus', 'languages', 'log-out', 'eye', 'ellipsis', 'x', 'search',
  'cloud-off', 'split', 'circle-alert', 'triangle-alert', 'shield-alert', 'shield-check', 'shield', 'settings', 'key-round',
  'external-link', 'monitor-smartphone', 'circle-check', 'archive', 'trash-2', 'pencil', 'coins', 'wallet-cards', 'folder-tree',
  'trending-up', 'gauge', 'chart-pie', 'receipt-text', 'calendar-days', 'calendar-check', 'list-checks', 'chevron-left',
  'chevron-right', 'chevron-down', 'check', 'skip-forward', 'undo-2', 'arrow-right', 'search-x', 'zap',
  'arrow-right-left', 'scale', 'qr-code', 'link', 'share-2', 'user-plus',
];
const brand = {
  'brand-github': '<path fill="currentColor" stroke="none" d="M12 .5C5.65.5.5 5.65.5 12a11.5 11.5 0 0 0 7.86 10.91c.58.1.79-.25.79-.56v-2c-3.2.7-3.87-1.37-3.87-1.37-.53-1.33-1.28-1.69-1.28-1.69-1.05-.72.08-.7.08-.7 1.16.08 1.77 1.19 1.77 1.19 1.03 1.77 2.7 1.26 3.36.96.1-.75.4-1.26.73-1.55-2.55-.29-5.24-1.28-5.24-5.69 0-1.26.45-2.29 1.19-3.09-.12-.29-.52-1.46.11-3.05 0 0 .97-.31 3.17 1.18a11 11 0 0 1 5.77 0c2.2-1.49 3.17-1.18 3.17-1.18.63 1.59.23 2.76.11 3.05.74.8 1.19 1.83 1.19 3.09 0 4.42-2.7 5.39-5.26 5.68.41.36.78 1.06.78 2.14v3.17c0 .31.21.67.8.56A11.5 11.5 0 0 0 23.5 12C23.5 5.65 18.35.5 12 .5Z"/>',
  'brand-facebook': '<path fill="currentColor" stroke="none" d="M24 12.07C24 5.4 18.63 0 12 0S0 5.4 0 12.07C0 18.1 4.39 23.1 10.13 24v-8.44H7.08v-3.49h3.05V9.41c0-3.02 1.79-4.69 4.53-4.69 1.31 0 2.68.24 2.68.24v2.97h-1.51c-1.49 0-1.96.93-1.96 1.89v2.25h3.33l-.53 3.49h-2.8V24C19.61 23.1 24 18.1 24 12.07Z"/>',
};
const lucide = `${pkg('lucide-static')}icons`;
let sprite = '<svg xmlns="http://www.w3.org/2000/svg">\n';
for (const name of icons) {
  const file = `${lucide}/${name}.svg`;
  if (!existsSync(file)) throw new Error(`нет иконки Lucide: ${name}`);
  const inner = readFileSync(file, 'utf8').replace(/<!--[\s\S]*?-->/g, '').replace(/^[\s\S]*?<svg[^>]*>/, '').replace(/<\/svg>\s*$/, '').trim();
  sprite += `<symbol id="i-${name}" viewBox="0 0 24 24">${inner}</symbol>\n`;
}
for (const [name, body] of Object.entries(brand)) sprite += `<symbol id="i-${name}" viewBox="0 0 24 24">${body}</symbol>\n`;
writeFileSync(`${out}/icons.svg`, sprite + '</svg>\n');

// Favicon и иконки PWA
const mark = readFileSync('web/brand/mark.svg');
const maskable = readFileSync('web/brand/maskable.svg');
writeFileSync(`${out}/favicon.svg`, mark);
await sharp(mark, { density: 300 }).resize(32, 32).png().toFile(`${out}/favicon-32.png`);
await sharp(mark, { density: 300 }).resize(180, 180).flatten({ background: '#4338ca' }).png().toFile(`${out}/apple-touch-icon.png`);
await sharp(mark, { density: 300 }).resize(192, 192).png().toFile(`${out}/icon-192.png`);
await sharp(mark, { density: 300 }).resize(512, 512).png().toFile(`${out}/icon-512.png`);
await sharp(maskable, { density: 300 }).resize(512, 512).png().toFile(`${out}/icon-maskable-512.png`);
await sharp(readFileSync('web/brand/og.svg')).resize(1200, 630).png({ compressionLevel: 9 }).toFile(`${out}/og.png`);
// Скриншот «Обзора» для лендинга: web/brand/overview.png (снимок экрана) → WebP 1280×800
if (existsSync('web/brand/overview.png')) {
  await sharp('web/brand/overview.png').resize(1280, 800, { fit: 'cover', position: 'top' }).webp({ quality: 80 }).toFile(`${out}/overview.webp`);
}
console.log('статика собрана:', out);
