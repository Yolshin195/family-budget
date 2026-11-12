// Рендер рекламного ролика в MP4 1080×1920, 30 к/с — для TikTok и Instagram Reels, на трёх языках.
// Покадрово: анимации на паузе, window.seek(t) → скриншот → ffmpeg. Нужны Chromium и ffmpeg.
//
//   node marketing/promo-video/render.mjs pains             # все языки → out/pains.<язык>.mp4 и out/pains.cover.<язык>.jpg
//   node marketing/promo-video/render.mjs pains th          # один язык
//   node marketing/promo-video/render.mjs pains ru 7.2      # один кадр → out/pains.frame.ru.png
//   node marketing/promo-video/render.mjs pains ru sheet    # конец каждой сцены одним листом → out/pains.sheet.ru.png
//
// CHROMIUM — путь к браузеру (по умолчанию /usr/bin/chromium-browser), FFMPEG — к ffmpeg, FPS — частота кадров.
import { spawn } from 'node:child_process';
import { mkdirSync } from 'node:fs';
import { dirname, join } from 'node:path';
import { fileURLToPath, pathToFileURL } from 'node:url';
import puppeteer from 'puppeteer-core';
import sharp from 'sharp';

const here = dirname(fileURLToPath(import.meta.url));
const out = join(here, 'out');
mkdirSync(out, { recursive: true });
const fps = Number(process.env.FPS || 30);
const [name, langArg, mode] = process.argv.slice(2);
if (!name) {
  console.error('нужно: node marketing/promo-video/render.mjs <сценарий> [ru|en|th] [секунда|sheet]');
  process.exit(1);
}
const langs = langArg ? [langArg] : ['ru', 'en', 'th'];

const browser = await puppeteer.launch({
  executablePath: process.env.CHROMIUM || '/usr/bin/chromium-browser',
  args: ['--no-sandbox', '--disable-dev-shm-usage', '--allow-file-access-from-files', '--font-render-hinting=none'],
});
try {
  for (const lang of langs) {
    const page = await browser.newPage();
    const errors = [];
    page.on('pageerror', e => errors.push(e.message));
    await page.setViewport({ width: 1080, height: 1920, deviceScaleFactor: 1 });
    const url = pathToFileURL(join(here, 'index.html'));
    url.search = `?scenario=${name}&lang=${lang}&render`;
    await page.goto(url.href, { waitUntil: 'load' });
    await page.evaluate(() => window.ready).catch(e => { throw new Error(`${lang}: ${e.message} ${errors.join('; ')}`); });
    const { total, scenes } = await page.evaluate(() => ({ total: window.TOTAL, scenes: window.SCENE_TIMES }));
    const seek = t => page.evaluate(t => window.seek(t), t);

    if (mode === 'sheet') {
      // Кадр за 0,4 с до конца каждой сцены — всё уже появилось; лист для проверки глазами.
      const w = 360, h = 640;
      const tiles = [];
      for (const [start, dur] of scenes) {
        await seek(start + dur - 0.4);
        tiles.push(await sharp(await page.screenshot({ type: 'png' })).resize(w, h).toBuffer());
      }
      const file = join(out, `${name}.sheet.${lang}.png`);
      await sharp({ create: { width: w * tiles.length, height: h, channels: 3, background: '#fff' } })
        .composite(tiles.map((input, i) => ({ input, left: i * w, top: 0 }))).png().toFile(file);
      console.log(file);
      continue;
    }
    if (mode != null) {
      await seek(Number(mode));
      const file = join(out, `${name}.frame.${lang}.png`);
      await page.screenshot({ path: file });
      console.log(file);
      continue;
    }

    // Поток кадров — в ffmpeg; тихая дорожка звука — некоторые площадки не любят видео без аудио.
    const file = join(out, `${name}.${lang}.mp4`);
    const ffmpeg = spawn(process.env.FFMPEG || 'ffmpeg', [
      '-y', '-loglevel', 'error',
      '-f', 'image2pipe', '-framerate', String(fps), '-c:v', 'mjpeg', '-i', '-',
      '-f', 'lavfi', '-i', 'anullsrc=channel_layout=stereo:sample_rate=44100',
      '-shortest', '-c:v', 'libx264', '-preset', 'slow', '-crf', '20', '-pix_fmt', 'yuv420p',
      '-c:a', 'aac', '-b:a', '64k', '-movflags', '+faststart', file,
    ], { stdio: ['pipe', 'inherit', 'inherit'] });
    const done = new Promise((ok, fail) => ffmpeg.on('close', code => (code ? fail(new Error(`ffmpeg: ${code}`)) : ok())));

    const frames = Math.round(total * fps);
    for (let i = 0; i < frames; i++) {
      await seek(i / fps);
      const jpg = await page.screenshot({ type: 'jpeg', quality: 95 });
      if (!ffmpeg.stdin.write(jpg)) await new Promise(r => ffmpeg.stdin.once('drain', r));
      if (i % (fps * 5) === 0) console.log(`${lang}: ${(i / fps).toFixed(0)} / ${total.toFixed(1)} с`);
    }
    ffmpeg.stdin.end();
    await done;
    // Обложка — конец первой сцены.
    await seek(scenes[0][1] - 0.5);
    await page.screenshot({ path: join(out, `${name}.cover.${lang}.jpg`), type: 'jpeg', quality: 90 });
    console.log(`${lang}: ${file}`);
    await page.close();
  }
} finally {
  await browser.close();
}
