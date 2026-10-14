// Скриншот «Обзора» для лендинга: headless Chromium открывает демо гостя и снимает 1280×800.
// Запуск (контейнер Node + Chromium, приложение — на машине разработчика):
//   make screenshot   → web/brand/overview.png → `npm run assets` → crates/app/static/overview.webp
import puppeteer from 'puppeteer-core';

const base = process.env.APP_URL || 'http://localhost:8090';
const lang = process.env.SHOT_LANG || 'ru';
const browser = await puppeteer.launch({
  executablePath: process.env.CHROMIUM || '/usr/bin/chromium-browser',
  args: ['--no-sandbox', '--disable-dev-shm-usage', ...(process.env.HOST_RULES ? [`--host-resolver-rules=${process.env.HOST_RULES}`] : [])],
});
const page = await browser.newPage();
await page.setViewport({ width: 1280, height: 800, deviceScaleFactor: 2 });
await page.emulateMediaFeatures([{ name: 'prefers-color-scheme', value: 'light' }]);
await page.goto(`${base}/?lang=${lang}`, { waitUntil: 'networkidle0' });
await Promise.all([page.waitForNavigation({ waitUntil: 'networkidle0' }), page.click('form[action="/demo"] button')]);
// Бюджет недели/месяца: развернуть дерево до второго уровня, убрать баннер «только просмотр».
await page.evaluate(() => document.querySelector('.readonly-banner')?.remove());
await page.screenshot({ path: 'web/brand/overview.png', clip: { x: 0, y: 0, width: 1280, height: 800 } });
await browser.close();
console.log('web/brand/overview.png готов:', page.url());
