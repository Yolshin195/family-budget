// Рендер рекламного ролика (index.html) в MP4 1080×1920, 30 к/с — для TikTok и Instagram Reels, на трёх языках.
// Покадрово: анимации на паузе, window.seek(t) → скриншот → ffmpeg. Нужны chromium и ffmpeg.
//
//   node marketing/promo-video/render.mjs            # все языки → marketing/promo-video/out/promo.<язык>.mp4
//   node marketing/promo-video/render.mjs en         # один язык
//   node marketing/promo-video/render.mjs ru 5.2     # один кадр (секунда 5.2) → out/frame.ru.png, для отладки
//
// CHROMIUM — путь к браузеру (по умолчанию /usr/bin/chromium-browser), FFMPEG — к ffmpeg, FPS — частота кадров.
import { spawn } from 'node:child_process';
import { mkdirSync } from 'node:fs';
import { dirname, join } from 'node:path';
import { fileURLToPath, pathToFileURL } from 'node:url';
import puppeteer from 'puppeteer-core';

const here = dirname(fileURLToPath(import.meta.url));
const out = join(here, 'out');
mkdirSync(out, { recursive: true });
const fps = Number(process.env.FPS || 30);
const [langArg, frameArg] = process.argv.slice(2);
const langs = langArg ? [langArg] : ['ru', 'en', 'th'];

const browser = await puppeteer.launch({
  executablePath: process.env.CHROMIUM || '/usr/bin/chromium-browser',
  args: ['--no-sandbox', '--disable-dev-shm-usage', '--allow-file-access-from-files', '--font-render-hinting=none'],
});
try {
  for (const lang of langs) {
    const page = await browser.newPage();
    await page.setViewport({ width: 1080, height: 1920, deviceScaleFactor: 1 });
    const url = pathToFileURL(join(here, 'index.html'));
    url.search = `?lang=${lang}&render`;
    await page.goto(url.href, { waitUntil: 'load' });
    await page.evaluate(() => window.ready);
    const total = await page.evaluate(() => window.TOTAL);

    if (frameArg != null) {
      await page.evaluate(t => window.seek(t), Number(frameArg));
      await page.screenshot({ path: join(out, `frame.${lang}.png`) });
      console.log(`out/frame.${lang}.png`);
      continue;
    }

    // Поток кадров — в ffmpeg; тихая дорожка звука — некоторые площадки не любят видео без аудио.
    const file = join(out, `promo.${lang}.mp4`);
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
      await page.evaluate(t => window.seek(t), i / fps);
      const jpg = await page.screenshot({ type: 'jpeg', quality: 95 });
      if (!ffmpeg.stdin.write(jpg)) await new Promise(r => ffmpeg.stdin.once('drain', r));
      if (i % fps === 0) process.stdout.write(`\r${lang}: ${(i / fps).toFixed(0)} / ${total.toFixed(1)} с`);
    }
    ffmpeg.stdin.end();
    await done;
    // Обложка для Reels/TikTok — кадр со вступлением.
    await page.evaluate(() => window.seek(1.9));
    await page.screenshot({ path: join(out, `cover.${lang}.jpg`), type: 'jpeg', quality: 90 });
    console.log(`\r${lang}: ${file}`);
    await page.close();
  }
} finally {
  await browser.close();
}
