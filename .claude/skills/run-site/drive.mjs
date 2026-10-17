// Смоук сайта в браузере: страница на трёх языках — компьютер в светлой теме и телефон в тёмной.
//
//   node .claude/skills/run-site/drive.mjs --page /articles/roadmap/ [--page /guides/limits/]
//        [--base http://localhost:8765] [--out site/.build/shots]
//
// --page — адрес русской версии; английская и тайская — с префиксами /en и /th.
// На каждую страницу и язык: снимок первого экрана и всей страницы (компьютер), первого экрана (телефон).
// Печатает ошибки консоли, неудачные запросы, ответы 4xx/5xx и горизонтальную прокрутку на телефоне;
// код выхода 1, если что-то нашлось. Снимки нужно посмотреть (Read) — пустой кадр тоже провал.
import { existsSync, mkdirSync } from 'node:fs';
import { execSync } from 'node:child_process';
import { createRequire } from 'node:module';

const root = execSync('git rev-parse --show-toplevel').toString().trim();
const puppeteer = createRequire(`${root}/package.json`)('puppeteer-core');

const args = { base: 'http://localhost:8765', out: `${root}/site/.build/shots`, page: [] };
for (let i = 2; i < process.argv.length; i += 2) {
  const key = process.argv[i].replace(/^--/, '');
  if (key === 'page') args.page.push(process.argv[i + 1]);
  else args[key] = process.argv[i + 1];
}
if (!args.page.length) args.page.push('/');
mkdirSync(args.out, { recursive: true });

// Chromium: CHROME, затем браузер облачной среды Claude, затем системный.
const chrome = process.env.CHROME
  || ['/opt/pw-browsers/chromium', '/usr/bin/chromium', '/usr/bin/chromium-browser', '/usr/bin/google-chrome'].find(existsSync);
if (!chrome) throw new Error('Не найден Chromium: укажите путь в CHROME=');

const devices = {
  desktop: { viewport: { width: 1280, height: 820 }, theme: 'light' },
  phone: { viewport: { width: 390, height: 844, deviceScaleFactor: 2, isMobile: true, hasTouch: true }, theme: 'dark' },
};
const problems = [];
const browser = await puppeteer.launch({ executablePath: chrome, args: ['--no-sandbox'] });

for (const [device, { viewport, theme }] of Object.entries(devices)) {
  const tab = await browser.newPage();
  await tab.setViewport(viewport);
  await tab.emulateMediaFeatures([{ name: 'prefers-color-scheme', value: theme }]);
  tab.on('console', (m) => m.type() === 'error' && problems.push(`${device}: console: ${m.text()}`));
  tab.on('requestfailed', (r) => problems.push(`${device}: не загрузилось: ${r.url()}`));
  tab.on('response', (r) => r.status() >= 400 && problems.push(`${device}: ${r.status()} ${r.url()}`));
  for (const page of args.page) {
    for (const lang of ['ru', 'en', 'th']) {
      const url = args.base + (lang === 'ru' ? '' : `/${lang}`) + page;
      const res = await tab.goto(url, { waitUntil: 'networkidle0' });
      const name = `${page.replace(/^\/|\/$/g, '').replace(/\//g, '-') || 'home'}.${lang}.${device}`;
      await tab.screenshot({ path: `${args.out}/${name}.png` });
      if (device === 'desktop') await tab.screenshot({ path: `${args.out}/${name}.full.png`, fullPage: true });
      const overflow = await tab.evaluate(() => document.documentElement.scrollWidth - window.innerWidth);
      if (overflow > 0) problems.push(`${device}: ${url}: горизонтальная прокрутка ${overflow}px`);
      console.log(`${res.status()} ${url} → ${name}.png`);
    }
  }
}
await browser.close();

console.log(problems.length ? `\nПроблемы (${problems.length}):\n  ${problems.join('\n  ')}` : '\nОшибок нет.');
console.log(`Снимки: ${args.out}`);
process.exit(problems.length ? 1 : 0);
