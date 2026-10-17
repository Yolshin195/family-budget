// Скриншоты приложения для гайдов сайта — сразу на трёх языках: <имя>.ru.webp, <имя>.en.webp, <имя>.th.webp.
// Снимает демо-семью под первым демо-пользователем через «Вход для разработки», поэтому приложение должно быть
// запущено локально с демо-данными: `make dev` (APP_ENV=dev, SEED_DEMO=1, пустой OIDC_ISSUER).
//
//   node scripts/guide-screenshots.mjs <папка гайда> <имя>=<путь>[@клавиша] ...
//   node scripts/guide-screenshots.mjs site/content/guides/first-budget overview=/budget quick-expense=/budget@n limits=/limits
//
// «@клавиша» — нажать клавишу после загрузки (n — окно «Новый расход»). APP_URL — адрес приложения
// (по умолчанию http://localhost:8080), DEMO_PROJECT — проект для съёмки (по умолчанию «Семья»), CHROMIUM — путь к браузеру. Кадр 1280×800, WebP ~30–50 КБ.
import puppeteer from 'puppeteer-core';
import sharp from 'sharp';

const [dir, ...specs] = process.argv.slice(2);
if (!dir || !specs.length) {
  console.error('нужно: node scripts/guide-screenshots.mjs <папка гайда> <имя>=<путь>[@клавиша] ...');
  process.exit(1);
}
const base = process.env.APP_URL || 'http://localhost:8080';
const project = process.env.DEMO_PROJECT || 'Семья';
const locales = { ru: 'ru-RU', en: 'en-GB', th: 'th-TH' };

for (const [lang, locale] of Object.entries(locales)) {
  const browser = await puppeteer.launch({
    executablePath: process.env.CHROMIUM || '/usr/bin/chromium-browser',
    args: ['--no-sandbox', '--disable-dev-shm-usage', `--lang=${locale}`],
  });
  const page = await browser.newPage();
  await page.setViewport({ width: 1280, height: 800, deviceScaleFactor: 1.5 });
  await page.setExtraHTTPHeaders({ 'Accept-Language': locale });
  await page.emulateMediaFeatures([{ name: 'prefers-color-scheme', value: 'light' }]);
  await page.goto(`${base}/auth/login`, { waitUntil: 'networkidle0' });
  await Promise.all([page.waitForNavigation({ waitUntil: 'networkidle0' }), page.click('form[action="/auth/dev"] button')]);
  // Демо-семья богаче личного бюджета: переключаемся на неё, если активен другой проект.
  const family = await page.$$eval('a[href^="/projects/switch/"]', (as, name) => as.find((a) => a.textContent.includes(name))?.getAttribute('href'), project);
  if (family) await page.goto(base + family, { waitUntil: 'networkidle0' });
  await page.goto(`${base}/lang/${lang}?back=/budget`, { waitUntil: 'networkidle0' });
  for (const spec of specs) {
    const [name, target] = spec.split('=');
    const [path, key] = target.split('@');
    await page.goto(base + path, { waitUntil: 'networkidle0' });
    if (key) {
      await page.keyboard.press(key);
      await new Promise((r) => setTimeout(r, 600));
    }
    const png = await page.screenshot({ clip: { x: 0, y: 0, width: 1280, height: 800 } });
    await sharp(png).resize(1280).webp({ quality: 78 }).toFile(`${dir}/${name}.${lang}.webp`);
    console.log(`${dir}/${name}.${lang}.webp`);
  }
  await browser.close();
}
