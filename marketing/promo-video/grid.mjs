// Сетка координат поверх скриншота 1280×800 — чтобы подобрать from/to, rings и typing для сцены shot.
//   node marketing/promo-video/grid.mjs site/content/guides/overview/overview.ru.webp   → out/grid.png
// Линии каждые 50 px, подписи каждые 100 px — это координаты снимка, их и пишут в сценарий.
import { mkdirSync } from 'node:fs';
import { dirname, join } from 'node:path';
import { fileURLToPath } from 'node:url';
import sharp from 'sharp';

const src = process.argv[2];
if (!src) {
  console.error('нужно: node marketing/promo-video/grid.mjs <скриншот>');
  process.exit(1);
}
const out = join(dirname(fileURLToPath(import.meta.url)), 'out');
mkdirSync(out, { recursive: true });
const { width: W, height: H } = await sharp(src).metadata();
let svg = `<svg xmlns="http://www.w3.org/2000/svg" width="${W}" height="${H}">`;
for (let x = 0; x <= W; x += 50) {
  svg += `<line x1="${x}" y1="0" x2="${x}" y2="${H}" stroke="${x % 100 ? '#f472b6' : '#db2777'}" stroke-opacity="${x % 100 ? .25 : .6}"/>`;
  if (x % 100 === 0) svg += `<text x="${x + 2}" y="12" font-size="11" font-family="sans-serif" fill="#be185d">${x}</text>`;
}
for (let y = 0; y <= H; y += 50) {
  svg += `<line x1="0" y1="${y}" x2="${W}" y2="${y}" stroke="${y % 100 ? '#f472b6' : '#db2777'}" stroke-opacity="${y % 100 ? .25 : .6}"/>`;
  if (y % 100 === 0) svg += `<text x="2" y="${y - 2}" font-size="11" font-family="sans-serif" fill="#be185d">${y}</text>`;
}
svg += '</svg>';
const file = join(out, 'grid.png');
await sharp(src).composite([{ input: Buffer.from(svg) }]).png().toFile(file);
console.log(file);
