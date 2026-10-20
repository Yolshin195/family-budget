// Схемы окон ChatGPT для гайда «Как подключить ChatGPT» — сразу на трёх языках: <имя>.ru|en|th.webp.
// Это не скриншоты: снять ChatGPT на русском и тайском нельзя (его настройки — по-английски), поэтому окно
// нарисовано заново — подписи ChatGPT как на экране, второстепенный текст серыми полосами, метки 1, 2, 3…
// у нужных полей и пояснения к ним на языке гайда. ChatGPT поменял окно — поправьте сцену и перерисуйте:
//
//   node scripts/chatgpt-schemes.mjs site/content/guides/connect-chatgpt
//
// ONLY=chatgpt-add,chatgpt-oauth — только эти схемы; CHROMIUM — путь к браузеру.
import { mkdtempSync, writeFileSync } from 'fs';
import { tmpdir } from 'os';
import { join, resolve } from 'path';
import puppeteer from 'puppeteer-core';
import sharp from 'sharp';

const out = process.argv[2];
if (!out) {
  console.error('нужно: node scripts/chatgpt-schemes.mjs <папка гайда>');
  process.exit(1);
}
const tmp = mkdtempSync(join(tmpdir(), 'chatgpt-schemes-'));
const fonts = `file://${resolve('node_modules/@fontsource-variable')}`;
const URL = 'https://family-budget-555.ru/mcp';

const T = {
  ru: {
    name: 'Семейный бюджет', chat: 'Кофе 120 бат наличными', legend: 'Что сделать',
    add: [
      ['Plugins — в меню слева. Если меню скрыто, откройте его значком в левом верхнем углу.'],
      ['Add — кнопка справа вверху.'],
      ['Add custom MCP server. В некоторых аккаунтах пункт называется Create custom MCP server.'],
    ],
    server: [
      ['Name — как будет называться плагин, например «Семейный бюджет».'],
      ['Server URL — адрес из блока «Как подключить» на странице «Ассистенты».'],
      ['Authentication — OAuth.'],
      ['Advanced OAuth settings — откройте и настройте вход (следующий шаг).'],
      ['Отметьте I understand and want to continue.'],
      ['Create as a plugin — нажмите последним, когда вход уже настроен.'],
    ],
    oauth: [
      ['Registration method — User-Defined OAuth Client.'],
      ['OAuth client ID — Client ID со страницы «Ассистенты».'],
      ['OAuth client secret — секрет со страницы «Ассистенты». Если секрета там нет, оставьте поле пустым.'],
      ['Token endpoint auth method — как на странице «Ассистенты»: client_secret_post, а без секрета — none.'],
      ['Base scopes — пусто. Если там что-то написано, сотрите.'],
      ['Остальное ChatGPT заполнил сам — не меняйте.'],
    ],
    connect: [
      ['Continue — откроется вход в бюджет. Войдите тем же аккаунтом, в котором открыли доступ ChatGPT.'],
      ['После входа вас вернёт в ChatGPT, и плагин будет подключён.'],
    ],
    chatLegend: [
      ['Напишите @ и выберите плагин «Семейный бюджет».'],
      ['Напишите, что записать или что узнать. Перед записью ChatGPT попросит подтвердить действие.'],
    ],
  },
  en: {
    name: 'Family budget', chat: 'Coffee 120 baht cash', legend: 'What to do',
    add: [
      ['Plugins — in the left menu. If the menu is hidden, open it with the icon in the top-left corner.'],
      ['Add — the button at the top right.'],
      ['Add custom MCP server. Some accounts call it Create custom MCP server.'],
    ],
    server: [
      ['Name — what the plugin will be called, for example “Family budget”.'],
      ['Server URL — the address from “How to connect” on the “Assistants” page.'],
      ['Authentication — OAuth.'],
      ['Advanced OAuth settings — open it and set up sign-in (the next step).'],
      ['Tick I understand and want to continue.'],
      ['Create as a plugin — click it last, once sign-in is set up.'],
    ],
    oauth: [
      ['Registration method — User-Defined OAuth Client.'],
      ['OAuth client ID — the Client ID from the “Assistants” page.'],
      ['OAuth client secret — the secret from the “Assistants” page. If there is no secret there, leave the field empty.'],
      ['Token endpoint auth method — as on the “Assistants” page: client_secret_post, or none without a secret.'],
      ['Base scopes — empty. If something is written there, delete it.'],
      ['ChatGPT has filled in the rest itself — do not change it.'],
    ],
    connect: [
      ['Continue — the budget sign-in opens. Sign in with the account where you granted ChatGPT access.'],
      ['After signing in you return to ChatGPT, and the plugin is connected.'],
    ],
    chatLegend: [
      ['Type @ and pick the “Family budget” plugin.'],
      ['Write what to record or what to find out. Before recording, ChatGPT asks you to confirm.'],
    ],
  },
  th: {
    name: 'งบครอบครัว', chat: 'กาแฟ 120 บาท เงินสด', legend: 'สิ่งที่ต้องทำ',
    add: [
      ['Plugins — ในเมนูด้านซ้าย ถ้าเมนูซ่อนอยู่ ให้เปิดด้วยไอคอนมุมซ้ายบน'],
      ['Add — ปุ่มด้านขวาบน'],
      ['Add custom MCP server บางบัญชีจะเป็น Create custom MCP server'],
    ],
    server: [
      ['Name — ชื่อของปลั๊กอิน เช่น “งบครอบครัว”'],
      ['Server URL — ที่อยู่จากหัวข้อ “วิธีเชื่อมต่อ” ในหน้า “ผู้ช่วย”'],
      ['Authentication — OAuth'],
      ['Advanced OAuth settings — เปิดแล้วตั้งค่าการเข้าสู่ระบบ (ขั้นถัดไป)'],
      ['ติ๊ก I understand and want to continue'],
      ['Create as a plugin — กดเป็นอย่างสุดท้าย หลังตั้งค่าการเข้าสู่ระบบแล้ว'],
    ],
    oauth: [
      ['Registration method — User-Defined OAuth Client'],
      ['OAuth client ID — Client ID จากหน้า “ผู้ช่วย”'],
      ['OAuth client secret — secret จากหน้า “ผู้ช่วย” ถ้าไม่มี secret ให้เว้นว่าง'],
      ['Token endpoint auth method — ตามหน้า “ผู้ช่วย”: client_secret_post หรือ none ถ้าไม่มี secret'],
      ['Base scopes — เว้นว่าง ถ้ามีข้อความอยู่ให้ลบออก'],
      ['ส่วนที่เหลือ ChatGPT กรอกให้เองแล้ว — ไม่ต้องเปลี่ยน'],
    ],
    connect: [
      ['Continue — จะเปิดหน้าเข้าสู่ระบบของงบ ให้เข้าสู่ระบบด้วยบัญชีเดียวกับที่เปิดสิทธิ์ให้ ChatGPT'],
      ['หลังเข้าสู่ระบบ ระบบจะพากลับไปที่ ChatGPT และปลั๊กอินจะเชื่อมต่อแล้ว'],
    ],
    chatLegend: [
      ['พิมพ์ @ แล้วเลือกปลั๊กอิน “งบครอบครัว”'],
      ['เขียนว่าจะบันทึกหรือถามอะไร ก่อนบันทึก ChatGPT จะขอให้ยืนยัน'],
    ],
  },
};

