// Движок рекламных роликов: строит сцены из сценария scenarios/<имя>.js и анимирует их Web Animations API.
// Сценарий вызывает scenario({ scenes: [...] }); типы сцен и их поля — в README.md.
// Время каждой сцены считается подряд; соседние сцены сменяются плавно (FADE).
(() => {
  const params = new URLSearchParams(location.search);
  const LANG = params.get('lang') || 'ru';
  const NAME = params.get('scenario') || 'pains';
  const RENDER = params.has('render');
  const FADE = 0.3;
  const EASE = 'cubic-bezier(.2,.8,.2,1)', INOUT = 'cubic-bezier(.45,0,.25,1)', POP = 'cubic-bezier(.34,1.56,.64,1)';
  const CHECK = '<svg width="44" height="44" viewBox="0 0 24 24" fill="none" stroke="#fff" stroke-width="3.2" stroke-linecap="round" stroke-linejoin="round"><path d="M20 6 9 17l-5-5"/></svg>';
  const COIN_COLORS = ['#059669', '#4338ca', '#d97706'];
  document.documentElement.lang = LANG;
  document.body.classList.add(LANG);

  function el(tag, cls, parent, html) {
    const e = document.createElement(tag);
    if (cls) e.className = cls;
    if (html != null) e.innerHTML = html;
    (parent || document.body).appendChild(e);
    return e;
  }
  const anim = (e, kf, start, dur, easing = EASE) =>
    e.animate(kf, { delay: start * 1000, duration: dur * 1000, fill: 'both', easing });
  const pop = (e, t) => anim(e, [{ opacity: 0, transform: 'translateY(40px) scale(.9)' }, { opacity: 1, transform: 'none' }], t, 0.55, POP);
  const rise = (e, t, d = 0.6) => anim(e, [{ opacity: 0, transform: 'translateY(50px)' }, { opacity: 1, transform: 'none' }], t, d);
  // Текст сцены на текущем языке: { ru, en, th }. Нет перевода — ошибка, а не тихий русский текст.
  const tr = (text, what) => {
    if (text == null) return text;
    if (!(LANG in text)) throw new Error(`нет перевода (${LANG}): ${what}`);
    return text[LANG];
  };

  function frame(start, dur, first, last) {
    const s = el('div', 'scene');
    const from = first ? start : start - FADE, len = start + dur - from, f = FADE / len;
    anim(s, [{ opacity: first ? 1 : 0 }, { opacity: 1, offset: first ? 0 : f }, { opacity: 1, offset: 1 - f / 2 }, { opacity: last ? 1 : 0 }], from, len, 'linear');
    return s;
  }

  // Красная плашка «боль», зачёркивается; затем заголовок-решение с галочкой и подзаголовок.
  function headline(s, start, { pain, title, sub, emoji = '😩' }) {
    const txt = el('div', 'text', s);
    let t = 0.3;
    if (pain) {
      const p = el('div', 'pain', txt);
      const inner = el('div', 'pain-inner', p, `${emoji} ${pain}`);
      const strike = el('div', 'strike', p);
      anim(p, [{ opacity: 0, transform: 'scale(.85)' }, { opacity: 1, transform: 'none' }], start + 0.05, 0.45, POP);
      anim(strike, [{ transform: 'scaleX(0)' }, { transform: 'scaleX(1)' }], start + 1.1, 0.35);
      anim(inner, [{ opacity: 1 }, { opacity: .6 }], start + 1.2, 0.3);
      t = 1.25;
    }
    rise(el('div', 'title', txt, `<span class="check">${CHECK}</span><span>${title}</span>`), start + t);
    if (sub) rise(el('div', 'sub', txt, sub), start + t + 0.2);
    return t;
  }

  const SCENES = {
    hook(s, start, sc) {
      const [l1, l2, q] = tr(sc.text, 'hook');
      const h = el('div', 'hook', s);
      pop(el('div', 'l1', h, l1), start + 0.1);
      if (l2) pop(el('div', 'l2', h, l2), start + 0.6);
      if (q) pop(el('div', 'q', h, q), start + 1.25);
      const spots = [[90, 1500, -20], [820, 1560, 25], [300, 1700, 10], [640, 1680, -15], [900, 1300, 30], [60, 1250, -30], [480, 1520, 5]];
      if (sc.flying !== false) spots.forEach(([x, y, r], i) => {
        const m = el('div', 'money', s, sc.flying || '💸');
        m.style.left = x + 'px'; m.style.top = y + 'px';
        anim(m, [{ opacity: 0, transform: `translateY(0) rotate(${r}deg)` }, { opacity: 1, offset: .2 },
          { opacity: 0, transform: `translateY(-1300px) rotate(${-r * 2}deg)` }], start + 0.7 + i * 0.12, 1.6, 'cubic-bezier(.4,0,.6,1)');
      });
    },

    shot(s, start, sc) {
      const [pain, title, sub] = tr(sc.text, sc.img);
      const t = headline(s, start, { pain, title, sub, emoji: sc.emoji });
      const H = Math.min(780, 960 / sc.aspect), W = H * sc.aspect;
      const card = el('div', 'card', s);
      Object.assign(card.style, { width: W + 'px', height: H + 'px', left: (1080 - W) / 2 + 'px' });
      anim(card, [{ opacity: 0, transform: 'translateY(240px) scale(.94)' }, { opacity: 1, transform: 'none' }], start + 0.3, 0.7);
      const layer = el('div', 'layer', card, `<img src="../../site/content/guides/${sc.img}.${LANG}.webp">`);
      const view = v => `scale(${W / v.w}) translate(${-v.x}px, ${-v.y}px)`;
      const to = sc.to || sc.from;
      anim(layer, [{ transform: view(sc.from) }, { transform: view(to) }], start + t - 0.05, sc.dur - t - 0.15, INOUT);
      (sc.rings || []).forEach(r => {
        const ring = el('div', 'ring' + (r.ok ? ' ok' : ''), layer);
        Object.assign(ring.style, { left: r.x + 'px', top: r.y + 'px', width: r.w + 'px', height: r.h + 'px' });
        anim(ring, [{ opacity: 0, transform: 'scale(1.25)' }, { opacity: 1, transform: 'none' }], start + r.at, 0.4, POP);
      });
      if (sc.typing) {
        const ty = sc.typing;
        const box = el('div', 'typing', layer);
        Object.assign(box.style, { left: ty.x + 'px', top: ty.y + 'px', width: ty.w + 'px', height: ty.h + 'px' });
        anim(box, [{ opacity: 0 }, { opacity: 1 }], start + ty.at - 0.05, 0.05, 'linear');
        [...ty.text].forEach((ch, k) => anim(el('span', '', box, ch), [{ opacity: 0 }, { opacity: 1 }], start + ty.at + k * 0.22, 0.01, 'linear'));
        anim(el('span', 'caret', box), [{ opacity: 1 }, { opacity: 1, offset: .5 }, { opacity: 0, offset: .5 }, { opacity: 0 }], start + ty.at, 0.8, 'linear')
          .effect.updateTiming({ iterations: 4 });
      }
      (sc.coins || []).forEach((c, k) => {
        const coin = el('div', 'coin', s, c);
        Object.assign(coin.style, { left: [70, 470, 860][k % 3] + 'px', top: [1560, 1600, 1540][k % 3] + 'px', color: COIN_COLORS[k % 3] });
        pop(coin, start + 1.7 + k * 0.2);
        anim(coin, [{ translate: '0 0' }, { translate: '0 -30px' }], start + 1.7, sc.dur - 1.7, 'ease-in-out');
      });
    },

    chat(s, start, sc) {
      const C = tr(sc.text, 'chat');
      headline(s, start, { pain: C.pain, title: C.title, emoji: sc.emoji || '⏳' });
      const chat = el('div', 'chat', s);
      rise(chat, start + 0.3, 0.7);
      el('div', 'chat-head', chat, '<span class="spark">✳</span> Claude');
      C.messages.forEach((m, k) => {
        let e;
        if (m.receipt) e = el('div', 'receipt', chat, `<div class="h">${m.receipt}</div><div class="row"></div><div class="row" style="width:70%"></div><div class="row"></div><div class="row" style="width:55%"></div><div class="tot"><span>Σ</span><span>${m.total}</span></div>`);
        else if (m.tool) e = el('div', 'tool', chat, `<i></i> ${m.tool}`);
        else e = el('div', 'msg ' + (m.me ? 'me' : 'bot'), chat, m.me || m.bot);
        pop(e, start + (m.at ?? 1.0 + k * 0.5));
      });
    },

    cta(s, start, sc) {
      const C = tr(sc.text, 'cta');
      const box = el('div', 'cta', s);
      const logo = el('img', '', box);
      logo.src = '../../crates/app/static/favicon.svg';
      pop(logo, start + 0.1);
      rise(el('div', 'name', box, C.name), start + 0.35);
      rise(el('div', 'tag', box, C.tag), start + 0.55);
      const btn = el('div', 'btn', box, C.btn + ' →');
      pop(btn, start + 0.9);
      anim(btn, [{ scale: 1 }, { scale: 1.06 }, { scale: 1 }], start + 1.6, 0.9, 'ease-in-out').effect.updateTiming({ iterations: 2 });
      rise(el('div', 'url', box, sc.url || 'family-budget-555.ru'), start + 1.15);
      rise(el('div', 'langs', box, C.langs || 'Русский · English · ไทย'), start + 1.35);
    },
  };

  function build({ scenes }) {
    const total = scenes.reduce((sum, sc) => sum + sc.dur, 0);
    ['b1', 'b2', 'b3'].forEach((id, k) => anim(document.getElementById(id), [{ transform: 'translate(0,0)' },
      { transform: ['translate(160px,120px)', 'translate(-180px,-260px)', 'translate(220px,-120px)'][k] }], 0, total, 'ease-in-out'));
    let start = 0;
    window.SCENE_TIMES = [];
    scenes.forEach((sc, i) => {
      window.SCENE_TIMES.push([start, sc.dur]);
      if (!SCENES[sc.type]) throw new Error(`неизвестный тип сцены: ${sc.type}`);
      SCENES[sc.type](frame(start, sc.dur, i === 0, i === scenes.length - 1), start, sc);
      start += sc.dur;
    });
    window.TOTAL = total;
  }

  let resolveScenario;
  const loaded = new Promise(r => (resolveScenario = r));
  window.scenario = s => resolveScenario(s);
  window.seek = t => document.getAnimations().forEach(a => { a.pause(); a.currentTime = t * 1000; });
  window.ready = (async () => {
    const script = el('script', '', document.head);
    script.src = `scenarios/${NAME}.js`;
    await new Promise((ok, fail) => { script.onload = ok; script.onerror = () => fail(new Error(`нет сценария: ${NAME}`)); });
    build(await loaded);
    await document.fonts.ready;
    await Promise.all([...document.images].map(img => img.decode().catch(() => { throw new Error('нет кадра: ' + img.src); })));
    if (RENDER) window.seek(0);
  })();
})();
