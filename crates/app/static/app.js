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
  // Копирование (API-токен).
  doc.addEventListener('click', function (e) {
    var b = e.target.closest && e.target.closest('[data-copy]');
    if (!b) return;
    var src = $(b.dataset.copy);
    if (src && navigator.clipboard) navigator.clipboard.writeText(src.textContent.trim()).then(function () { b.textContent = b.dataset.copied || 'OK'; });
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
    var lines = $('[data-lines]', form), totalEl = $('[data-total]', form), curEl = $('[data-cur]', form), note = $('[data-limit-note]', form);
    var kind = function () { var r = $('[name=tx_kind]:checked', form); return r ? r.value : 'EXPENSE'; };
    function filterCats() { $$('select[data-kind-filtered]', form).forEach(function (s) { filterOptions(s, kind()); }); }
    function total() {
      var s = 0;
      $$('[data-amount]', form).forEach(function (i) { var v = evalAmount(i.value); if (!isNaN(v)) s += v; });
      totalEl.textContent = money(s);
      var acc = $('[name=account_code]', form);
      curEl.textContent = acc && acc.selectedOptions[0] ? acc.selectedOptions[0].dataset.cur || '' : '';
      return s;
    }
    function warn() {
      if (!note || kind() !== 'EXPENSE') { if (note) note.textContent = ''; return; }
      loadLimits().then(function (d) {
        var acc = (d.accounts || []).filter(function (a) { return a.code === ($('[name=account_code]', form) || {}).value; })[0];
        var rate = acc ? parseFloat(acc.rate_to_base) : 1;
        var per = {};
        $$('[data-line]', form).forEach(function (row) {
          var c = $('select', row).value, v = evalAmount($('[data-amount]', row).value);
          if (c && !isNaN(v)) per[c] = (per[c] || 0) + v * rate;
        });
        var worst = null;
        Object.keys(per).forEach(function (code) { var w = limitWarning(d, code, per[code]); if (w && (!worst || w.rank > worst.rank)) worst = w; });
        showLimit(note, worst, d.base_currency);
      }).catch(function () { note.textContent = ''; });
    }
    lines.addEventListener('click', function (e) {
      var rm = e.target.closest('[data-remove-line]');
      if (rm && $$('[data-line]', lines).length > 1) { rm.closest('[data-line]').remove(); total(); warn(); }
    });
    var add = $('[data-add-line]', form);
    if (add) add.addEventListener('click', function () {
      var rows = $$('[data-line]', lines), row = rows[rows.length - 1].cloneNode(true), n = rows.length + 1;
      $$('input', row).forEach(function (i) { i.value = ''; i.removeAttribute('autofocus'); i.removeAttribute('aria-invalid'); });
      $('select', row).value = '';
      $$('[id]', row).forEach(function (el) { var id = el.id.replace(/-\d+$/, '-' + n); var lab = $('label[for="' + el.id + '"]', row); el.id = id; if (lab) lab.htmlFor = id; });
      lines.appendChild(row); filterCats(); $('select', row).focus();
    });
    form.addEventListener('input', function () { total(); });
    form.addEventListener('change', function () { filterCats(); total(); warn(); });
    $$('[data-amount]', form).forEach(function (i) { i.addEventListener('blur', function () { var v = evalAmount(i.value); if (!isNaN(v) && /[+*/]|\d-/.test(i.value)) i.value = String(v); }); });
    filterCats(); total();
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

  /* ---------------------------------------------------------------- обновление цифр страницы без перезагрузки */
  function refreshPage() {
    var main = $('#main');
    if (!main) return;
    var url = location.pathname + location.search;
    if (window.htmx) {
      window.htmx.ajax('GET', url, { target: '#main', select: '#main', swap: 'outerHTML' }).then(function () { bindPage($('#main')); });
    } else {
      fetch(url, { credentials: 'same-origin' }).then(function (r) { return r.text(); }).then(function (html) {
        var next = new DOMParser().parseFromString(html, 'text/html').querySelector('#main');
        if (next) { main.replaceWith(next); bindPage(next); }
      });
    }
  }
  function bindPage(scope) { bindFilters(scope || doc); bindTxForms(scope || doc); }
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
  function account() { return (Q.data.accounts || []).filter(function (a) { return a.code === $('#q-account').value; })[0]; }
  function setKind(kind) {
    Q.kind = kind;
    $$('input[name=tx_kind]', dlg).forEach(function (r) { r.checked = r.value === kind; });
    $('#quick-title').textContent = kind === 'INCOME' ? dlg.dataset.titleIncome : dlg.dataset.titleExpense;
    var c = Q.category && catByCode(Q.category);
    if (c && c.kind !== kind) Q.category = '';
    renderChips(); updateLimit();
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
    var el = $('#q-limit');
    if (!Q.data || Q.kind !== 'EXPENSE' || !Q.category) { el.textContent = ''; el.className = 'limit-note'; return; }
    var v = evalAmount($('#q-amount').value), a = account();
    var amount = (isNaN(v) ? 0 : v) * (a ? parseFloat(a.rate_to_base) : 1);
    var w = limitWarning(Q.data, Q.category, amount);
    if (w && w.left != null && isNaN(v) && w.rank === 0) { el.className = 'limit-note'; el.textContent = (i18n.available || '') + ': ' + money(w.left) + ' ' + Q.data.base_currency; return; }
    showLimit(el, w, Q.data.base_currency);
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
    $('#q-amount').value = ''; $('#q-desc').value = ''; Q.category = '';
    $('#q-amount').removeAttribute('aria-invalid'); $('#q-amount-err').hidden = true; $('#q-cat-err').hidden = true; showError('');
    if (!keepAccountDate) {
      $$('input[name=q_day]', dlg).forEach(function (r) { r.checked = r.value === '0'; });
      $('#q-date').hidden = true; $('#q-date').value = todayISO(0);
    }
    renderChips(); updateLimit();
  }
  function openQuick(kind, category) {
    if (!dlg) return false;
    resetForm(false);
    if (!dlg.open) dlg.showModal();
    var after = function () { setKind(kind || 'EXPENSE'); if (category) pickCategory(category); $('#q-amount').focus(); };
    if (Q.data) { qState('ready'); after(); } else (Q.loading || load()).then(after);
    return true;
  }
  function selectedDate() {
    var r = $('input[name=q_day]:checked', dlg), v = r ? r.value : '0';
    return v === 'other' ? ($('#q-date').value || todayISO(0)) : todayISO(parseInt(v, 10));
  }
  function save(more) {
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
      lines: [{ category_code: Q.category, amount: String(amount) }]
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
      if (cat && Q.kind === 'EXPENSE') { cat.forecast = String(parseFloat(cat.forecast) + amount * (acc ? parseFloat(acc.rate_to_base) : 1)); }
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
    $('#q-amount').addEventListener('input', function () { $('#q-amount').removeAttribute('aria-invalid'); $('#q-amount-err').hidden = true; updateLimit(); });
    $('#q-amount').addEventListener('blur', function () { var i = $('#q-amount'), v = evalAmount(i.value); if (!isNaN(v) && /[+*/]|\d-/.test(i.value)) { i.value = String(v); updateLimit(); } });
    $('#q-account').addEventListener('change', function () { updateCurrency(); updateLimit(); });
    $('[data-action="quick-retry"]', dlg).addEventListener('click', function () { load().then(function () { setKind(Q.kind); $('#q-amount').focus(); }); });
    $$('[data-close]', dlg).forEach(function (b) { b.addEventListener('click', function () { dlg.close(); }); });
    dlg.addEventListener('click', function (e) { if (e.target === dlg) dlg.close(); });
    // «Разбить на позиции»: полная форма, введённое — первая позиция.
    $('#q-split').addEventListener('click', function (e) {
      e.preventDefault();
      var v = evalAmount($('#q-amount').value), q = new URLSearchParams();
      q.set('kind', Q.kind); q.set('back', location.pathname + location.search);
      if (!isNaN(v)) q.set('amount', String(v));
      if (Q.category) q.set('category', Q.category);
      if ($('#q-account').value) q.set('account', $('#q-account').value);
      q.set('date', selectedDate());
      if ($('#q-desc').value.trim()) { q.set('desc', $('#q-desc').value.trim()); q.set('desc_lang', lang); }
      location.href = '/transactions/new?' + q.toString();
    });
  }
  // Точки входа: «+ Расход», круглая «+», «+» в строке категории, клавиши N и I.
  doc.addEventListener('click', function (e) {
    var t = e.target.closest && e.target.closest('[data-quick]');
    if (!t || !dlg || e.metaKey || e.ctrlKey || e.shiftKey) return;
    e.preventDefault();
    openQuick(t.dataset.quick, t.dataset.category);
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
    else if (e.key === '?' && keys) { e.preventDefault(); keys.showModal(); }
  });

  // Префетч данных формы, пока пользователь думает: открытие диалога — мгновенное.
  if (dlg && canWrite && 'requestIdleCallback' in window) window.requestIdleCallback(function () { if (!Q.loading) load().then(function () { qState('ready'); }); });
})();