const css = `
@font-face { font-family: Inter; src: url(${fonts}/inter/files/inter-latin-wght-normal.woff2) format('woff2'); font-weight: 100 900; }
@font-face { font-family: Inter; src: url(${fonts}/inter/files/inter-cyrillic-wght-normal.woff2) format('woff2'); font-weight: 100 900; unicode-range: U+0400-04FF; }
@font-face { font-family: 'Noto Sans Thai'; src: url(${fonts}/noto-sans-thai/files/noto-sans-thai-thai-wght-normal.woff2) format('woff2'); font-weight: 100 900; }
* { box-sizing: border-box; margin: 0; padding: 0; }
body { width: 800px; font-family: Inter, 'Noto Sans Thai', sans-serif; color: #0d0d0d; background: #eef0f4; -webkit-font-smoothing: antialiased; }
.wrap { display: flex; flex-direction: column; gap: 28px; padding: 36px 40px 40px; align-items: flex-start; }
.mock { width: 720px; position: relative; }
.legend { width: 720px; background: #fff; border-radius: 16px; padding: 24px 24px 20px; box-shadow: 0 1px 2px rgba(0,0,0,.06); }
.legend h3 { font-size: 18px; font-weight: 600; color: #4b5563; margin-bottom: 14px; text-transform: none; }
.legend ol { list-style: none; display: grid; gap: 14px; }
.legend li { display: flex; gap: 14px; font-size: 21px; line-height: 1.45; }
.legend li .n { flex: 0 0 28px; }
.n { display: inline-flex; align-items: center; justify-content: center; width: 28px; height: 28px; border-radius: 999px; background: #4f46e5; color: #fff; font-size: 15px; font-weight: 700; box-shadow: 0 0 0 3px #fff; }
.pin { position: absolute; z-index: 5; }
.hl { outline: 3px solid #4f46e5; outline-offset: 3px; border-radius: 10px; }
.dialog { background: #fff; border-radius: 24px; padding: 28px 30px; box-shadow: 0 12px 40px rgba(0,0,0,.12); }
.title { font-size: 26px; font-weight: 700; margin-bottom: 18px; }
.label { font-size: 15px; font-weight: 500; margin: 14px 0 7px; display: flex; justify-content: space-between; align-items: center; }
.input { border: 1px solid #d9d9e0; border-radius: 12px; padding: 11px 14px; font-size: 16px; background: #fff; min-height: 46px; display: flex; align-items: center; justify-content: space-between; }
.input.muted { color: #8e8ea0; }
.input.gray { background: #f4f4f6; color: #6b6b7b; font-size: 14px; }
.chev::after { content: '▾'; color: #6b6b7b; margin-left: 10px; }
.seg { display: inline-flex; background: #f1f1f4; border-radius: 10px; padding: 3px; font-size: 14px; }
.seg span { padding: 5px 12px; border-radius: 8px; color: #8e8ea0; }
.seg span.on { background: #fff; color: #0d0d0d; box-shadow: 0 1px 2px rgba(0,0,0,.08); }
.bar { height: 10px; border-radius: 6px; background: #e6e6ea; margin: 6px 0; }
.card { border: 1px solid #e3e3e8; border-radius: 14px; padding: 14px 16px; margin-top: 14px; }
.warn { border: 1px solid #f6c3a8; background: #fff6f1; border-radius: 12px; padding: 12px 14px; margin-top: 16px; font-size: 15px; display: flex; gap: 10px; align-items: center; }
.check { display: flex; gap: 12px; align-items: flex-start; }
.box { flex: 0 0 22px; height: 22px; border-radius: 6px; background: #0d0d0d; color: #fff; display: flex; align-items: center; justify-content: center; font-size: 15px; margin-top: 2px; }
.btns { display: flex; justify-content: space-between; align-items: center; margin-top: 20px; }
.btn { border-radius: 999px; padding: 11px 20px; font-size: 16px; border: 1px solid #d9d9e0; background: #fff; }
.btn.dark { background: #0d0d0d; color: #fff; border-color: #0d0d0d; }
.link { text-decoration: underline; font-size: 15px; }
.sec { font-size: 18px; font-weight: 600; margin-top: 4px; }
.hint { font-size: 13px; color: #6b6b7b; line-height: 1.4; }
.side { position: absolute; }
`;


