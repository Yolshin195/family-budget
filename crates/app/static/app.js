/* Семейный бюджет — поведение интерфейса (ТЗ, разделы 9–10). Без фреймворков: небольшой свой JS + htmx
   для частичных обновлений. Всё, что можно, работает и без JS: формы — обычные POST, «+ Расход» —
   ссылка на полную форму, «Ещё» на телефоне — якорь #more. */
(function () {
  'use strict';

  var doc = document, root = doc.documentElement, body = doc.body;
  var $ = function (sel, el) { return (el || doc).querySelector(sel); };
  var $$ = function (sel, el) { return Array.prototype.slice.call((el || doc).querySelectorAll(sel)); };
  var i18n = ($('#i18n') || {}).dataset || {};
  var lang = (body && body.dataset.lang) || root.lang || 'ru';
  var locale = lang === 'th' ? 'th-TH-u-ca-gregory' : lang === 'en' ? 'en-US' : 'ru-RU';
  var csrf = (body && body.dataset.csrf) || '';
  var canWrite = !!(body && body.dataset.canWrite);
  // Символ базовой валюты для сумм (฿); если символа нет — код.
  var baseSym = (body && (body.dataset.baseSym || body.dataset.base)) || '';

  function money(v) {
    return new Intl.NumberFormat(locale, { minimumFractionDigits: 2, maximumFractionDigits: 2 }).format(v);
  }
  function svgIcon(href) {
    var s = doc.createElementNS('http://www.w3.org/2000/svg', 'svg');
    s.setAttribute('class', 'icon');
    s.setAttribute('aria-hidden', 'true');
    var u = doc.createElementNS('http://www.w3.org/2000/svg', 'use');
    u.setAttribute('href', href);
    s.appendChild(u);
    return s;
  }
  function typing(e) {
    var t = e.target;
    return t && (t.isContentEditable || /^(INPUT|TEXTAREA|SELECT)$/.test(t.tagName));
  }

  /* ---------------------------------------------------------------- суммы: «120+45», запятая и точка */
  function evalAmount(text) {
    var s = String(text || '').replace(/[\s  ]/g, '').replace(/,/g, '.');
    if (!s) return NaN;
    if (!/^[-+*/.\d]+$/.test(s)) return NaN;
    var total = 0, sign = 1, term = '';
    function flush(t, sg) {
      var parts = t.split(/([*/])/), acc = null, op = '*';
      for (var i = 0; i < parts.length; i++) {
        var p = parts[i];
        if (p === '*' || p === '/') { op = p; continue; }
        var n = parseFloat(p);
        if (isNaN(n) || !/^-?\d*\.?\d+$/.test(p)) return NaN;
        acc = acc === null ? n : op === '*' ? acc * n : n === 0 ? NaN : acc / n;
      }
      return acc === null ? NaN : sg * acc;
    }
    for (var i = 0; i < s.length; i++) {
      var c = s[i];
      if ((c === '+' || c === '-') && i > 0 && !/[*/]$/.test(term)) {
        total += flush(term, sign); term = ''; sign = c === '-' ? -1 : 1;
      } else term += c;
    }
    total += flush(term, sign);
    return Math.round(total * 10000) / 10000;
  }

  /* ---------------------------------------------------------------- тосты (5 секунд, «Отменить») */
  var toasts = $('#toasts');
  function dismiss(t) { if (t && t.parentNode) t.parentNode.removeChild(t); }
  function arm(t) {
    var ms = parseInt(t.dataset.timeout || '5000', 10), timer = setTimeout(function () { dismiss(t); }, ms);
    t.addEventListener('mouseenter', function () { clearTimeout(timer); });
    t.addEventListener('focusin', function () { clearTimeout(timer); });
    t.addEventListener('mouseleave', function () { timer = setTimeout(function () { dismiss(t); }, 2000); });
  }
  function toast(text, opts) {
    opts = opts || {};
    if (!toasts) return;
    var t = doc.createElement('div');
    t.className = 'toast' + (opts.error ? ' toast-error' : '');
    t.dataset.timeout = '5000';
    var span = doc.createElement('span');
    span.className = 'toast-text';
    span.textContent = text;
    t.appendChild(span);
    if (opts.undo) {
      var b = doc.createElement('button');
      b.type = 'button'; b.className = 'toast-action'; b.textContent = i18n.undo || 'Undo';
      b.addEventListener('click', function () { dismiss(t); opts.undo(); });
      t.appendChild(b);
    }
    var x = doc.createElement('button');
    x.type = 'button'; x.className = 'toast-close'; x.setAttribute('aria-label', i18n.close || 'Close');
    x.appendChild(svgIcon(i18n.iconX || ''));
    x.addEventListener('click', function () { dismiss(t); });
    t.appendChild(x);
    toasts.appendChild(t);
    arm(t);
  }
  $$('.toast').forEach(arm);
  doc.addEventListener('click', function (e) {
    var c = e.target.closest && e.target.closest('[data-close-toast]');
    if (c) dismiss(c.closest('.toast'));
  });

  /* ---------------------------------------------------------------- боковое меню и «Ещё» */
  doc.addEventListener('click', function (e) {
    var t = e.target.closest && e.target.closest('[data-action]');
    if (!t) return;
    var a = t.dataset.action;
    if (a === 'toggle-sidebar') {
      var next = root.dataset.sidebar === 'collapsed' ? 'expanded' : 'collapsed';
      root.dataset.sidebar = next;
      try { localStorage.setItem('fb.sidebar', next); } catch (err) { /* приватный режим */ }
      var label = next === 'collapsed' ? t.dataset.labelExpand : t.dataset.labelCollapse;
      if (label) { t.setAttribute('aria-label', label); t.title = label; }
    } else if (a === 'open-more') {
      e.preventDefault();
      var m = $('#more'); m.classList.add('open');
      var first = $('a, button', $('.sheet', m)); if (first) first.focus();
    } else if (a === 'close-more') {
      e.preventDefault();
      $('#more').classList.remove('open');
      if (location.hash === '#more') history.replaceState(null, '', location.pathname + location.search);
    }
  });
  (function () {
    var t = $('[data-action="toggle-sidebar"]');
    if (t && root.dataset.sidebar === 'collapsed' && t.dataset.labelExpand) { t.setAttribute('aria-label', t.dataset.labelExpand); t.title = t.dataset.labelExpand; }
  })();
  // Меню проекта закрывается по клику снаружи.
  doc.addEventListener('click', function (e) {
    $$('details[data-menu][open]').forEach(function (d) { if (!d.contains(e.target)) d.open = false; });
  });

  /* ---------------------------------------------------------------- формы */
  // Автоотправка фильтров (без JS — кнопка «Показать»).
  doc.addEventListener('change', function (e) {
    var f = e.target.closest && e.target.closest('form[data-autosubmit]');
    if (f && /^(SELECT|INPUT)$/.test(e.target.tagName)) (f.requestSubmit ? f.requestSubmit() : f.submit());
  });
  // Список категорий/родителей — только того же типа, что выбран в поле «Тип».
  function filterOptions(select, kind) {
    $$('option', select).forEach(function (o) {
      var ok = !o.dataset.kind || o.dataset.kind === kind;
      o.hidden = !ok; o.disabled = !ok;
      if (!ok && o.selected) select.value = '';
    });
  }
  function bindFilters(scope) {
    $$('[data-filter-target]', scope).forEach(function (kindSel) {
      var target = $(kindSel.dataset.filterTarget);
      if (!target || kindSel.dataset.bound) return;
      kindSel.dataset.bound = '1';
      var sync = function () { filterOptions(target, kindSel.value); };
      kindSel.addEventListener('change', sync); sync();
    });
  }
  // Поля, зависящие от типа (форма плана): [data-kind-show] / [data-kind-hide] — по значению поля «Тип».
  function bindKindToggles(scope) {
    $$('form select[name=tx_kind]', scope).forEach(function (sel) {
      var form = sel.form;
      if (!form || sel.dataset.toggles) return;
      sel.dataset.toggles = '1';
      var sync = function () {
        $$('[data-kind-show]', form).forEach(function (el) { el.hidden = el.dataset.kindShow !== sel.value; });
        $$('[data-kind-hide]', form).forEach(function (el) { el.hidden = el.dataset.kindHide === sel.value; });
      };
      sel.addEventListener('change', sync); sync();
    });
  }
  // Курс перевода — в направлении, где число больше 1 («1 ฿ = 2,556 ₽»), и отклонение от курса
  // справочника (`rFrom`, `rTo` — курсы валют к базовой).
  function fmtRate(v) { return new Intl.NumberFormat(locale, { maximumFractionDigits: 4 }).format(v); }
  function rateText(sent, from, got, to, rFrom, rTo) {
    if (!(sent > 0) || !(got > 0)) return '';
    var up = sent >= got, x = up ? sent / got : got / sent, quote = up ? from : to;
    var s = (i18n.rate || '') + ' 1 ' + (up ? to : from) + ' = ' + fmtRate(x) + ' ' + quote;
    if (rFrom > 0 && rTo > 0) {
      var ref = up ? rTo / rFrom : rFrom / rTo, dev = (x / ref - 1) * 100;
      var pct = new Intl.NumberFormat(locale, { minimumFractionDigits: 1, maximumFractionDigits: 1 }).format(Math.abs(dev));
      s += ' · ' + (i18n.rateRef || '') + ' ' + fmtRate(ref) + ' ' + quote + ' (' + (dev >= 0 ? '+' : '−') + pct + ' %)';
    }
    return s;
  }
  // «Сверить остаток»: разница с остатком по операциям и категории нужного типа.
  function bindReconcile(scope) {
    $$('form[data-reconcile]', scope).forEach(function (form) {
      if (form.dataset.bound) return;
      form.dataset.bound = '1';
      var book = parseFloat(form.dataset.book), input = $('[data-actual]', form), out = $('[data-diff]', form), cat = $('select[data-kind-filtered]', form);
      var sync = function () {
        var v = evalAmount(input.value);
        if (isNaN(v)) { out.textContent = ''; return; }
        var d = Math.round((v - book) * 10000) / 10000;
        out.textContent = d === 0 ? out.dataset.same : (d > 0 ? out.dataset.more : out.dataset.less) + ' ' + money(Math.abs(d)) + (out.dataset.cur ? '\u00a0' + out.dataset.cur : '');
        if (cat && d !== 0) filterOptions(cat, d > 0 ? 'INCOME' : 'EXPENSE');
      };
      input.addEventListener('input', sync); sync();
    });
  }
  // Флажок-группа ([data-check-all] — селектор флажков группы, «Все справочники»): отмечает всю группу и сам
  // отмечен, только когда отмечены все. Без JS сервер понимает его как «все права группы».
  function bindCheckAll(scope) {
    $$('input[data-check-all]', scope).forEach(function (all) {
      if (all.dataset.bound) return;
      all.dataset.bound = '1';
      var items = $$(all.dataset.checkAll, all.form);
      var sync = function () {
        var n = items.filter(function (i) { return i.checked; }).length;
        all.checked = n === items.length;
        all.indeterminate = n > 0 && n < items.length;
      };
      all.addEventListener('change', function () { items.forEach(function (i) { i.checked = all.checked; }); all.indeterminate = false; });
      items.forEach(function (i) { i.addEventListener('change', sync); });
      sync();
    });
  }
  // Копирование (API-токен).
  doc.addEventListener('click', function (e) {
    var b = e.target.closest && e.target.closest('[data-copy]');
    if (!b) return;
    var src = $(b.dataset.copy);
    if (src && navigator.clipboard) navigator.clipboard.writeText(src.textContent.trim()).then(function () { b.textContent = b.dataset.copied || 'OK'; });
  });
  // «Поделиться» приглашением: системное меню телефона (Telegram, WhatsApp…), без него — копирование ссылки.
  doc.addEventListener('click', function (e) {
    var b = e.target.closest && e.target.closest('[data-share]');
    if (!b) return;
    var url = b.dataset.share;
    if (navigator.share) { navigator.share({ text: b.dataset.shareText || '', url: url }).catch(function () {}); return; }
    if (navigator.clipboard) navigator.clipboard.writeText(url).then(function () { b.textContent = b.dataset.copied || 'OK'; });
  });
  // Ошибка поля — сразу, при уходе из поля (HTML5-валидация), введённое не теряется.
  doc.addEventListener('focusout', function (e) {
    var i = e.target;
    if (i.matches && i.matches('form[data-form] .input[required]') && i.value) i.setAttribute('aria-invalid', String(!i.checkValidity()));
  });

  /* ---------------------------------------------------------------- полная форма операции (позиции) */
  var limitsCache = null;
  function loadLimits() {
    if (!limitsCache) limitsCache = fetch('/api/transactions/quick-defaults', { headers: { Accept: 'application/json' }, credentials: 'same-origin' })
      .then(function (r) { if (!r.ok) throw new Error(r.status); return r.json(); });
    return limitsCache;
  }
  function bindTxForms(scope) {
  $$('form[data-tx-form]', scope).forEach(function (form) {
    if (form.dataset.bound) return;
    form.dataset.bound = '1';
    var lines = $('[data-lines]', form), totalEl = $('[data-total]', form), curEl = $('span[data-cur]', form), note = $('[data-limit-note]', form);
    var kind = function () { var r = $('[name=tx_kind]:checked', form); return r ? r.value : 'EXPENSE'; };
    var flows = $$('[data-section=flow]', form), transfer = $('[data-section=transfer]', form), accLabel = $('label[for=account_code]', form);
    var refund = $('[data-refund]', form);
    function opt(sel) { var o = sel && sel.selectedOptions[0]; return o ? { cur: o.dataset.cur || '', sym: o.dataset.sym || o.dataset.cur || '', rate: parseFloat(o.dataset.rate) } : { cur: '', sym: '', rate: NaN }; }
    // Перевод: валюты счетов, «Зачислено» — только при разных валютах, курс из сумм (без комиссии).
    function syncTransfer() {
      if (!transfer) return;
      var f = opt($('[name=account_code]', form)), t = opt($('[name=to_account_code]', form));
      $$('[data-from-cur]', form).forEach(function (el) { el.textContent = f.cur; });
      $$('[data-to-cur]', form).forEach(function (el) { el.textContent = t.cur; });
      var exch = $('[data-exchange]', form), differ = !!(t.cur && f.cur && t.cur !== f.cur);
      if (exch) { exch.hidden = !differ; $('input', exch).disabled = !differ; }
      var line = $('[data-rate-line]', form);
      if (line) {
        var fee = evalAmount($('[data-fee]', form).value), sent = evalAmount($('[data-sent]', form).value) - (isNaN(fee) ? 0 : fee);
        line.textContent = differ ? rateText(sent, f.sym, evalAmount($('[data-received]', form).value), t.sym, f.rate, t.rate) : '';
      }
    }
    function syncKind() {
      var t = kind() === 'TRANSFER';
      flows.forEach(function (el) { el.hidden = t; if (el.tagName === 'FIELDSET') el.disabled = t; });
      if (transfer) { transfer.hidden = !t; transfer.disabled = !t; }
      if (accLabel && accLabel.dataset.labelFlow) accLabel.textContent = t ? accLabel.dataset.labelTransfer : accLabel.dataset.labelFlow;
      syncTransfer();
    }
    function filterCats() { $$('select[data-kind-filtered]', form).forEach(function (s) { filterOptions(s, kind()); }); }
    // Позиция с ценой: сумма = round(количество × цена) − скидка, поле суммы только для чтения
    // (docs/group-1.md, 3.2); итог сервер пересчитает при сохранении по тем же правилам.
    function minor() { var o = ($('[name=account_code]', form) || {}).selectedOptions; var m = o && o[0] ? parseInt(o[0].dataset.minor, 10) : NaN; return isNaN(m) ? 2 : m; }
    function itemAmount(row) {
      var amount = $('[data-amount]', row), price = evalAmount(($('[data-price]', row) || {}).value);
      if (isNaN(price)) { amount.readOnly = false; return evalAmount(amount.value); }
      var q = evalAmount(($('[data-qty]', row) || {}).value), d = evalAmount(($('[data-discount]', row) || {}).value), k = Math.pow(10, minor());
      // Половина — вверх, как на сервере; округление до 1e-8 убирает погрешность float (0,335 × 15 = 5,03).
      var raw = Math.round((isNaN(q) ? 1 : q) * price * 1e8) / 1e8;
      var v = Math.round(raw * k + 1e-7) / k - (isNaN(d) ? 0 : d);
      amount.readOnly = true;
      amount.value = v > 0 ? String(Math.round(v * k) / k) : '';
      return v;
    }
    function total() {
      var s = 0, acc = $('[name=account_code]', form), sym = acc && acc.selectedOptions[0] ? opt(acc).sym : '';
      $$('[data-line]', form).forEach(function (row) {
        if (row.hidden) return;
        var v = itemAmount(row), sum = $('[data-item-sum]', row);
        if (sum) sum.textContent = isNaN(v) ? '' : money(v);
        if (!isNaN(v)) s += v;
      });
      totalEl.textContent = money(s);
      $$('span[data-cur]', form).forEach(function (el) { el.textContent = sym; });
      return s;
    }
    function warn() {
      var k = kind();
      if (!note || (k !== 'EXPENSE' && k !== 'INCOME')) { if (note) note.textContent = ''; return; }
      loadLimits().then(function (d) {
        var acc = (d.accounts || []).filter(function (a) { return a.code === ($('[name=account_code]', form) || {}).value; })[0];
        if (acc && acc.rate_to_base == null) { showNoRate(note, acc); return; }
        if (k !== 'EXPENSE' || (refund && refund.checked)) { note.className = 'limit-note'; note.textContent = ''; return; }
        var rate = acc ? parseFloat(acc.rate_to_base) : 1;
        var per = {};
        $$('[data-line]', form).forEach(function (row) {
          var c = $('select', row).value, v = evalAmount($('[data-amount]', row).value);
          if (c && !isNaN(v)) per[c] = (per[c] || 0) + v * rate;
        });
        var worst = null;
        Object.keys(per).forEach(function (code) { var w = limitWarning(d, code, per[code]); if (w && (!worst || w.rank > worst.rank)) worst = w; });
        showLimit(note, worst, baseSym || d.base_currency);
      }).catch(function () { note.textContent = ''; });
    }
    // Пустые строки в запасе нужны только без JS: прячем их до «Добавить позицию».
    var visible = function () { return $$('[data-line]', lines).filter(function (r) { return !r.hidden; }); };
    $$('.item-spare', lines).forEach(function (r) { r.hidden = true; });
    if (!visible().length && $('[data-line]', lines)) $('[data-line]', lines).hidden = false;
    function clearRow(row) {
      $$('input', row).forEach(function (i) { if (i.type === 'checkbox') i.checked = false; else i.value = ''; i.readOnly = false; i.removeAttribute('autofocus'); i.removeAttribute('aria-invalid'); });
      $$('select', row).forEach(function (sel) { sel.selectedIndex = 0; });
      $$('.field-error', row).forEach(function (el) { el.remove(); });
    }
    lines.addEventListener('click', function (e) {
      var rm = e.target.closest('[data-remove-line]');
      if (!rm) return;
      var row = rm.closest('[data-line]');
      if (visible().length > 1) row.remove(); else clearRow(row);
      total(); warn();
    });
    var add = $('[data-add-line]', form);
    if (add) {
      add.hidden = false;
      add.addEventListener('click', function () {
        var row = $$('[data-line]', lines).filter(function (r) { return r.hidden; })[0];
        if (!row) {
          var rows = $$('[data-line]', lines), n = rows.length + 1;
          row = rows[rows.length - 1].cloneNode(true);
          clearRow(row);
          $$('[id]', row).forEach(function (el) { var id = el.id.replace(/-\d+$/, '-' + n); var lab = $('label[for="' + el.id + '"]', row); el.id = id; if (lab) lab.htmlFor = id; });
          $$('[aria-label]', row).forEach(function (el) { el.setAttribute('aria-label', el.getAttribute('aria-label').replace(/\d+$/, n)); });
          lines.appendChild(row);
        }
        row.hidden = false; filterCats(); total(); $('select', row).focus();
      });
    }
    form.addEventListener('input', function () { total(); syncTransfer(); });
    form.addEventListener('change', function (e) {
      if (e.target.name === 'tx_kind') syncKind();
      filterCats(); total(); warn(); syncTransfer();
    });
    // «120+45» в поле суммы → 165 при выходе из поля (и в добавленных строках позиций).
    form.addEventListener('focusout', function (e) {
      var i = e.target;
      if (!i.matches || !i.matches('[data-amount], [data-sent], [data-received], [data-fee], [data-price], [data-qty], [data-discount], [data-discount-total]') || i.readOnly) return;
      var v = evalAmount(i.value);
      if (!isNaN(v) && /[+*/]|\d-/.test(i.value)) { i.value = String(v); total(); syncTransfer(); }
    });
    filterCats(); total(); syncKind(); warn();
  });
  }

  /* ---------------------------------------------------------------- предупреждение о лимите (раздел 5) */
  // Сумма в базовой валюте `amount` в категории `code`: худший уровень по узлу и предкам.
  function limitWarning(d, code, amount) {
    var byCode = {}; (d.categories || []).forEach(function (c) { byCode[c.code] = c; });
    var node = byCode[code], worst = null, available = null;
    if (node && node.available != null) available = parseFloat(node.available) - amount;
    while (node) {
      if (node.limit != null) {
        var lim = parseFloat(node.limit), fc = parseFloat(node.forecast) + amount, pct = lim > 0 ? fc / lim * 100 : 0;
        var rank = pct > 100 ? 2 : pct >= 80 ? 1 : 0;
        if (!worst || rank > worst.rank || (rank === worst.rank && pct > worst.pct)) worst = { rank: rank, pct: pct, title: node.title };
      }
      node = node.parent_code ? byCode[node.parent_code] : null;
    }
    if (available == null) return worst ? { rank: worst.rank, pct: worst.pct, title: worst.title, left: null } : null;
    return { rank: worst ? worst.rank : (available < 0 ? 2 : 0), pct: worst ? worst.pct : 0, title: worst ? worst.title : '', left: available };
  }
  // Нет курса валюты счёта к базовой: лимит не посчитать, а сумма не войдёт в бюджет, пока курс не добавят.
  function showNoRate(el, acc) {
    el.className = 'limit-note warn';
    el.textContent = (i18n.noRate || '').replace('{cur}', acc.currency);
  }
  function showLimit(el, w, base) {
    el.className = 'limit-note';
    if (!w) { el.textContent = ''; return; }
    var parts = [];
    if (w.left != null) parts.push(w.left < 0 ? (i18n.over || '') + ': ' + money(-w.left) + ' ' + base : (i18n.left || '') + ': ' + money(w.left) + ' ' + base);
    if (w.rank === 1) { el.classList.add('warn'); parts.push((i18n.warn || '') + (w.title ? ' — ' + w.title : '') + ' (' + Math.round(w.pct) + ' %)'); }
    if (w.rank === 2) { el.classList.add('over'); parts.push((w.title ? w.title + ': ' : '') + Math.round(w.pct) + ' %. ' + (i18n.anyway || '')); }
    el.textContent = parts.join(' · ');
  }

  /* ---------------------------------------------------------------- дерево бюджета */
  doc.addEventListener('click', function (e) {
    var t = e.target.closest && e.target.closest('table[data-tree] [data-toggle]');
    if (!t) return;
    var table = t.closest('table'), rows = $$('tbody tr', table);
    var kids = function (code) { return rows.filter(function (r) { return r.dataset.parent === code; }); };
    function hideDeep(code) {
      kids(code).forEach(function (k) { k.hidden = true; var x = $('[data-toggle]', k); if (x) x.setAttribute('aria-expanded', 'false'); hideDeep(k.dataset.code); });
    }
    var row = t.closest('tr'), open = t.getAttribute('aria-expanded') === 'true';
    if (open) hideDeep(row.dataset.code); else kids(row.dataset.code).forEach(function (k) { k.hidden = false; });
    t.setAttribute('aria-expanded', String(!open));
  });
  doc.addEventListener('click', function (e) {
    var b = e.target.closest && e.target.closest('[data-tree-expand]');
    if (!b) return;
    var table = $(b.dataset.treeExpand), expand = b.getAttribute('aria-pressed') !== 'true';
    $$('tbody tr', table).forEach(function (r) {
      var depth = parseInt(r.dataset.depth || '0', 10);
      r.hidden = !expand && depth >= 2;
      var t = $('[data-toggle]', r); if (t) t.setAttribute('aria-expanded', String(expand || depth < 1));
    });
    b.setAttribute('aria-pressed', String(expand));
    b.textContent = expand ? b.dataset.labelCollapse : b.dataset.labelExpand;
  });

  // Свёрнутость дерева переживает подмену страницы: видимые строки, раскрытые узлы и «Развернуть всё».
  // Строки, которых раньше не было (новая категория), остаются как отдал сервер.
  function treeState() {
    var table = $('table[data-tree]');
    if (!table) return null;
    var s = { shown: {}, open: {}, expand: null };
    $$('tbody tr', table).forEach(function (r) {
      s.shown[r.dataset.code] = !r.hidden;
      var t = $('[data-toggle]', r); if (t) s.open[r.dataset.code] = t.getAttribute('aria-expanded') === 'true';
    });
    var b = $('[data-tree-expand]'); if (b) s.expand = b.getAttribute('aria-pressed') === 'true';
    return s;
  }
  function restoreTree(s) {
    var table = $('table[data-tree]');
    if (!s || !table) return;
    $$('tbody tr', table).forEach(function (r) {
      var code = r.dataset.code;
      if (code in s.shown) r.hidden = !s.shown[code];
      var t = $('[data-toggle]', r); if (t && code in s.open) t.setAttribute('aria-expanded', String(s.open[code]));
    });
    var b = $('[data-tree-expand]');
    if (b && s.expand !== null) { b.setAttribute('aria-pressed', String(s.expand)); b.textContent = s.expand ? b.dataset.labelCollapse : b.dataset.labelExpand; }
  }

  /* ---------------------------------------------------------------- обновление цифр страницы без перезагрузки */
  function swapMain(url, after) {
    var main = $('#main');
    if (!main) return;
    var tree = treeState();
    var done = function () { var m = $('#main'); restoreTree(tree); bindPage(m); if (after) after(m); };
    if (window.htmx) {
      window.htmx.ajax('GET', url, { target: '#main', select: '#main', swap: 'outerHTML' }).then(done);
    } else {
      fetch(url, { credentials: 'same-origin' }).then(function (r) { return r.text(); }).then(function (html) {
        var next = new DOMParser().parseFromString(html, 'text/html').querySelector('#main');
        if (next) { main.replaceWith(next); done(); }
      });
    }
  }
  function refreshPage() { swapMain(location.pathname + location.search); }
  // Сортировка «Расходов»: без JS — обычная ссылка, с JS — подмена страницы без потери свёрнутости.
  doc.addEventListener('click', function (e) {
    var a = e.target.closest && e.target.closest('table[data-tree] a[data-sort]');
    if (!a || e.metaKey || e.ctrlKey || e.shiftKey || e.button) return;
    e.preventDefault();
    var href = a.getAttribute('href'), key = a.dataset.sort;
    swapMain(href, function (m) {
      history.replaceState(null, '', href);
      var f = $('table[data-tree] a[data-sort="' + key + '"]', m); if (f) f.focus();
    });
  });
  // Курс валюты (справочник «Курсы»): направление ввода выбирает пользователь кнопкой ⇄ — «1 RUB = x THB» или
  // «1 THB = x RUB» (скрытое поле inverted). Новая валюта открывается в направлении её прошлого курса.
  // Подсказка «последний: …», предпросмотр пересчёта и предупреждение, если курс отличается от прошлого больше
  // чем в 5 раз. Курс к базовой в data-rate у <option> — базовой за 1 единицу валюты.
  function fmtRateSig(v) { return new Intl.NumberFormat(locale, v >= 1 ? { maximumFractionDigits: 4 } : { maximumSignificantDigits: 4 }).format(v); }
  function bindRateForms(scope) {
    $$('form[data-rate-form]', scope).forEach(function (form) {
      if (form.dataset.bound) return;
      form.dataset.bound = '1';
      var t = form.dataset, base = t.baseSym || t.base, sel = $('select[name=currency_code]', form), input = $('input[name=rate]', form);
      var inv = $('[data-rate-inverted]', form), last = $('[data-rate-last]', form), preview = $('[data-rate-preview]', form);
      var warn = $('[data-rate-warn]', form), warnText = $('[data-rate-warn-text]', form);
      var plain = function (v) { return String(Math.round(v * 1e6) / 1e6); };
      var isInv = function () { return inv.value === 'true'; };
      // Символ выбранной валюты для подписей (₽), значение списка — код.
      var cur = function () { var o = sel.selectedOptions[0]; return o ? o.dataset.sym || o.value : ''; };
      function prev() {
        var o = sel.selectedOptions[0], r = o ? parseFloat(o.dataset.rate) : NaN;
        return r > 0 ? { rate: r, entered: parseFloat(o.dataset.entered), inverted: o.dataset.inverted === 'true', date: o.dataset.date } : null;
      }
      // Прошлый курс в выбранном направлении: введённое число как есть, иначе пересчитанное.
      function prevValue(p) { return p.inverted === isInv() ? p.entered : isInv() ? 1 / p.rate : p.rate; }
      function sync() {
        var c = cur(), i = isInv(), p = prev(), from = i ? base : c, to = i ? c : base;
        $$('[data-rate-from]', form).forEach(function (el) { el.textContent = from; });
        $$('[data-rate-to]', form).forEach(function (el) { el.textContent = to; });
        var dir = $('[data-rate-dir]', form); if (dir) dir.textContent = '1 ' + from + ' = … ' + to;
        var pv = p ? prevValue(p) : NaN, pText = p ? fmtRateSig(pv) + ' · ' + p.date : '';
        input.placeholder = p ? plain(pv) : '';
        last.textContent = p ? t.tLast + ': ' + pText : '';
        last.hidden = !p;
        var v = evalAmount(input.value);
        if (!(v > 0) || !c) { preview.textContent = ''; warn.hidden = true; return; }
        var rate = i ? 1 / v : v;  // базовой за 1 единицу валюты
        var n = Math.max(100, Math.pow(10, Math.ceil(Math.log10(100 / rate))));
        var whole = new Intl.NumberFormat(locale, { maximumFractionDigits: 2 });
        preview.textContent = '= ' + (i ? fmtRateSig(rate) + ' ' + base + ' ' + t.tPer + ' 1 ' + c : fmtRateSig(1 / rate) + ' ' + c + ' ' + t.tPer + ' 1 ' + base) +
          ' · ' + whole.format(n) + ' ' + c + ' ≈ ' + whole.format(n * rate) + ' ' + base;
        var k = p ? v / pv : 1;
        if (k > 5 || k < 0.2) {
          warnText.textContent = (k < 1 ? t.tLess : t.tMore).replace('{k}', Math.round(k < 1 ? 1 / k : k)) + ' (' + pText + '). ' + t.tCheck;
          warn.hidden = false;
        } else warn.hidden = true;
      }
      function setInv(v) { inv.value = v ? 'true' : 'false'; }
      form.addEventListener('click', function (e) {
        var b = e.target.closest('[data-rate-swap]');
        if (!b) return;
        var v = evalAmount(input.value);
        setInv(!isInv());
        // ⇄ — тот же курс в другую сторону (число пересчитывается; обратно — снова введённое),
        // «Поменять направление» из предупреждения — то же число, но в другую сторону.
        if (b.dataset.rateSwap === 'convert' && v > 0) {
          var back = input.dataset.swapBack, typed = input.value;
          input.value = back && input.dataset.swapped === typed ? back : plain(1 / v);
          input.dataset.swapBack = typed; input.dataset.swapped = input.value;
        }
        sync();
        input.focus();
      });
      sel.addEventListener('change', function () {
        var p = prev();
        setInv(p ? p.inverted : false);
        delete input.dataset.swapBack;
        sync();
      });
      input.addEventListener('input', sync);
      sync();
    });
  }
  function bindPage(scope) { bindFilters(scope || doc); bindKindToggles(scope || doc); bindTxForms(scope || doc); bindReconcile(scope || doc); bindCheckAll(scope || doc); bindRateForms(scope || doc); }
  bindPage(doc);

  /* ---------------------------------------------------------------- быстрый расход (раздел 10) */
  var dlg = $('#quick');
  var Q = { data: null, kind: 'EXPENSE', category: '', loading: null };

  function api(method, url, payload) {
    var h = { Accept: 'application/json', 'X-CSRF-Token': csrf };
    if (payload) h['Content-Type'] = 'application/json';
    return fetch(url, { method: method, headers: h, credentials: 'same-origin', body: payload ? JSON.stringify(payload) : undefined })
      .then(function (r) {
        if (r.status === 204) return null;
        return r.json().catch(function () { return {}; }).then(function (j) { if (!r.ok) { var e = new Error(j.message || r.status); e.body = j; throw e; } return j; });
      });
  }
  function todayISO(offset) {
    var d = new Date(); d.setDate(d.getDate() - (offset || 0));
    return d.getFullYear() + '-' + String(d.getMonth() + 1).padStart(2, '0') + '-' + String(d.getDate()).padStart(2, '0');
  }
  function qState(state) {
    $('#q-body').hidden = state !== 'ready';
    $('#q-loading').hidden = state !== 'loading';
    $('#q-failed').hidden = state !== 'failed';
    $$('.dialog-foot button, .dialog-foot a', dlg).forEach(function (b) { b.toggleAttribute('disabled', state !== 'ready'); });
  }
  function catByCode(code) { return (Q.data.categories || []).filter(function (c) { return c.code === code; })[0]; }
  // До загрузки данных (первое нажатие в Safari: префетча через requestIdleCallback там нет) счёта нет.
  function account() { return ((Q.data && Q.data.accounts) || []).filter(function (a) { return a.code === $('#q-account').value; })[0]; }
  function setKind(kind) {
    Q.kind = kind;
    var t = kind === 'TRANSFER';
    $$('input[name=tx_kind]', dlg).forEach(function (r) { r.checked = r.value === kind; });
    $('#quick-title').textContent = t ? dlg.dataset.titleTransfer : kind === 'INCOME' ? dlg.dataset.titleIncome : dlg.dataset.titleExpense;
    // Перевод: вместо категории — счёт зачисления; «Счёт» становится «Откуда».
    $('[data-q-flow]', dlg).hidden = t;
    $('[data-q-transfer]', dlg).hidden = !t;
    $$('[data-label-flow]', dlg).forEach(function (el) { var x = $('span', el) || el; x.textContent = t ? el.dataset.labelTransfer : el.dataset.labelFlow; });
    var c = Q.category && catByCode(Q.category);
    if (c && c.kind !== kind) Q.category = '';
    renderChips(); updateLimit(); if (t) fillTo();
  }
  // Счета зачисления — все, кроме счёта списания; по умолчанию — как в прошлом переводе.
  function fillTo() {
    if (!Q.data) return;
    var sel = $('#q-to'), from = $('#q-account').value, keep = sel.value, last = Q.data.last_transfer;
    sel.textContent = '';
    (Q.data.accounts || []).forEach(function (a) {
      if (a.code === from) return;
      var o = doc.createElement('option'); o.value = a.code; o.textContent = a.title + ' · ' + a.currency; sel.appendChild(o);
    });
    var pick = [keep, last && last[0] === from ? last[1] : ''].filter(function (c) { return c && c !== from && $('option[value="' + c + '"]', sel); })[0];
    if (pick) sel.value = pick;
    $('#q-to-err').hidden = true;
    updateTransfer();
  }
  function accountBy(code) { return ((Q.data && Q.data.accounts) || []).filter(function (a) { return a.code === code; })[0]; }
  function updateTransfer() {
    if (!Q.data || Q.kind !== 'TRANSFER') return;
    var f = account(), t = accountBy($('#q-to').value), differ = !!(f && t && f.currency !== t.currency);
    $('#q-exchange').hidden = !differ;
    $('#q-to-cur').textContent = t ? t.currency : '';
    $('#q-rate').textContent = differ ? rateText(evalAmount($('#q-amount').value), f.symbol, evalAmount($('#q-to-amount').value), t.symbol, parseFloat(f.rate_to_base), parseFloat(t.rate_to_base)) : '';
  }
  function pickCategory(code) {
    Q.category = code;
    renderChips();
    $('#q-cat-err').hidden = true;
    updateLimit();
  }
  function renderChips() {
    var box = $('#q-chips'); box.textContent = '';
    if (!Q.data) return;
    var list = (Q.kind === 'INCOME' ? Q.data.frequent_income : Q.data.frequent_expense).slice();
    if (Q.category && list.indexOf(Q.category) < 0) list.unshift(Q.category);
    list.slice(0, 7).forEach(function (code) {
      var c = catByCode(code); if (!c) return;
      var b = doc.createElement('button');
      b.type = 'button'; b.className = 'chip'; b.textContent = c.title; b.dataset.code = code;
      b.setAttribute('aria-pressed', String(code === Q.category));
      b.addEventListener('click', function () { pickCategory(code); $('#q-amount').focus(); });
      box.appendChild(b);
    });
  }
  // Поиск по дереву на любом из трёх языков; стрелки, Enter — выбор.
  var search = $('#q-search'), results = $('#q-results'), active = -1;
  function pathOf(c) {
    var names = [], p = c.parent_code ? catByCode(c.parent_code) : null;
    while (p) { names.unshift(p.title); p = p.parent_code ? catByCode(p.parent_code) : null; }
    return names.join(' › ');
  }
  function renderResults() {
    var q = search.value.trim().toLowerCase();
    results.textContent = ''; active = -1;
    if (!q || !Q.data) { results.hidden = true; search.setAttribute('aria-expanded', 'false'); return; }
    var found = Q.data.categories.filter(function (c) { return c.kind === Q.kind && c.search.toLowerCase().indexOf(q) >= 0; }).slice(0, 12);
    if (!found.length) {
      var li = doc.createElement('li'); li.className = 'opt-path'; li.textContent = i18n.noMatch || ''; li.setAttribute('role', 'presentation'); results.appendChild(li);
    }
    found.forEach(function (c, i) {
      var li = doc.createElement('li');
      li.id = 'q-opt-' + i; li.setAttribute('role', 'option'); li.dataset.code = c.code;
      var t = doc.createElement('span'); t.textContent = c.title; li.appendChild(t);
      var p = pathOf(c); if (p) { var s = doc.createElement('span'); s.className = 'opt-path'; s.textContent = p; li.appendChild(s); }
      li.addEventListener('mousedown', function (e) { e.preventDefault(); choose(c.code); });
      results.appendChild(li);
    });
    results.hidden = false; search.setAttribute('aria-expanded', 'true');
  }
  function choose(code) { pickCategory(code); search.value = ''; results.hidden = true; search.setAttribute('aria-expanded', 'false'); $('#q-amount').focus(); }
  function moveActive(d) {
    var opts = $$('[role=option]', results); if (!opts.length) return;
    active = (active + d + opts.length) % opts.length;
    opts.forEach(function (o, i) { o.setAttribute('aria-selected', String(i === active)); });
    search.setAttribute('aria-activedescendant', opts[active].id);
    opts[active].scrollIntoView({ block: 'nearest' });
  }
  if (search) {
    search.addEventListener('input', renderResults);
    search.addEventListener('keydown', function (e) {
      if (e.key === 'ArrowDown') { e.preventDefault(); moveActive(1); }
      else if (e.key === 'ArrowUp') { e.preventDefault(); moveActive(-1); }
      else if (e.key === 'Enter' && !results.hidden) {
        e.preventDefault();
        var opts = $$('[role=option]', results), o = opts[active >= 0 ? active : 0];
        if (o) choose(o.dataset.code);
      } else if (e.key === 'Escape' && !results.hidden) { e.preventDefault(); e.stopPropagation(); search.value = ''; renderResults(); }
    });
    search.addEventListener('blur', function () { setTimeout(function () { results.hidden = true; }, 120); });
  }
  function updateCurrency() { var a = account(); $('#q-cur').textContent = a ? a.symbol : ''; }
  function updateLimit() {
    var el = $('#q-limit'), a = account();
    if (Q.data && a && a.rate_to_base == null && Q.kind !== 'TRANSFER') { showNoRate(el, a); return; }
    if (!Q.data || Q.kind !== 'EXPENSE' || !Q.category) { el.textContent = ''; el.className = 'limit-note'; return; }
    var v = evalAmount($('#q-amount').value);
    var amount = (isNaN(v) ? 0 : v) * (a ? parseFloat(a.rate_to_base) : 1);
    var w = limitWarning(Q.data, Q.category, amount);
    if (w && w.left != null && isNaN(v) && w.rank === 0) { el.className = 'limit-note'; el.textContent = (i18n.available || '') + ': ' + money(w.left) + ' ' + (baseSym || Q.data.base_currency); return; }
    showLimit(el, w, baseSym || Q.data.base_currency);
  }
  function fillAccounts() {
    var sel = $('#q-account'), keep = sel.value; sel.textContent = '';
    (Q.data.accounts || []).forEach(function (a) {
      var o = doc.createElement('option'); o.value = a.code; o.textContent = a.title + ' · ' + a.currency; sel.appendChild(o);
    });
    sel.value = keep && account() ? keep : (Q.data.default_account || '');
    updateCurrency();
  }
  function load() {
    qState('loading');
    Q.loading = api('GET', '/api/transactions/quick-defaults?date=' + todayISO(0)).then(function (d) {
      Q.data = d; fillAccounts(); renderChips(); qState('ready');
      if (!d.accounts.length) { showError(i18n.noAccounts); }
      return d;
    }).catch(function () { Q.loading = null; qState('failed'); });
    return Q.loading;
  }
  function showError(msg) { var e = $('#q-error'); e.textContent = msg || ''; e.hidden = !msg; }
  function resetForm(keepAccountDate) {
    $('#q-amount').value = ''; $('#q-desc').value = ''; Q.category = ''; $('#q-to-amount').value = ''; $('#q-to-amount-err').hidden = true;
    $('#q-amount').removeAttribute('aria-invalid'); $('#q-amount-err').hidden = true; $('#q-cat-err').hidden = true; showError('');
    if (!keepAccountDate) {
      $$('input[name=q_day]', dlg).forEach(function (r) { r.checked = r.value === '0'; });
      $('#q-date').hidden = true; $('#q-date').value = todayISO(0);
    }
    renderChips(); updateLimit();
  }
  function openQuick(kind, category, accountCode) {
    if (!dlg) return false;
    resetForm(false);
    if (!dlg.open) dlg.showModal();
    var after = function () {
      if (accountCode && accountBy(accountCode)) { $('#q-account').value = accountCode; updateCurrency(); }
      setKind(kind || 'EXPENSE'); if (category) pickCategory(category); $('#q-amount').focus();
    };
    if (Q.data) { qState('ready'); after(); } else (Q.loading || load()).then(after);
    return true;
  }
  function selectedDate() {
    var r = $('input[name=q_day]:checked', dlg), v = r ? r.value : '0';
    return v === 'other' ? ($('#q-date').value || todayISO(0)) : todayISO(parseInt(v, 10));
  }
  // Перевод: POST /api/transactions с типом TRANSFER — сервер строит проводки.
  function saveTransfer(more) {
    var amount = evalAmount($('#q-amount').value), ok = true, f = account(), t = accountBy($('#q-to').value);
    var differ = !!(f && t && f.currency !== t.currency), got = evalAmount($('#q-to-amount').value);
    if (isNaN(amount) || amount <= 0) {
      $('#q-amount').setAttribute('aria-invalid', 'true');
      var ae = $('#q-amount-err'); ae.textContent = i18n.amountRequired || ''; ae.hidden = false; ok = false;
    }
    if (!t) { var te = $('#q-to-err'); te.textContent = i18n.chooseTo || ''; te.hidden = false; ok = false; }
    if (differ && (isNaN(got) || got <= 0)) { var ge = $('#q-to-amount-err'); ge.textContent = i18n.receivedRequired || ''; ge.hidden = false; ok = false; }
    if (!ok) return;
    var desc = $('#q-desc').value.trim();
    var payload = {
      occurred_on: selectedDate(), tx_kind: 'TRANSFER', account_code: f.code, to_account_code: t.code, amount: String(amount),
      to_amount: differ ? String(got) : null, description: desc || null, description_lang: lang.toUpperCase()
    };
    showError('');
    $$('.dialog-foot button', dlg).forEach(function (b) { b.disabled = true; });
    api('POST', '/api/transactions', payload).then(function (tx) {
      var label = (i18n.transfer || '') + ' ' + money(amount) + ' ' + f.symbol + ' → ' + t.title;
      toast(label, { undo: function () { api('DELETE', '/api/transactions/' + tx.id).then(function () { toast(i18n.undone || ''); refreshPage(); }); } });
      Q.data.last_transfer = [f.code, t.code];
      if (more) { resetForm(true); $('#q-amount').focus(); } else dlg.close();
      refreshPage();
    }).catch(function (e) {
      showError((e.body && e.body.message) || i18n.failed || '');
    }).then(function () { $$('.dialog-foot button', dlg).forEach(function (b) { b.disabled = false; }); });
  }
  function save(more) {
    if (Q.kind === 'TRANSFER') return saveTransfer(more);
    var amount = evalAmount($('#q-amount').value), ok = true;
    if (isNaN(amount) || amount <= 0) {
      $('#q-amount').setAttribute('aria-invalid', 'true');
      var ae = $('#q-amount-err'); ae.textContent = i18n.amountRequired || ''; ae.hidden = false; ok = false;
    }
    if (!Q.category) { var ce = $('#q-cat-err'); ce.textContent = i18n.choose || ''; ce.hidden = false; ok = false; }
    if (!ok) { (isNaN(amount) || amount <= 0 ? $('#q-amount') : search).focus(); return; }
    var acc = account(), cat = catByCode(Q.category), desc = $('#q-desc').value.trim();
    var payload = {
      occurred_on: selectedDate(), account_code: $('#q-account').value, tx_kind: Q.kind,
      description: desc || null, description_lang: lang.toUpperCase(),
      items: [{ category_code: Q.category, amount: String(amount) }]
    };
    showError('');
    $$('.dialog-foot button', dlg).forEach(function (b) { b.disabled = true; });
    api('POST', '/api/transactions', payload).then(function (t) {
      var label = (Q.kind === 'INCOME' ? i18n.income : i18n.expense) + ' ' + money(amount) + ' ' + (acc ? acc.symbol : '') + ' · ' + (cat ? cat.title : '');
      toast(label, { undo: function () { api('DELETE', '/api/transactions/' + t.id).then(function () { toast(i18n.undone || ''); refreshPage(); }); } });
      // Частые категории и счёт по умолчанию — свежие при следующем открытии.
      Q.data.default_account = payload.account_code;
      var freq = Q.kind === 'INCOME' ? Q.data.frequent_income : Q.data.frequent_expense;
      if (freq.indexOf(Q.category) < 0) { freq.unshift(Q.category); freq.length = Math.min(freq.length, 6); }
      if (cat && Q.kind === 'EXPENSE' && !(acc && acc.rate_to_base == null)) { cat.forecast = String(parseFloat(cat.forecast) + amount * (acc ? parseFloat(acc.rate_to_base) : 1)); }
      limitsCache = null;
      if (more) { resetForm(true); $('#q-amount').focus(); } else dlg.close();
      refreshPage();
    }).catch(function (e) {
      var msg = (e.body && e.body.message) || i18n.failed || '';
      showError(msg);
    }).then(function () { $$('.dialog-foot button', dlg).forEach(function (b) { b.disabled = false; }); });
  }
  if (dlg) {
    $('#quick-form').addEventListener('submit', function (e) { e.preventDefault(); save(e.submitter && e.submitter.hasAttribute('data-more')); });
    dlg.addEventListener('keydown', function (e) {
      if (e.key === 'Enter' && (e.ctrlKey || e.metaKey)) { e.preventDefault(); save(true); }
    });
    $$('input[name=tx_kind]', dlg).forEach(function (r) { r.addEventListener('change', function () { setKind(r.value); }); });
    $$('input[name=q_day]', dlg).forEach(function (r) {
      r.addEventListener('change', function () { var other = r.value === 'other'; $('#q-date').hidden = !other; if (other) $('#q-date').focus(); });
    });
    $('#q-amount').addEventListener('input', function () { $('#q-amount').removeAttribute('aria-invalid'); $('#q-amount-err').hidden = true; updateLimit(); updateTransfer(); });
    $('#q-to').addEventListener('change', function () { $('#q-to-err').hidden = true; updateTransfer(); });
    $('#q-to-amount').addEventListener('input', function () { $('#q-to-amount-err').hidden = true; updateTransfer(); });
    $('#q-amount').addEventListener('blur', function () { var i = $('#q-amount'), v = evalAmount(i.value); if (!isNaN(v) && /[+*/]|\d-/.test(i.value)) { i.value = String(v); updateLimit(); } });
    $('#q-account').addEventListener('change', function () { updateCurrency(); updateLimit(); if (Q.kind === 'TRANSFER') fillTo(); });
    $('[data-action="quick-retry"]', dlg).addEventListener('click', function () { load().then(function () { setKind(Q.kind); $('#q-amount').focus(); }); });
    $$('[data-close]', dlg).forEach(function (b) { b.addEventListener('click', function () { dlg.close(); }); });
    dlg.addEventListener('click', function (e) { if (e.target === dlg) dlg.close(); });
    // «Разбить на позиции»: полная форма, введённое — первая позиция.
    $('#q-split').addEventListener('click', function (e) {
      e.preventDefault();
      var v = evalAmount($('#q-amount').value), q = new URLSearchParams();
      q.set('kind', Q.kind); q.set('back', location.pathname + location.search);
      if (!isNaN(v)) q.set('amount', String(v));
      if (Q.kind === 'TRANSFER' && $('#q-to').value) q.set('to_account', $('#q-to').value);
      else if (Q.category) q.set('category', Q.category);
      if ($('#q-account').value) q.set('account', $('#q-account').value);
      q.set('date', selectedDate());
      if ($('#q-desc').value.trim()) { q.set('desc', $('#q-desc').value.trim()); q.set('desc_lang', lang); }
      location.href = '/transactions/new?' + q.toString();
    });
  }
  // Точки входа: «+ Расход», круглая «+», «+» в строке категории, «Перевести» у счёта, клавиши N, I и T.
  doc.addEventListener('click', function (e) {
    var t = e.target.closest && e.target.closest('[data-quick]');
    if (!t || !dlg || e.metaKey || e.ctrlKey || e.shiftKey) return;
    e.preventDefault();
    openQuick(t.dataset.quick, t.dataset.category, t.dataset.account);
  });

  /* ---------------------------------------------------------------- горячие клавиши */
  var keys = $('#keys');
  if (keys) $$('[data-close]', keys).forEach(function (b) { b.addEventListener('click', function () { keys.close(); }); });
  doc.addEventListener('keydown', function (e) {
    if (e.defaultPrevented || e.altKey || e.ctrlKey || e.metaKey || typing(e)) return;
    var open = doc.querySelector('dialog[open]');
    if (e.key === 'Escape') {
      var more = $('#more.open'); if (more) { more.classList.remove('open'); e.preventDefault(); }
      return;
    }
    if (open) return;
    if ((e.key === 'n' || e.key === 'N' || e.key === 'т' || e.key === 'Т') && canWrite) { e.preventDefault(); openQuick('EXPENSE'); }
    else if ((e.key === 'i' || e.key === 'I' || e.key === 'ш' || e.key === 'Ш') && canWrite) { e.preventDefault(); openQuick('INCOME'); }
    else if ((e.key === 't' || e.key === 'T' || e.key === 'е' || e.key === 'Е') && canWrite) { e.preventDefault(); openQuick('TRANSFER'); }
    else if (e.key === '?' && keys) { e.preventDefault(); keys.showModal(); }
  });

  // Префетч данных формы, пока пользователь думает: открытие диалога — мгновенное.
  // В Safari нет requestIdleCallback — там просто чуть позже загрузки страницы.
  var idle = window.requestIdleCallback || function (f) { setTimeout(f, 500); };
  if (dlg && canWrite) idle(function () { if (!Q.loading) load().then(function () { qState('ready'); }); });
})();
