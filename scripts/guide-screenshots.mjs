// Скриншоты приложения для гайдов сайта — сразу на трёх языках: <имя>.ru.webp, <имя>.en.webp, <имя>.th.webp.
// Снимает демо-семью под первым демо-пользователем через «Вход для разработки», поэтому приложение должно быть
// запущено локально с демо-данными: `make dev` (APP_ENV=dev, SEED_DEMO=1, пустой OIDC_ISSUER).
//
//   node scripts/guide-screenshots.mjs <папка гайда> <имя>=<путь>[@клавиша] ...
//   node scripts/guide-screenshots.mjs site/content/guides/first-budget overview=/budget quick-expense=/budget@n limits=/limits
//
// «@клавиша» — нажать клавишу после загрузки (n — окно «Новый расход»); «@селектор=текст» — ввести текст в поле,
// «@!селектор» — нажать на элемент, «@^селектор» — прокрутить страницу к элементу (кадр — с этого места);
// действий может быть несколько:
//   quick-place='/budget@n@#q-search=cafe@!#q-results [data-place]'
//   card-recipients='/transactions/<id>/edit@^details.more'
// APP_URL — адрес приложения
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
    // Имя — до первого «=»: в пути могут быть свои «=» (/budget?view=year).
    const name = spec.slice(0, spec.indexOf('='));
    const [path, ...actions] = spec.slice(spec.indexOf('=') + 1).split('@');
    await page.goto(base + path, { waitUntil: 'networkidle0' });
    for (const act of actions) {
      if (act.startsWith('^')) await page.$eval(act.slice(1), (el) => el.scrollIntoView({ block: 'start' }));
      else if (act.startsWith('!')) await page.click(act.slice(1));
      else if (act.includes('=')) await page.type(act.slice(0, act.indexOf('=')), act.slice(act.indexOf('=') + 1));
      else await page.keyboard.press(act);
      await new Promise((r) => setTimeout(r, 600));
    }
    const y = await page.evaluate(() => window.scrollY);
    const png = await page.screenshot({ clip: { x: 0, y, width: 1280, height: 800 } });
    await sharp(png).resize(1280).webp({ quality: 78 }).toFile(`${dir}/${name}.${lang}.webp`);
    console.log(`${dir}/${name}.${lang}.webp`);
  }
  await browser.close();
}