function legend(t, items) {
  return `<div class="legend"><h3>${t.legend}</h3><ol>${items.map((it, i) => `<li><span class="n">${i + 1}</span><span>${it[0]}</span></li>`).join('')}</ol></div>`;
}

const scenes = {
  'chatgpt-add': (t) => `
    <div class="mock" style="height:330px">
      <div style="display:flex;background:#fff;border-radius:20px;overflow:hidden;box-shadow:0 12px 40px rgba(0,0,0,.10);height:330px">
        <div style="flex:0 0 200px;background:#f9f9fa;padding:18px 12px;font-size:15px;display:grid;gap:4px;align-content:start">
          <div style="padding:8px 10px">New chat</div><div style="padding:8px 10px">Search chats</div>
          <div data-pin="1" class="hl" style="padding:8px 10px;background:#ececf1;font-weight:600">Plugins</div>
          <div style="padding:8px 10px">Projects</div><div class="bar" style="width:70%;margin:14px 10px 6px"></div><div class="bar" style="width:55%;margin:6px 10px"></div>
        </div>
        <div style="flex:1;padding:26px 24px;position:relative">
          <div style="display:flex;justify-content:space-between;align-items:center">
            <div style="font-size:28px;font-weight:600">Plugins</div>
            <div style="display:flex;gap:10px;align-items:center"><div class="input muted" style="width:170px;min-height:40px;border-radius:999px;font-size:14px">Search plugins</div>
            <div data-pin="2" class="btn dark hl" style="padding:9px 16px">Add ▾</div></div>
          </div>
          <div class="bar" style="width:62%;margin-top:12px"></div>
          <div style="display:flex;gap:8px;margin-top:18px;font-size:15px"><span style="background:#f1f1f4;border-radius:999px;padding:7px 14px">Public</span><span style="padding:7px 14px;color:#6b6b7b">Personal</span></div>
          <div style="position:absolute;right:24px;top:80px;background:#fff;border:1px solid #e3e3e8;border-radius:16px;padding:8px;box-shadow:0 10px 30px rgba(0,0,0,.12);font-size:16px;width:270px;display:grid;gap:2px">
            <div style="padding:10px 12px">Create plugin</div><div style="padding:10px 12px">Upload plugin archive</div>
            <div data-pin="3" class="hl" style="padding:10px 12px;background:#f1f1f4;font-weight:600">Add custom MCP server</div>
          </div>
        </div>
      </div>
    </div>${legend(t, t.add)}`,

  'chatgpt-server': (t) => `
    <div class="mock">
      <div class="dialog">
        <div class="title">Add custom MCP server</div>
        <div style="display:flex;gap:14px;align-items:center"><div style="width:52px;height:52px;border-radius:12px;border:1px solid #e3e3e8;display:flex;align-items:center;justify-content:center;font-size:26px;color:#6b6b7b">+</div><div><div style="font-size:15px;font-weight:500">Icon (optional)</div><div class="bar" style="width:260px"></div></div></div>
        <div class="label">Name</div><div data-pin="1" class="input hl">${t.name}</div>
        <div class="label">Description (optional)</div><div class="input muted"><span class="bar" style="width:240px;margin:0"></span></div>
        <div class="label">Connection <span class="seg"><span class="on">Server URL</span><span>Tunnel</span></span></div><div data-pin="2" class="input hl">${URL}</div>
        <div class="label">Authentication</div><div data-pin="3" class="input chev hl">OAuth</div>
        <div data-pin="4" class="card hl" style="display:flex;justify-content:space-between;align-items:center"><div><div style="font-size:15px;font-weight:600">Advanced OAuth settings</div><div class="bar" style="width:380px"></div></div><span style="font-size:20px;color:#6b6b7b">›</span></div>
        <div class="warn">⚠ <span>Custom MCP servers introduce risk.</span></div>
        <div data-pin="5" class="card check hl"><div class="box">✓</div><div><div style="font-size:16px;font-weight:600">I understand and want to continue</div><div class="bar" style="width:440px"></div><div class="bar" style="width:380px"></div></div></div>
        <div class="btns"><span class="link">Read the guide</span><span style="display:flex;gap:10px"><span class="btn">Cancel</span><span data-pin="6" class="btn dark hl">Create as a plugin</span></span></div>
      </div>
    </div>${legend(t, t.server)}`,

  'chatgpt-oauth': (t) => `
    <div class="mock">
      <div class="dialog">
        <div class="title" style="margin-bottom:6px">OAuth advanced settings</div><div class="bar" style="width:520px"></div><div class="bar" style="width:420px;margin-bottom:10px"></div>
        <div class="card" style="padding:16px 18px">
          <div class="sec">Client registration</div>
          <div class="label">Registration method</div><div data-pin="1" class="input chev hl">User-Defined OAuth Client</div>
          <div class="label">Callback URL</div><div class="input gray">https://chatgpt.com/connector_platform_oauth_redirect</div>
          <div class="label">OAuth client ID</div><div data-pin="2" class="input hl">chatgpt</div>
          <div class="label">OAuth client secret</div><div data-pin="3" class="input hl">••••••••••••••••</div>
          <div class="label">Token endpoint auth method</div><div data-pin="4" class="input chev hl">client_secret_post</div>
        </div>
        <div class="card" style="padding:16px 18px">
          <div class="sec">Scopes</div>
          <div class="label">Default scopes</div><div class="check" style="align-items:center"><div class="box" style="margin:0">✓</div><span style="font-size:16px">openid</span></div>
          <div class="label">Base scopes</div><div data-pin="5" class="input hl" style="min-height:64px"></div>
        </div>
        <div data-pin="6" class="card hl" style="padding:16px 18px">
          <div class="sec">OAuth endpoints · OpenID support</div>
          <div class="hint" style="margin-top:8px">Auth URL, Token URL, Authorization server base, Resource: ${URL}</div>
        </div>
      </div>
    </div>${legend(t, t.oauth)}`,

  'chatgpt-connect': (t) => `
    <div class="mock">
      <div class="dialog" style="width:560px;margin:0 auto">
        <div class="title" style="text-align:center">Connect ${t.name}</div>
        <div class="card" style="margin-top:0">
          <div style="font-size:15px;font-weight:600">Permissions always respected</div><div class="bar" style="width:90%"></div><div class="bar" style="width:60%"></div>
          <div style="font-size:15px;font-weight:600;margin-top:14px">You're in control</div><div class="bar" style="width:85%"></div><div class="bar" style="width:70%"></div>
          <div style="font-size:15px;font-weight:600;margin-top:14px">Connectors may introduce risk</div><div class="bar" style="width:88%"></div><div class="bar" style="width:50%"></div>
        </div>
        <div data-pin="1" class="btn dark hl" style="text-align:center;margin-top:18px">Continue to ${t.name} ↗</div>
      </div>
      <div data-pin="2" class="hl" style="margin:22px auto 0;width:560px;display:flex;align-items:center;gap:12px;background:#fff;border-radius:16px;padding:14px 18px;box-shadow:0 6px 20px rgba(0,0,0,.08)">
        <span style="font-size:22px">✓</span><span style="font-size:16px;font-weight:600">${t.name}</span><span class="bar" style="width:160px;margin:0 0 0 auto"></span>
      </div>
    </div>${legend(t, t.connect)}`,

  'chatgpt-chat': (t) => `
    <div class="mock" style="padding-top:10px">
      <div style="background:#fff;border:1px solid #e3e3e8;border-radius:16px;padding:8px;box-shadow:0 10px 30px rgba(0,0,0,.10);width:360px;font-size:16px;margin-left:20px">
        <div class="hint" style="padding:6px 10px">Plugins</div>
        <div data-pin="1" class="hl" style="padding:10px 12px;background:#f1f1f4;border-radius:10px;font-weight:600">${t.name}</div>
        <div style="padding:10px 12px"><span class="bar" style="display:block;width:150px;margin:4px 0"></span></div>
      </div>
      <div data-pin="2" class="hl" style="margin-top:16px;background:#fff;border:1px solid #e3e3e8;border-radius:28px;padding:16px 18px;box-shadow:0 6px 20px rgba(0,0,0,.08);display:flex;align-items:center;gap:10px;font-size:17px">
        <span style="font-size:22px;color:#6b6b7b">+</span>
        <span style="background:#e8edff;color:#3446c4;border-radius:8px;padding:3px 8px;font-weight:600">@${t.name}</span>
        <span>${t.chat}</span>
        <span style="margin-left:auto;width:36px;height:36px;border-radius:999px;background:#0d0d0d;color:#fff;display:flex;align-items:center;justify-content:center">↑</span>
      </div>
    </div>${legend(t, t.chatLegend)}`,
};

const browser = await puppeteer.launch({ executablePath: process.env.CHROMIUM || '/usr/bin/chromium-browser', args: ['--no-sandbox', '--allow-file-access-from-files'] });
const page = await browser.newPage();
await page.setViewport({ width: 800, height: 800, deviceScaleFactor: 1.5 });
const only = process.env.ONLY ? process.env.ONLY.split(',') : Object.keys(scenes);
for (const [lang, t] of Object.entries(T)) {
  for (const name of only) {
    const file = `${tmp}/${name}.${lang}.html`;
    writeFileSync(file, `<!doctype html><html lang="${lang}"><head><meta charset="utf-8"><style>${css}</style></head><body><div class="wrap">${scenes[name](t)}</div></body></html>`);
    await page.goto(`file://${file}`, { waitUntil: 'networkidle0' });
    await page.evaluate(() => document.fonts.ready);
    // Метка — у правого края подсвеченного элемента, по центру по высоте.
    await page.evaluate(() => {
      const mock = document.querySelector('.mock');
      const box = mock.getBoundingClientRect();
      for (const el of document.querySelectorAll('[data-pin]')) {
        const r = el.getBoundingClientRect();
        const pin = document.createElement('span');
        pin.className = 'n pin';
        pin.textContent = el.dataset.pin;
        pin.style.left = `${r.right - box.left + 10}px`;
        pin.style.top = `${r.top - box.top + r.height / 2 - 14}px`;
        mock.appendChild(pin);
      }
    });
    const h = await page.evaluate(() => Math.ceil(document.querySelector('.wrap').getBoundingClientRect().height));
    const png = await page.screenshot({ clip: { x: 0, y: 0, width: 800, height: h } });
    await sharp(png).resize(1200).webp({ quality: 80 }).toFile(`${out}/${name}.${lang}.webp`);
    console.log(`${out}/${name}.${lang}.webp`);
  }
}
await browser.close();
