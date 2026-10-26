// StashSync window. Markup and behaviour follow the design board «Стало 7» (Combo7.dc.html);
// the data comes from the Rust side through window.__ss.state() / __ss.goals().
(function () {
  'use strict';

  const VIEW = new URLSearchParams(location.search).get('view') === 'tray' ? 'tray' : 'main';
  const OK = '#46C76A', WARN = '#F5BE2E', ERR = '#FF6A55', WAIT = '#6C64A8';
  const SHOW = 5;
  const root = document.getElementById('app');

  let S = null; // snapshot from Rust
  let G = { received: false, legacy: false, items: [] };
  const U = { tab: 'sync', fstat: '', showAll: false, tour: null, trect: null, tch: 0, keyWait: false,
    drag: null, dragPos: null, bumped: {}, order: null, prevHave: {}, snap: null, advTimer: null, scrolledFor: null, tourKey: '' };

  function send(cmd, extra) {
    const msg = JSON.stringify(Object.assign({ cmd }, extra || {}));
    if (window.ipc) window.ipc.postMessage(msg);
    else if (window.__mock) window.__mock(JSON.parse(msg));
  }

  // ---------- text ----------
  function t(key) {
    const lang = (S && S.lang) || 'en';
    const table = window.I18N[lang] || window.I18N.en;
    let v = key in table ? table[key] : window.I18N.en[key];
    if (v === undefined) return key;
    if (typeof v === 'function') v = v.apply(null, Array.prototype.slice.call(arguments, 1));
    return v;
  }
  const esc = (v) => String(v).replace(/[&<>"']/g, (c) => ({ '&': '&amp;', '<': '&lt;', '>': '&gt;', '"': '&quot;', "'": '&#39;' }[c]));
  function hhmm(ms) {
    const d = new Date(ms);
    return String(d.getHours()).padStart(2, '0') + ':' + String(d.getMinutes()).padStart(2, '0');
  }
  function dayTime(ms) {
    const d = new Date(ms), now = new Date();
    if (d.toDateString() === now.toDateString()) return t('todayAt', hhmm(ms));
    return String(d.getDate()).padStart(2, '0') + '.' + String(d.getMonth() + 1).padStart(2, '0') + ' ' + hhmm(ms);
  }

  // ---------- DOM patching (keeps nodes, so hover, focus and running animations survive) ----------
  function morph(target, html) {
    const tpl = document.createElement('template');
    tpl.innerHTML = html;
    patchChildren(target, tpl.content);
  }
  function keyOf(n) { return n.nodeType === 1 ? n.getAttribute('data-key') || '' : ''; }
  function same(a, b) { return a.nodeType === b.nodeType && a.nodeName === b.nodeName && keyOf(a) === keyOf(b); }
  function patchChildren(a, b) {
    const next = Array.prototype.slice.call(b.childNodes);
    for (let i = 0; i < next.length; i++) {
      const nb = next[i];
      let na = a.childNodes[i];
      const key = keyOf(nb);
      if (key && (!na || keyOf(na) !== key)) {
        const found = Array.prototype.find.call(a.children, (c) => c.getAttribute('data-key') === key);
        if (found) { a.insertBefore(found, na || null); na = found; }
      }
      if (!na) a.appendChild(nb);
      else if (same(na, nb)) patchNode(na, nb);
      else a.replaceChild(nb, na);
    }
    while (a.childNodes.length > next.length) a.removeChild(a.lastChild);
  }
  function patchNode(a, b) {
    if (a.nodeType !== 1) { if (a.nodeValue !== b.nodeValue) a.nodeValue = b.nodeValue; return; }
    for (const at of Array.prototype.slice.call(a.attributes)) if (!b.hasAttribute(at.name)) a.removeAttribute(at.name);
    for (const at of Array.prototype.slice.call(b.attributes)) if (a.getAttribute(at.name) !== at.value) a.setAttribute(at.name, at.value);
    if (a.tagName === 'INPUT' && document.activeElement !== a && a.value !== b.getAttribute('value')) a.value = b.getAttribute('value') || '';
    patchChildren(a, b);
  }

  // ---------- icons ----------
  const ARC = (w, h) => '<svg width="' + w + '" height="' + h + '" viewBox="0 0 46 34" aria-hidden="true"><g transform="skewX(-28)"><rect x="18" y="0" width="7" height="34" fill="#FF6A55"></rect><rect x="27" y="0" width="7" height="34" fill="#F5BE2E"></rect><rect x="36" y="0" width="7" height="34" fill="#46C76A"></rect><rect x="45" y="0" width="7" height="34" fill="#45D3E6"></rect></g></svg>';
  const SYNC_PATH = 'M4 10a6 6 0 0 1 10.3-4.2L16 7.5M16 4v3.5h-3.5M16 10a6 6 0 0 1-10.3 4.2L4 12.5M4 16v-3.5h3.5';
  const CHECK = (cls, color) => '<svg width="20" height="20" viewBox="0 0 20 20" aria-hidden="true"><path class="' + cls + '" d="M5.5 10.5l3 3L14.5 7" fill="none" stroke="' + color + '" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"></path></svg>';
  const sw = (on, act, label, extra) => '<button class="cb-sw" role="switch" aria-checked="' + on + '" data-on="' + on + '" aria-label="' + esc(label) + '" data-act="' + act + '"' + (extra || '') + '><span class="cb-knob"></span></button>';

  // ---------- derived values ----------
  function clock() {
    const s = S;
    const secs = s.nextPoll ? Math.max(0, Math.ceil((s.nextPoll - Date.now()) / 1000)) : 0;
    const frac = Math.max(0, Math.min(1, secs / (s.interval * 60)));
    return { secs, frac, txt: Math.floor(secs / 60) + ':' + String(secs % 60).padStart(2, '0') };
  }

  function status() {
    const s = S, ready = s.signedIn, nosite = ready && !s.site;
    if (!ready) return ['cb-p-off', t('stNeedLogin')];
    if (s.phase === 'done') return ['cb-p-on', t('stDone')];
    if (s.phase === 'syncing') return ['cb-p-busy', t('stSyncing')];
    if (s.failed) return ['cb-p-err', t('stFailed')];
    if (nosite) return ['cb-p-warn', t('stNoSite')];
    if (!s.lastSync) return ['cb-p-off', t('stNever')];
    return ['cb-p-on', t('stOk')];
  }

  // Goal rows: fixed order (missing → partial → enough, then by shortage) so rows never jump after a sync.
  function goalRows() {
    const items = G.items;
    const rank = (it) => (G.legacy ? 0 : it.have >= it.need ? 2 : it.have > 0 ? 1 : 0);
    const known = new Set(U.order || []);
    const fresh = items.filter((it) => !known.has(it.name)).sort((a, b) => rank(a) - rank(b) || b.left - a.left).map((it) => it.name);
    const names = new Set(items.map((it) => it.name));
    U.order = (U.order || []).filter((n) => names.has(n)).concat(fresh);
    const by = {}; items.forEach((it) => { by[it.name] = it; });
    return U.order.map((n) => by[n]).map((it) => {
      const r = rank(it);
      const color = r === 2 ? OK : r === 1 ? WARN : ERR;
      const w = G.legacy || !it.need ? 0 : Math.round(Math.min(it.have, it.need) / it.need * 100);
      return { it, rank: r, st: ['none', 'partial', 'full'][r], color, w };
    });
  }

  // ---------- main window ----------
  function titlebar() {
    return '<div data-drag="1" style="flex: none; height: 48px; display: flex; align-items: center; gap: 10px; padding: 0 8px 0 18px; border-bottom: 1px solid #2A2550">' +
      ARC(26, 18) +
      '<div class="cb-u" style="font-weight: 700; font-size: 14px; letter-spacing: -0.01em">StashSync</div>' +
      '<div style="flex: 1"></div>' +
      // Ko-fi: the cup fills with the four ARC colours and steams in them on hover.
      '<button class="cb-kf" data-act="open" data-url="https://ko-fi.com/U7U21PU1TF" aria-label="' + esc(t('kofi')) + '">' +
      // The viewBox centres the cup body (not the steam above it) like the other title bar icons.
      '<svg width="24" height="24" viewBox="1 3.5 22 22" aria-hidden="true"><defs><clipPath id="cb-kfc"><path d="M5 10h11v4.5a4.5 4.5 0 0 1-4.5 4.5h-2A4.5 4.5 0 0 1 5 14.5z"></path></clipPath></defs>' +
      '<g clip-path="url(#cb-kfc)"><g class="cb-kf-fill"><g transform="translate(10.5 14.5) skewX(-20) translate(-10.5 -14.5)">' +
      '<rect x="0" y="9" width="7.75" height="11" fill="#FF6A55"></rect><rect x="7.75" y="9" width="2.85" height="11" fill="#F5BE2E"></rect><rect x="10.5" y="9" width="2.85" height="11" fill="#46C76A"></rect><rect x="13.25" y="9" width="8" height="11" fill="#45D3E6"></rect></g></g></g>' +
      '<path d="M4 9h13v5.5a5.5 5.5 0 0 1-5.5 5.5h-2A5.5 5.5 0 0 1 4 14.5zM17 10.5h1.2a2.8 2.8 0 0 1 0 5.6H17" fill="none" stroke="currentColor" stroke-width="1.7" stroke-linejoin="round"></path>' +
      '<g class="cb-kf-st"><path class="s1" d="M6.5 7c-1-1.1 1-1.9 0-3.2" stroke="#FF6A55"></path><path class="s2" d="M9.2 7c-1-1.1 1-1.9 0-3.2" stroke="#F5BE2E"></path><path class="s3" d="M11.9 7c-1-1.1 1-1.9 0-3.2" stroke="#46C76A"></path><path class="s4" d="M14.6 7c-1-1.1 1-1.9 0-3.2" stroke="#45D3E6"></path></g></svg>' +
      '<span class="cb-kf-tip">' + esc(t('kofi')) + '</span></button>' +
      '<button class="cb-wbtn" data-tour="help" aria-label="' + esc(t('help')) + '" title="' + esc(t('help')) + '" data-act="tour"><svg width="16" height="16" viewBox="0 0 16 16" aria-hidden="true"><circle cx="8" cy="8" r="6.5" fill="none" stroke="currentColor" stroke-width="1.4"></circle><path d="M6.2 6.3a1.9 1.9 0 0 1 3.7.5c0 1.3-1.9 1.6-1.9 2.8" fill="none" stroke="currentColor" stroke-width="1.4" stroke-linecap="round"></path><circle cx="8" cy="11.6" r=".9" fill="currentColor"></circle></svg></button>' +
      '<span data-tour="winbtns" style="display: flex; gap: 10px">' +
      '<button class="cb-wbtn" aria-label="' + esc(t('minimize')) + '" title="' + esc(t('minimize')) + '" data-act="minimize"><svg width="14" height="14" viewBox="0 0 14 14" aria-hidden="true"><path d="M3 7h8" stroke="currentColor" stroke-width="1.6" stroke-linecap="round"></path></svg></button>' +
      '<button class="cb-wbtn cb-wclose" aria-label="' + esc(t('quitApp')) + '" title="' + esc(t('quitApp')) + '" data-act="quit"><svg width="14" height="14" viewBox="0 0 14 14" aria-hidden="true"><path d="M3.5 3.5l7 7M10.5 3.5l-7 7" stroke="currentColor" stroke-width="1.6" stroke-linecap="round"></path></svg></button>' +
      '</span></div>';
  }

  function routeHtml() {
    const s = S;
    const steps = [s.steamOn, !s.gameOn, s.signedIn];
    let setup = 0; while (setup < 3 && steps[setup]) setup++;
    const errText = (() => {
      const e = s.loginError || '';
      if (!e) return '';
      if (/nobody is signed in/i.test(e)) return t('errSteamNoUser');
      if (/open steam/i.test(e)) return t('errOpenSteam');
      if (/close the game/i.test(e)) return '';
      return t('errLogin');
    })();
    const defs = [
      [t('rSteam'), s.steamOn ? t('rSteamOk') : t('rSteamWait')],
      s.gameOn ? [t('rGameOpen'), t('rGameOpenText')] : [t('rGameClosed'), t('rGameClosedText')],
      [t('rLogin'), errText || t('rLoginText')],
      [t('rSite'), t('rSiteText')]
    ];
    const rows = defs.map((d, i) => {
      const isDone = i < 3 ? (i < setup) : s.site, isBusy = i === 2 && s.signing, isWarn = s.gameOn && i === 1;
      const cur = !isDone && !isBusy && !isWarn && i === setup;
      const isNum = !isDone && !isBusy && !isWarn;
      const bg = isDone ? '#1E2D36' : isWarn ? '#32282E' : cur ? '#1F2A4A' : '#1E1A3A';
      const ring = isDone ? 'rgba(70,199,106,.5)' : isWarn ? WARN : cur || isBusy ? '#45D3E6' : '#3A3466';
      const fg = isDone ? OK : isWarn ? WARN : cur || isBusy ? '#45D3E6' : WAIT;
      const titleColor = isDone || cur || isWarn || isBusy ? '#F2EEE6' : '#A9A3C7';
      const textColor = isWarn ? '#E9DFC4' : i === 2 && errText && !isBusy ? '#FFB3A6' : '#A9A3C7';
      const hasAction = isWarn || (cur && i === 2 && !s.signing);
      return '<div data-key="st' + i + '" style="position: relative; display: flex; gap: 14px; padding: 12px 0; align-items: flex-start">' +
        '<span class="' + (cur ? 'cb-pulse' : '') + '" style="position: relative; z-index: 1; flex: none; width: 40px; height: 40px; border-radius: 50%; display: flex; align-items: center; justify-content: center; background: ' + bg + '; box-shadow: inset 0 0 0 1.5px ' + ring + '; color: ' + fg + '">' +
        (isDone ? CHECK('cb-check', 'currentColor') : '') +
        (isBusy ? '<svg class="cb-spin" width="20" height="20" viewBox="0 0 20 20" aria-hidden="true"><path d="M10 3a7 7 0 1 1-7 7" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round"></path></svg>' : '') +
        (isNum ? '<span class="cb-tnum" style="font-size: 15px; font-weight: 700">' + (i + 1) + '</span>' : '') +
        (isWarn ? '<svg width="20" height="20" viewBox="0 0 20 20" aria-hidden="true"><path d="M10 6v5M10 14v.1" fill="none" stroke="currentColor" stroke-width="2.2" stroke-linecap="round"></path></svg>' : '') +
        '</span><div style="flex: 1; min-width: 0; padding-top: 2px">' +
        '<div style="font-size: 15px; font-weight: 600; color: ' + titleColor + '">' + esc(d[0]) + '</div>' +
        '<div style="font-size: 13px; color: ' + textColor + '; margin-top: 3px; line-height: 1.45"' + (i === 2 && errText ? ' title="' + esc(s.loginError) + '"' : '') + '>' + esc(d[1]) + '</div>' +
        (hasAction ? '<button class="cb-btn cb-fade" style="height: 36px; margin-top: 10px; font-size: 13px" data-act="' + (isWarn ? 'recheck' : 'login') + '">' + esc(isWarn ? t('rGameClosed') : t('signIn')) + '</button>' : '') +
        '</div></div>';
    }).join('');
    return '<div class="cb-fade" data-key="route" style="padding-top: 22px">' +
      '<div class="cb-u" style="font-weight: 700; font-size: 18px; letter-spacing: -0.02em">' + esc(t('rTitle')) + '</div>' +
      '<div style="font-size: 13px; color: #A9A3C7; margin-top: 6px">' + esc(t('rSub')) + '</div>' +
      '<div data-tour="route" style="position: relative; margin-top: 18px; padding: 8px 16px; border-radius: 14px; background: #17142E"><div style="position: relative">' +
      '<div class="cb-line"><div class="cb-fill" style="height: ' + (Math.min(setup, 3) / 3 * 100) + '%"></div></div>' + rows +
      '</div></div>' +
      // What the user gets once signed in; fills the free space under the route.
      '<div style="margin-top: 14px; font-size: 12px; font-weight: 700; color: #A9A3C7; letter-spacing: .04em; text-transform: uppercase">' + esc(t('aTitle')) + '</div>' +
      [[SYNC_PATH, t('a1'), t('a1s', s.interval)], ['', t('a2'), t('a2s', s.ov.key)]].map((a) =>
        '<div style="display: flex; gap: 12px; align-items: flex-start; padding: 7px 0">' +
        '<span style="flex: none; width: 36px; height: 36px; border-radius: 10px; background: #262150; display: flex; align-items: center; justify-content: center; color: #45D3E6">' +
        (a[0] ? '<svg width="20" height="20" viewBox="0 0 20 20" aria-hidden="true"><path d="' + a[0] + '" fill="none" stroke="currentColor" stroke-width="1.6" stroke-linecap="round" stroke-linejoin="round"></path></svg>'
          : '<svg width="20" height="20" viewBox="0 0 20 20" aria-hidden="true"><rect x="2.5" y="4" width="15" height="10.5" rx="2" fill="none" stroke="currentColor" stroke-width="1.6"></rect><path d="M7 17.5h6M6 8h2.5M6 10.5h4" fill="none" stroke="currentColor" stroke-width="1.6" stroke-linecap="round"></path></svg>') +
        '</span><div><div style="font-size: 14px; font-weight: 600">' + esc(a[1]) + '</div><div style="font-size: 12px; color: #A9A3C7; margin-top: 2px; line-height: 1.45">' + esc(a[2]) + '</div></div></div>').join('') +
      '</div>';
  }

  function heroHtml() {
    const s = S, c = clock();
    const syncing = s.phase === 'syncing', done = s.phase === 'done', idle = s.phase === 'idle';
    const nosite = !s.site, failed = s.failed, bad = idle && (failed || nosite);
    const Cm = 2 * Math.PI * 25;
    const st = status();
    const showRing = idle && s.auto && !failed && !nosite;
    const ringColor = failed ? ERR : nosite ? WARN : '#45D3E6';
    const ringTrack = !idle ? '#2A2550' : failed ? 'rgba(255,106,85,.7)' : nosite ? 'rgba(245,190,46,.7)' : '#2A2550';
    const headColor = !idle ? '#F2EEE6' : failed ? '#FFB3A6' : nosite ? '#F5D57A' : '#F2EEE6';
    const actLabel = failed ? t('retryNow') : t('updateNow');
    const changes = s.changes == null ? '' : s.changes ? t('changesN', s.changes) : t('noChanges');
    const sub = syncing ? t('subReading')
      : done ? (nosite ? t('subNowhere') : t('subSiteUpdated', hhmm(s.lastSync || Date.now())))
      : failed ? (s.buildWait ? t('subFailedBuild') : t('subFailed'))
      : nosite && s.lastSync ? t('subNowhere')
      : s.lastSync ? t('updatedAt', hhmm(s.lastSync)) + changes : t('subNever');
    const autoTxt = syncing || done ? '' : failed ? (s.auto ? t('retryIn', c.txt) : '') : nosite ? t('sendWhenOpen') : s.auto ? t('autoIn', c.txt) : t('autoOff');
    const chain = [
      ['Steam', s.steamOn ? t('chReady') : t('chNotRunning'), s.steamOn ? OK : WAIT],
      [t('chLogin'), failed ? t('chNoAnswer') : t('chSaved'), failed ? ERR : OK],
      ['ARC Calc', nosite ? t('chClosed') : t('chOpen'), nosite ? WARN : OK]
    ];
    return '<div class="cb-fade" data-key="hero" style="padding-top: 20px">' +
      '<div data-tour="hero" style="display: flex; align-items: flex-start; gap: 14px; padding-bottom: 16px; border-bottom: 1px solid #2A2550">' +
      '<button class="cb-mini" data-act="sync"' + (idle ? '' : ' disabled') + ' aria-label="' + esc(actLabel) + '" title="' + esc(actLabel) + '" style="color: ' + ringColor + '">' +
      '<svg width="56" height="56" viewBox="0 0 56 56" aria-hidden="true" style="position: absolute; inset: 0; transform: rotate(-90deg)"><circle cx="28" cy="28" r="25" fill="none" stroke="' + ringTrack + '" stroke-width="3"></circle></svg>' +
      (showRing ? '<svg width="56" height="56" viewBox="0 0 56 56" aria-hidden="true" style="position: absolute; inset: 0; transform: rotate(-90deg)"><circle class="cb-prog" cx="28" cy="28" r="25" fill="none" stroke="#45D3E6" stroke-width="3" stroke-linecap="round" style="stroke-dasharray: ' + (Cm * c.frac).toFixed(1) + ' ' + Cm.toFixed(1) + '"></circle></svg>' : '') +
      (syncing ? '<svg class="cb-spin" width="56" height="56" viewBox="0 0 56 56" aria-hidden="true" style="position: absolute; inset: 0"><circle cx="28" cy="28" r="25" fill="none" stroke="#FF6A55" stroke-width="3" stroke-linecap="round" stroke-dasharray="33 124" stroke-dashoffset="0"></circle><circle cx="28" cy="28" r="25" fill="none" stroke="#F5BE2E" stroke-width="3" stroke-linecap="round" stroke-dasharray="33 124" stroke-dashoffset="-39.3"></circle><circle cx="28" cy="28" r="25" fill="none" stroke="#46C76A" stroke-width="3" stroke-linecap="round" stroke-dasharray="33 124" stroke-dashoffset="-78.5"></circle><circle cx="28" cy="28" r="25" fill="none" stroke="#45D3E6" stroke-width="3" stroke-linecap="round" stroke-dasharray="33 124" stroke-dashoffset="-117.8"></circle></svg>' : '') +
      (done ? '<svg width="56" height="56" viewBox="0 0 56 56" aria-hidden="true" style="position: absolute; inset: 0; transform: rotate(-90deg)"><circle class="cb-draw" cx="28" cy="28" r="25" fill="none" stroke="#46C76A" stroke-width="3" stroke-linecap="round"></circle></svg>' : '') +
      (!done && !bad ? '<svg width="22" height="22" viewBox="0 0 20 20" aria-hidden="true"><path d="' + SYNC_PATH + '" fill="none" stroke="currentColor" stroke-width="1.7" stroke-linecap="round" stroke-linejoin="round"></path></svg>' : '') +
      (bad ? '<svg width="22" height="22" viewBox="0 0 20 20" aria-hidden="true"><path d="M10 5.5v5.5M10 14.3v.1" fill="none" stroke="currentColor" stroke-width="2.2" stroke-linecap="round"></path></svg>' : '') +
      (done ? '<svg width="22" height="22" viewBox="0 0 20 20" aria-hidden="true"><path class="cb-check" d="M5.5 10.5l3 3L14.5 7" fill="none" stroke="#46C76A" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"></path></svg>' : '') +
      '</button>' +
      '<div style="flex: 1; min-width: 0; padding-top: 2px">' +
      '<div style="font-size: 15px; font-weight: 600; color: ' + headColor + '">' + esc(st[1]) + '</div>' +
      '<div style="font-size: 13px; color: #A9A3C7; margin-top: 3px; line-height: 1.4">' + esc(sub) + '</div>' +
      (failed || nosite ? '<div style="display: flex; gap: 12px; margin-top: 6px">' + chain.map((n) => '<span title="' + esc(n[1]) + '" style="display: flex; align-items: center; gap: 6px; font-size: 12px; color: #C9C3E6"><span style="width: 7px; height: 7px; border-radius: 50%; background: ' + n[2] + '"></span>' + esc(n[0]) + '</span>').join('') + '</div>' : '') +
      '<div style="display: flex; flex-wrap: wrap; align-items: center; justify-content: space-between; gap: 0 10px; margin-top: 6px; min-height: 32px">' +
      (idle && !nosite ? '<button class="cb-act" data-act="sync"><svg width="16" height="16" viewBox="0 0 20 20" aria-hidden="true"><path d="' + SYNC_PATH + '" fill="none" stroke="currentColor" stroke-width="1.8" stroke-linecap="round" stroke-linejoin="round"></path></svg>' + esc(actLabel) + '</button>' : '') +
      '<span class="cb-tnum" style="font-size: 12px; color: #A9A3C7; white-space: nowrap">' + esc(autoTxt) + '</span>' +
      '</div></div></div>' +
      (nosite && !syncing ? '<div class="cb-row" style="margin-top: 16px; padding: 14px 16px; border-radius: 14px; background: #262150; display: flex; gap: 12px; align-items: flex-start">' +
        '<span style="flex: none; width: 10px; height: 10px; border-radius: 50%; margin-top: 5px; background: ' + WARN + '; box-shadow: 0 0 0 4px rgba(245,190,46,.16)"></span>' +
        '<div style="flex: 1; min-width: 0"><div style="font-size: 14px; font-weight: 600">' + esc(t('nTitle')) + '</div>' +
        '<div style="font-size: 13px; color: #C9C3E6; margin-top: 4px; line-height: 1.45">' + esc(t('nText')) + '</div>' +
        '<button class="cb-ghost" style="margin-top: 10px" data-act="open" data-url="https://arccalc.github.io">' + esc(t('openSite')) + '</button></div></div>' : '') +
      goalsHtml() +
      '</div>';
  }

  function goalsHtml() {
    const s = S;
    if (!G.received) return '';
    if (!G.items.length) {
      return '<div class="cb-row" data-key="nogoals" style="margin-top: 18px; padding: 22px 18px; border-radius: 14px; background: #17142E; display: flex; flex-direction: column; align-items: flex-start; gap: 6px">' +
        '<span style="opacity: .55; display: flex">' + ARC(46, 34) + '</span>' +
        '<div style="margin-top: 8px; font-size: 16px; font-weight: 600">' + esc(t('gEmpty')) + '</div>' +
        '<div style="font-size: 13px; color: #A9A3C7; line-height: 1.45">' + esc(t('gEmptyText')) + '</div>' +
        '<button class="cb-ghost" style="margin-top: 10px" data-act="open" data-url="https://arccalc.github.io">' + esc(t('openSite')) + '</button></div>';
    }
    const rows = goalRows();
    let have = 0, need = 0, uY = 0, uR = 0, left = 0;
    const cnt = { full: 0, partial: 0, none: 0 };
    rows.forEach((r) => {
      cnt[r.st]++; left += r.it.left;
      if (!G.legacy) { have += Math.min(r.it.have, r.it.need); need += r.it.need; }
      if (r.rank === 1) uY += r.it.left; else if (r.rank === 0) uR += r.it.left;
    });
    if (U.fstat && !cnt[U.fstat]) U.fstat = '';
    const shown = rows.filter((r) => !U.fstat || r.st === U.fstat);
    const list = U.showAll ? shown : shown.slice(0, SHOW), hidden = shown.length - SHOW;
    // Rounded like the board, but never «100%» while something is still missing.
    const pct = !need ? 0 : have >= need ? 100 : Math.min(99, Math.round(have / need * 100));
    const STAT = [['full', t('fEnough'), OK], ['partial', t('fPartial'), WARN], ['none', t('fNone'), ERR]];
    const stale = !s.site && s.lastPush ? '<div style="margin-top: 10px; font-size: 12px; color: #A9A3C7">' + esc(t('lastSent', dayTime(s.lastPush))) + '</div>' : '';
    const head = '<div data-tour="goals-head">' +
      // «Show all» sits on the title row: in the chip row it wrapped and pushed the list down.
      '<div style="display: flex; align-items: center; justify-content: space-between; gap: 8px; min-height: 18px"><span style="font-size: 13px; color: #A9A3C7">' + esc(t('gTitle')) + '</span>' +
      (U.fstat ? '<button class="cb-act cb-fade" style="height: 26px; margin: -4px -10px -4px 0" data-act="filter" data-f="">' + esc(t('showAll')) + '</button>' : '') + '</div>' +
      '<div style="display: flex; align-items: flex-end; justify-content: space-between; gap: 12px; margin-top: 4px">' +
      (G.legacy ? '<div style="font-size: 12px; color: #A9A3C7; line-height: 1.4; max-width: 200px">' + esc(t('gLegacy')) + '</div>'
        : '<div style="display: flex; align-items: baseline; gap: 8px"><span class="cb-u" style="font-weight: 800; font-size: 44px; letter-spacing: -0.03em; line-height: 1">' + pct + '%</span><span style="font-size: 13px; color: #C9C3E6">' + esc(t('collected')) + '</span></div>') +
      '<div style="text-align: right; font-size: 13px; color: #A9A3C7; line-height: 1.4; white-space: nowrap">' + esc(t('leftToGet')) + '<br><span class="cb-u" style="font-weight: 700; font-size: 18px; color: #F2EEE6">' + esc(t('pcs', left)) + '</span></div>' +
      '</div>' + stale +
      (G.legacy ? '' : '<div data-tour="filters">' +
        '<div class="cb-bar" aria-hidden="true">' + STAT.map(([id, , c]) => '<span class="cb-bar-seg" style="flex-grow: ' + ({ full: have, partial: uY, none: uR }[id]) + '; background: ' + c + '; opacity: ' + (U.fstat && U.fstat !== id ? 0.3 : 1) + '"></span>').join('') + '</div>' +
        '<div class="cb-legend" role="group" aria-label="' + esc(t('showInList')) + '">' +
        STAT.map(([id, label, c]) => '<button data-key="lg-' + id + '" class="cb-lgb' + (U.fstat === id ? ' cb-lg-on' : U.fstat ? ' cb-lg-dim' : '') + '" aria-pressed="' + (U.fstat === id) + '"' + (cnt[id] ? '' : ' disabled') + ' data-act="filter" data-f="' + id + '"><span class="cb-lg" style="background: ' + c + '"></span>' + esc(label) + ' <b>' + cnt[id] + '</b></button>').join('') +
        '</div></div>') +
      '</div>';
    const items = list.map((r) => {
      const it = r.it;
      const num = G.legacy
        ? '<span style="font-size: 15px; font-weight: 700; color: ' + ERR + '">' + it.left + '</span>'
        : '<span style="font-size: 15px; font-weight: 700; color: ' + r.color + '">' + Math.min(it.have, it.need) + '</span> ' + esc(t('of')) + ' ' + it.need;
      return '<div data-key="g-' + esc(it.name) + '" style="display: grid; grid-template-columns: 36px minmax(0, 1fr) 72px; gap: 12px; align-items: center; padding: 8px 0; border-top: 1px solid #2A2550">' +
        (it.icon ? '<img src="' + it.icon + '" alt="" style="width: 36px; height: 36px; object-fit: contain; border-radius: 10px; background: #262150; padding: 3px">' : '<span style="width: 36px; height: 36px; border-radius: 10px; background: #262150"></span>') +
        '<div style="min-width: 0"><div style="font-size: 14px; white-space: nowrap; overflow: hidden; text-overflow: ellipsis">' + esc(it.label) + '</div>' +
        '<div style="margin-top: 6px; height: 4px; border-radius: 4px; background: #2A2550; overflow: hidden"><div class="cb-seg8" style="height: 4px; width: ' + r.w + '%; background: ' + r.color + '"></div></div></div>' +
        '<span class="' + (U.bumped[it.name] ? 'cb-up ' : '') + 'cb-tnum" style="justify-self: end; font-size: 13px; color: #A9A3C7; white-space: nowrap">' + num + '</span></div>';
    }).join('');
    const more = hidden > 0 ? '<div style="border-top: 1px solid #2A2550; padding-top: 6px"><button class="cb-act cb-act-mute" aria-expanded="' + U.showAll + '" data-act="more">' + esc(U.showAll ? t('showLess') : t('showMore', hidden)) +
      '<svg class="' + (U.showAll ? 'cb-chev cb-chev-on' : 'cb-chev') + '" width="14" height="14" viewBox="0 0 14 14" aria-hidden="true"><path d="M3.5 5.5L7 9l3.5-3.5" fill="none" stroke="currentColor" stroke-width="1.6" stroke-linecap="round" stroke-linejoin="round"></path></svg></button></div>' : '';
    return '<div data-key="goals" style="margin-top: 18px">' + head + '<div data-tour="list" style="margin-top: 12px">' + items + '</div>' + more + '</div>';
  }

  // Overlay preview at true scale: the primary screen drawn 360 px wide, tiles from overlay_win.rs (TILE 58, GAP 8, 8 per line, list 280 × 28).
  function ovGeo() {
    const o = S.ov, W = 360, H = Math.round(W * o.screenH / o.screenW);
    const kk = W / o.screenW * o.dpi * o.size / 100;
    const items = G.items.filter((it) => !o.hideDone || it.left > 0);
    const n = items.length, tile = 58 * kk, gap = 8 * kk, per = 8;
    let bw, bh;
    if (o.list) { bw = 280 * kk; bh = 28 * kk * n; } else {
      const cols = o.vertical ? Math.ceil(n / per) : Math.min(n, per), rows = o.vertical ? Math.min(n, per) : Math.ceil(n / per);
      bw = cols * tile + Math.max(0, cols - 1) * gap; bh = rows * tile + Math.max(0, rows - 1) * gap;
    }
    let x, y;
    if (U.dragPos) { x = U.dragPos.x; y = U.dragPos.y; }
    else if (o.corner) {
      const m = 6;
      x = o.corner[1] === 'l' ? m : W - bw - m; y = o.corner[0] === 't' ? m : H - bh - m;
    } else { x = o.x * W / o.screenW; y = o.y * W / o.screenW; }
    return { W, H, kk, tile, gap, per, items, bw, bh, x: Math.max(0, x), y: Math.max(0, y) };
  }

  function overlayHtml() {
    const o = S.ov, g = ovGeo(), kk = g.kk, f2 = (v) => v.toFixed(2) + 'px';
    const sizeP = (o.size - 50) / 110 * 100, opP = (o.opacity - 20) / 80 * 100;
    const tiles = g.items.map((it, i) => {
      const left = it.left > 0 ? it.left : '✓', cls = U.bumped[it.name] ? 'cb-bump' : 'cb-row';
      const icon = it.icon ? '<img src="' + it.icon + '" alt="" draggable="false" style="width: ' + (o.list ? f2(22 * kk) : '78%') + '; height: ' + (o.list ? f2(22 * kk) : '78%') + '; object-fit: contain">' : '<span style="width: ' + (o.list ? f2(22 * kk) : '78%') + '"></span>';
      if (o.list) {
        return '<span data-key="ov-' + esc(it.name) + '" class="' + cls + '" style="display: flex; align-items: center; gap: ' + f2(6 * kk) + '; height: ' + f2(28 * kk) + '; padding: 0 ' + f2(6 * kk) + '; font-size: ' + f2(Math.max(4, 13 * kk)) + '; color: #F2EEE6; text-shadow: 0 1px 2px #000; animation-delay: ' + (i * 0.04) + 's">' +
          icon + '<span style="flex: 1; white-space: nowrap; overflow: hidden">' + esc(it.label) + '</span><span class="cb-tnum" style="font-weight: 700">' + left + '</span></span>';
      }
      return '<span data-key="ov-' + esc(it.name) + '" class="' + cls + '" style="position: relative; width: ' + f2(g.tile) + '; height: ' + f2(g.tile) + '; flex: none; border-radius: ' + f2(8 * kk) + '; background: #15122B; box-shadow: inset 0 0 0 1px rgba(255,255,255,.12); display: flex; align-items: center; justify-content: center; animation-delay: ' + (i * 0.04) + 's">' +
        icon + '<span class="cb-tnum" style="position: absolute; right: 8%; bottom: 4%; font-weight: 700; line-height: 1; font-size: ' + f2(Math.max(4, 13 * kk)) + '; text-shadow: 0 1px 2px #000">' + left + '</span></span>';
    }).join('');
    const box = 'left: ' + g.x.toFixed(1) + 'px; top: ' + g.y.toFixed(1) + 'px; opacity: ' + o.opacity / 100 + '; display: flex; gap: ' + f2(g.gap) + '; ' +
      (o.list ? 'flex-direction: column; gap: 0; width: ' + f2(280 * kk) + '; border-radius: ' + f2(6 * kk) + '; background: #15122B;'
        : o.vertical ? 'flex-direction: column; flex-wrap: wrap; height: ' + f2(g.tile * g.per + g.gap * (g.per - 1)) + ';' : 'flex-wrap: wrap; width: ' + f2(g.tile * g.per + g.gap * (g.per - 1)) + ';');
    const corners = [['tl', t('cTL'), 'left: 4px; top: 4px'], ['tr', t('cTR'), 'right: 4px; top: 4px'], ['bl', t('cBL'), 'left: 4px; bottom: 4px'], ['br', t('cBR'), 'right: 4px; bottom: 4px']];
    const seg = (on, act, label, disabled) => '<button data-on="' + on + '" data-act="' + act + '"' + (disabled ? ' disabled' : '') + '>' + esc(label) + '</button>';
    // Title, preview and hint stay put; only the settings below scroll, so the preview stays in sight.
    const head = '<div class="cb-fade" data-key="overlay" style="flex: none; padding: 20px 20px 0">' +
      '<div style="display: flex; align-items: center; gap: 12px"><div style="flex: 1">' +
      '<div class="cb-u" style="font-weight: 700; font-size: 18px; letter-spacing: -0.02em">' + esc(t('ovTitle')) + '</div>' +
      '<div style="font-size: 13px; color: #A9A3C7; margin-top: 4px; line-height: 1.4">' + esc(t('ovSub')) + '</div></div>' +
      '<button class="cb-ghost" aria-pressed="' + o.shown + '" data-act="ovShown">' + esc(o.shown ? t('hide') : t('show')) + ' <span style="font-size: 12px; font-weight: 700; color: #A9A3C7">' + esc(o.key) + '</span></button></div>' +
      '<div class="cb-scene" data-tour="scene" style="position: relative; margin-top: 14px; width: 360px; height: ' + g.H + 'px; border-radius: 10px; overflow: hidden; box-shadow: inset 0 0 0 1px #2F2A55">' +
      (g.items.length ? '<div class="' + (U.drag ? 'cb-ov' : 'cb-ov cb-move') + '" data-ovbox="1" style="position: absolute; ' + box + '">' + tiles + '</div>'
        : '<div style="position: absolute; inset: 0; display: flex; align-items: center; justify-content: center; font-size: 13px; color: #F2EEE6; text-shadow: 0 1px 3px #000">' + esc(t('gEmpty')) + '</div>') +
      '</div>' +
      '<div style="margin-top: 8px; font-size: 12px; color: #A9A3C7; line-height: 1.45">' + esc(t('ovHint', o.screenW, o.screenH)) + '</div></div>';
    const settings = '<div class="cb-fade" data-key="overlay-set">' +
      '<div data-tour="ov-look"><div class="cb-card" style="margin-top: 16px">' +
      '<div class="cb-crow"><span class="cb-clab">' + esc(t('corner')) + '</span><div style="display: flex; gap: 6px">' +
      corners.map((c) => '<button class="cb-corner" data-on="' + (o.corner === c[0]) + '" aria-label="' + esc(c[1]) + '" title="' + esc(c[1]) + '" data-act="ovCorner" data-c="' + c[0] + '"><span style="' + c[2] + '"></span></button>').join('') + '</div></div>' +
      '<div class="cb-crow cb-cdiv"><span class="cb-clab">' + esc(t('style')) + '</span><div class="cb-seg" style="width: 200px">' + seg(!o.list, 'ovTiles', t('tiles')) + seg(o.list, 'ovListMode', t('list')) + '</div></div>' +
      '<div class="cb-crow cb-cdiv"><span class="cb-clab">' + esc(t('row')) + '</span><div class="cb-seg" style="width: 200px">' + seg(!o.vertical, 'ovAcross', t('across'), o.list) + seg(o.vertical, 'ovDown', t('down'), o.list) + '</div></div>' +
      '</div><div class="cb-card">' +
      '<div class="cb-cslide"><div class="cb-cline"><label for="cb-size" class="cb-clab">' + esc(t('size')) + '</label><span class="cb-tnum" style="font-size: 13px; font-weight: 600; color: #C9C3E6">' + o.size + '%</span></div><input id="cb-size" class="cb-range" type="range" min="50" max="160" step="5" value="' + o.size + '" data-input="ovSize" style="--p: ' + sizeP + '%"></div>' +
      '<div class="cb-cslide cb-cdiv"><div class="cb-cline"><label for="cb-op" class="cb-clab">' + esc(t('opacity')) + '</label><span class="cb-tnum" style="font-size: 13px; font-weight: 600; color: #C9C3E6">' + o.opacity + '%</span></div><input id="cb-op" class="cb-range" type="range" min="20" max="100" step="1" value="' + o.opacity + '" data-input="ovOpacity" style="--p: ' + opP + '%"></div>' +
      '</div></div>' +
      '<div class="cb-card" data-tour="ov-key">' +
      '<div class="cb-crow"><div style="min-width: 0"><div class="cb-clab">' + esc(t('key')) + '</div><div class="cb-csub">' + esc(t('keySub')) + '</div></div><button class="cb-key" data-wait="' + U.keyWait + '" aria-label="' + esc(t('key') + ': ' + o.key) + '" data-act="askKey">' + esc(U.keyWait ? '…' : o.key) + '</button></div>' +
      '<div class="cb-crow cb-cdiv"><div style="min-width: 0"><div class="cb-clab">' + esc(t('hideDone')) + '</div><div class="cb-csub">' + esc(t('hideDoneSub')) + '</div></div>' + sw(o.hideDone, 'ovHideDone', t('hideDone')) + '</div>' +
      '</div>' +
      '<button class="cb-ghost" data-act="ovReset" style="margin-top: 16px">' + esc(t('reset')) + '</button>' +
      '</div>';
    return [head, settings];
  }

  function settingsHtml() {
    const s = S;
    const langs = ['en', 'ru', 'de', 'fr'];
    return '<div class="cb-fade" data-key="settings" style="padding-top: 20px">' +
      '<div class="cb-u" style="font-weight: 700; font-size: 18px; letter-spacing: -0.02em">' + esc(t('settings')) + '</div>' +
      '<div class="cb-card" data-tour="set-auto" style="margin-top: 16px">' +
      '<div class="cb-crow"><div style="min-width: 0"><div class="cb-clab">' + esc(t('autoUpdate')) + '</div><div class="cb-csub">' + esc(t('autoUpdateSub')) + '</div></div>' + sw(s.auto, 'setAuto', t('autoUpdate')) + '</div>' +
      '<div class="cb-crow cb-cdiv" style="opacity: ' + (s.auto ? 1 : 0.45) + '"><span class="cb-clab">' + esc(t('every')) + '</span>' +
      '<div style="display: flex; align-items: center; height: 40px; border-radius: 10px; background: #15122B; box-shadow: inset 0 0 0 1px #2F2A55">' +
      '<button class="cb-step" aria-label="' + esc(t('rarer')) + '" data-act="intDec"' + (!s.auto || s.interval <= 1 ? ' disabled' : '') + '>−</button>' +
      '<span class="cb-tnum" style="min-width: 64px; text-align: center; font-weight: 600; font-size: 15px">' + esc(t('minutes', s.interval)) + '</span>' +
      '<button class="cb-step" aria-label="' + esc(t('oftener')) + '" data-act="intInc"' + (!s.auto || s.interval >= 30 ? ' disabled' : '') + '>+</button>' +
      '</div></div></div>' +
      '<div class="cb-card"><div class="cb-crow"><span class="cb-clab">' + esc(t('startup')) + '</span>' + sw(s.startup, 'setStartup', t('startup')) + '</div></div>' +
      '<div class="cb-card" style="padding: 14px 16px"><div class="cb-clab" style="margin-bottom: 10px">' + esc(t('language')) + '</div>' +
      '<div class="cb-seg">' + langs.map((l) => '<button data-on="' + (s.lang === l) + '" data-act="setLang" data-lang="' + l + '">' + l.toUpperCase() + '</button>').join('') + '</div></div>' +
      '<div class="cb-card" data-tour="set-login"><div class="cb-crow"><div style="min-width: 0"><div class="cb-clab">' + esc(t('login')) + '</div><div class="cb-csub">' + esc(s.signedIn ? t('chSaved') : t('notSigned')) + '</div></div>' +
      '<button class="cb-ghost" data-act="signOut"' + (s.signedIn ? '' : ' disabled') + '>' + esc(t('signOut')) + '</button></div></div>' +
      '<div style="margin-top: 16px; display: flex; justify-content: space-between; gap: 12px; font-size: 12px; color: #A9A3C7">' +
      '<span style="white-space: nowrap">StashSync ' + esc(s.version) + '</span>' +
      '<a href="#" data-act="open" data-url="https://github.com/Arccalc/StashSync" style="text-align: right">' + esc(t('privacy')) + '</a></div>' +
      '</div>';
  }

  function navHtml() {
    const tab = (id, label, icon, tour) => '<button class="cb-tab"' + (tour ? ' data-tour="' + tour + '"' : '') + ' data-on="' + (U.tab === id) + '" data-act="tab" data-tab="' + id + '">' + icon + esc(label) + '</button>';
    return '<nav style="flex: none; display: flex; border-top: 1px solid #2A2550; background: #1A1634">' +
      tab('sync', t('tabStash'), '<svg width="20" height="20" viewBox="0 0 20 20" aria-hidden="true"><path d="' + SYNC_PATH + '" fill="none" stroke="currentColor" stroke-width="1.6" stroke-linecap="round" stroke-linejoin="round"></path></svg>') +
      tab('overlay', t('tabOverlay'), '<svg width="20" height="20" viewBox="0 0 20 20" aria-hidden="true"><rect x="2.5" y="4" width="15" height="10" rx="2" fill="none" stroke="currentColor" stroke-width="1.6"></rect><rect x="4.8" y="6.2" width="2.6" height="2.6" rx=".6" fill="currentColor"></rect><rect x="8.4" y="6.2" width="2.6" height="2.6" rx=".6" fill="currentColor"></rect><path d="M7 17h6" stroke="currentColor" stroke-width="1.6" stroke-linecap="round"></path></svg>', 'tab-overlay') +
      tab('settings', t('tabSettings'), '<svg width="20" height="20" viewBox="0 0 20 20" aria-hidden="true"><path d="M3 6h8.5M15.5 6H17M3 14h2.5M9.5 14H17" fill="none" stroke="currentColor" stroke-width="1.6" stroke-linecap="round"></path><circle cx="13.5" cy="6" r="2" fill="none" stroke="currentColor" stroke-width="1.6"></circle><circle cx="7.5" cy="14" r="2" fill="none" stroke="currentColor" stroke-width="1.6"></circle></svg>', 'tab-settings') +
      '</nav>';
  }

  function mainHtml() {
    const [fixed, body] = U.tab === 'overlay' ? overlayHtml() : ['', U.tab === 'settings' ? settingsHtml() : S.signedIn ? heroHtml() : routeHtml()];
    // The fixed slot is always there, so the scroller and the tab bar keep their nodes across tabs.
    return titlebar() + '<div style="flex: none">' + fixed + '</div>' +
      '<div class="cb-scroll" style="flex: 1; min-height: 0; padding: 0 10px 0 20px">' + body + '</div>' +
      navHtml() + tourHtml();
  }

  // ---------- tray flyout ----------
  function trayHtml() {
    const s = S, c = clock(), ready = s.signedIn, nosite = !s.site;
    const syncing = s.phase === 'syncing', idle = s.phase === 'idle';
    const st = status();
    const timer = ready && s.auto && idle && !s.failed && !nosite ? t('autoIn', c.txt) : '';
    const changes = s.changes == null ? '' : s.changes ? t('changesN', s.changes) : t('noChanges');
    const text = !ready ? t('flyLogin') : syncing ? t('subReading') : s.failed ? t('subFailed') : nosite && s.lastSync ? t('flyNowhere')
      : s.lastSync ? t('updatedAt', hhmm(s.lastSync)) : t('subNever');
    // The change count goes to the second line, next to the timer, so the first line stays short.
    const plain = ready && !syncing && !s.failed && !nosite && s.lastSync;
    const sub = [plain ? changes.replace(/^ · /, '') : '', timer].filter(Boolean).join(' · ');
    const label = s.failed ? t('retryNow') : t('updateNow');
    // Ring = countdown to the next auto update; a click updates now (same as the main window).
    const Cm = 2 * Math.PI * 21, bad = ready && idle && (s.failed || nosite);
    const ringColor = s.failed ? ERR : nosite ? WARN : '#45D3E6';
    const ring = !ready ? '' : '<button class="cb-mini cb-mini-s" data-act="sync"' + (idle ? '' : ' disabled') + ' aria-label="' + esc(label) + '" title="' + esc(label) + '" style="color: ' + ringColor + '">' +
      '<svg width="48" height="48" viewBox="0 0 48 48" aria-hidden="true" style="position: absolute; inset: 0; transform: rotate(-90deg)"><circle cx="24" cy="24" r="21" fill="none" stroke="' + (bad ? ringColor : '#2F2A55') + '" stroke-width="3"></circle>' +
      (timer ? '<circle class="cb-prog" cx="24" cy="24" r="21" fill="none" stroke="#45D3E6" stroke-width="3" stroke-linecap="round" style="stroke-dasharray: ' + (Cm * c.frac).toFixed(1) + ' ' + Cm.toFixed(1) + '"></circle>' : '') + '</svg>' +
      (bad ? '<svg width="20" height="20" viewBox="0 0 20 20" aria-hidden="true"><path d="M10 5.5v5.5M10 14.3v.1" fill="none" stroke="currentColor" stroke-width="2.2" stroke-linecap="round"></path></svg>'
        : '<svg class="' + (syncing ? 'cb-spin' : '') + '" width="20" height="20" viewBox="0 0 20 20" aria-hidden="true"><path d="' + SYNC_PATH + '" fill="none" stroke="currentColor" stroke-width="1.8" stroke-linecap="round" stroke-linejoin="round"></path></svg>') +
      '</button>';
    // Goal progress at a glance, the same numbers as the main window: % on the status row,
    // the coloured line right under it (in place of the divider).
    let side = '', line = '<div style="margin: 14px 16px 0; height: 1px; background: #2A2550"></div>';
    if (ready && G.items.length) {
      let have = 0, need = 0, uY = 0, uR = 0, left = 0;
      goalRows().forEach((r) => {
        left += r.it.left;
        if (!G.legacy) { have += Math.min(r.it.have, r.it.need); need += r.it.need; }
        if (r.rank === 1) uY += r.it.left; else if (r.rank === 0) uR += r.it.left;
      });
      const pct = !need ? 0 : have >= need ? 100 : Math.min(99, Math.round(have / need * 100));
      side = '<div style="flex: none; text-align: right" title="' + esc(t('gTitle')) + '">' +
        (G.legacy ? '' : '<div class="cb-u cb-tnum" style="font-size: 20px; font-weight: 700; line-height: 1.1">' + pct + '%</div>') +
        '<div class="cb-tnum" style="font-size: 12px; color: #A9A3C7; margin-top: 2px; white-space: nowrap">' + esc(t('leftShort', left)) + '</div></div>';
      if (!G.legacy) {
        line = '<div class="cb-bar cb-bar-line" aria-hidden="true"><span class="cb-bar-seg" style="flex-grow: ' + have + '; background: ' + OK + '"></span><span class="cb-bar-seg" style="flex-grow: ' + uY + '; background: ' + WARN + '"></span><span class="cb-bar-seg" style="flex-grow: ' + uR + '; background: ' + ERR + '"></span></div>';
      }
    }
    const exit = '<svg width="16" height="16" viewBox="0 0 20 20" aria-hidden="true"><path d="M10 3v6.5M6 5.8a6 6 0 1 0 8 0" fill="none" stroke="currentColor" stroke-width="1.8" stroke-linecap="round"></path></svg>';
    // flex: none, or the flex parent squeezes it to the current window height and the window never grows.
    return '<div id="fly" style="position: relative; flex: none; background: #1E1A3A; overflow: hidden">' +
      (syncing ? '<div class="cb-sweep"></div>' : '') +
      '<div style="height: 48px; display: flex; align-items: center; gap: 10px; padding: 0 12px 0 16px; border-bottom: 1px solid #2A2550">' + ARC(22, 16) +
      '<span class="cb-u" style="font-weight: 700; font-size: 13px">StashSync</span><span style="flex: 1"></span>' +
      '<span class="cb-pill cb-pill-s ' + st[0] + '"><span class="cb-dot"></span>' + esc(st[1]) + '</span></div>' +
      '<div style="padding: 14px 16px 0; display: flex; align-items: center; gap: 12px">' + ring +
      '<div style="flex: 1; min-width: 0"><div style="font-size: 14px; font-weight: 600; line-height: 1.35">' + esc(text) + '</div>' +
      (sub ? '<div class="cb-tnum" style="font-size: 12px; color: #A9A3C7; margin-top: 3px">' + esc(sub) + '</div>' : '') + '</div>' + side + '</div>' +
      (!ready ? '<div style="padding: 14px 16px 0"><button class="cb-btn" style="width: 100%; height: 42px" data-act="openWindow">' + esc(t('openAndLogin')) + '</button></div>' : '') +
      line +
      '<div style="display: flex; align-items: center; gap: 10px; margin: 0 16px; padding: 12px 0; border-bottom: 1px solid #2A2550">' +
      '<svg width="20" height="20" viewBox="0 0 20 20" aria-hidden="true" style="flex: none; color: #A9A3C7"><rect x="2.5" y="4" width="15" height="10.5" rx="2" fill="none" stroke="currentColor" stroke-width="1.6"></rect><path d="M7 17.5h6M6 8h2.5M6 10.5h4" fill="none" stroke="currentColor" stroke-width="1.6" stroke-linecap="round"></path></svg>' +
      '<span style="flex: 1; font-size: 14px">' + esc(t('ovTitle')) + '</span><span class="cb-fkey">' + esc(s.ov.key) + '</span>' + sw(s.ov.shown, 'ovShown', t('ovTitle')) + '</div>' +
      '<div style="display: flex; align-items: center; gap: 4px; padding: 6px 8px 8px">' +
      // Before sign-in the primary button above already opens the window.
      (!ready ? '<span style="flex: 1; height: 44px"></span>' : '<button class="cb-fopen" data-act="openWindow"><svg width="20" height="20" viewBox="0 0 20 20" aria-hidden="true" style="color: #45D3E6"><path d="M8.5 3.5H5A1.5 1.5 0 0 0 3.5 5v10A1.5 1.5 0 0 0 5 16.5h10a1.5 1.5 0 0 0 1.5-1.5v-3.5M11.5 3.5h5v5M16.5 3.5L10 10" fill="none" stroke="currentColor" stroke-width="1.6" stroke-linecap="round" stroke-linejoin="round"></path></svg>' + esc(t('openApp')) +
      '<svg class="cb-chevr" width="16" height="16" viewBox="0 0 16 16" aria-hidden="true"><path d="M6 3.5L10.5 8 6 12.5" fill="none" stroke="currentColor" stroke-width="1.6" stroke-linecap="round" stroke-linejoin="round"></path></svg></button>') +
      '<button class="cb-fquit" data-act="quit" title="' + esc(t('quitApp')) + '">' + exit + esc(t('quitShort')) + '</button></div></div>';
  }

  // ---------- tour (same model as the site: ring + dim + card, hybrid steps) ----------
  function tourSteps() {
    const s = S, steps = [];
    if (!s.signedIn) steps.push({ at: ['route'], enter: { tab: 'sync' }, text: t('t_route') });
    else {
      steps.push({ at: ['hero'], enter: { tab: 'sync' }, text: t('t_hero') });
      if (G.items.length && !G.legacy) {
        const cnt = { none: 0, partial: 0 };
        goalRows().forEach((r) => { if (r.st in cnt) cnt[r.st]++; });
        const f = cnt.none ? 'none' : cnt.partial ? 'partial' : 'full';
        steps.push({ at: ['goals-head'], enter: { tab: 'sync' }, text: t('t_goals') });
        steps.push({ at: ['filters'], enter: { tab: 'sync', fstat: '' }, text: t('t_filters', f === 'none' ? t('fNone') : f === 'partial' ? t('fPartial') : t('fEnough')), until: () => U.fstat === f, act: { fstat: f } });
        steps.push({ at: ['list'], enter: { tab: 'sync', fstat: f }, text: t('t_list') });
      }
    }
    steps.push({ at: ['tab-overlay'], enter: { tab: 'sync', fstat: '' }, text: t('t_tabOverlay'), until: () => U.tab === 'overlay', act: { tab: 'overlay' } });
    steps.push({ at: ['scene'], enter: { tab: 'overlay' }, text: t('t_scene') });
    steps.push({ at: ['ov-look'], enter: { tab: 'overlay' }, text: t('t_look') });
    steps.push({ at: ['ov-key'], enter: { tab: 'overlay' }, text: t('t_key') });
    steps.push({ at: ['tab-settings'], enter: { tab: 'overlay' }, text: t('t_tabSettings'), until: () => U.tab === 'settings', act: { tab: 'settings' } });
    steps.push({ at: ['set-auto'], enter: { tab: 'settings' }, text: t('t_auto') });
    steps.push({ at: ['set-login'], enter: { tab: 'settings' }, text: t('t_login') });
    steps.push({ at: ['winbtns'], enter: { tab: 'sync' }, text: t('t_tray') });
    steps.push({ at: ['help'], text: t('t_help') });
    return steps;
  }
  function startTour() {
    U.snap = { tab: U.tab, fstat: U.fstat, showAll: U.showAll };
    U.showAll = false;
    goStep(0);
  }
  function goStep(i) {
    const steps = tourSteps(), st = steps[i];
    if (!st) { endTour(); return; }
    clearTimeout(U.advTimer); U.advTimer = null; U.scrolledFor = null; U.tourKey = '';
    const sc = root.querySelector('.cb-scroll');
    if (sc && st.enter && st.enter.tab) sc.scrollTop = 0;
    Object.assign(U, { tour: { i }, trect: null }, st.enter || {});
    render();
    setTimeout(measureTour, 80);
    setTimeout(measureTour, 520);
  }
  function tourNext() {
    const tr = U.tour; if (!tr) return;
    const steps = tourSteps(), st = steps[tr.i];
    if (st && st.until && !st.until() && st.act) Object.assign(U, st.act);
    if (tr.i >= steps.length - 1) endTour(); else goStep(tr.i + 1);
  }
  function tourBack() { const tr = U.tour; if (tr && tr.i > 0) goStep(tr.i - 1); }
  function endTour() {
    clearTimeout(U.advTimer); U.advTimer = null;
    Object.assign(U, { tour: null, trect: null }, U.snap || {});
    render();
  }
  function measureTour() {
    const tr = U.tour; if (!tr) return;
    const step = tourSteps()[tr.i]; if (!step) return;
    if (step.until && step.until() && !U.advTimer) {
      const i = tr.i;
      U.advTimer = setTimeout(() => { U.advTimer = null; if (U.tour && U.tour.i === i) goStep(i + 1); }, 650);
    }
    const nodes = (step.at || []).map((id) => root.querySelector('[data-tour="' + id + '"]')).filter((n) => n && n.getBoundingClientRect().width > 1);
    if (nodes.length && U.scrolledFor !== tr.i) {
      U.scrolledFor = tr.i;
      const sc = nodes[0].closest('.cb-scroll');
      if (sc) {
        const nb = nodes[0].getBoundingClientRect(), sb = sc.getBoundingClientRect();
        if (nb.top < sb.top || nb.bottom > sb.bottom) sc.scrollTop = Math.max(0, sc.scrollTop + (nb.top - sb.top) - 16);
      }
    }
    const rects = nodes.map((n) => {
      const b = n.getBoundingClientRect(); let top = b.top, bottom = b.bottom;
      const sc = n.closest('.cb-scroll'); if (sc) { const pb = sc.getBoundingClientRect(); top = Math.max(top, pb.top); bottom = Math.min(bottom, pb.bottom); }
      return { l: Math.round(b.left - 6), t: Math.round(top - 6), r: Math.round(b.right + 6), b: Math.round(bottom + 6) };
    }).filter((x) => x.b - x.t > 8);
    const card = document.getElementById('cb-tc'), ch = card ? card.offsetHeight : 0;
    const key = rects.map((x) => [x.l, x.t, x.r, x.b].join(',')).join('|') + '#' + ch;
    if (key !== U.tourKey) { U.tourKey = key; U.trect = rects.length ? rects : null; U.tch = ch; render(); }
  }
  function tourHtml() {
    const tr = U.tour; if (!tr) return '';
    const steps = tourSteps(), st = steps[tr.i] || {}, rects = U.trect || [];
    const W = window.innerWidth, H = window.innerHeight, cw = 300, ch = U.tch || 220, gap = 16, m = 16;
    let cl = W / 2 - cw / 2, ct = H / 2 - ch / 2;
    if (rects.length) {
      const R = { l: Math.min(...rects.map((x) => x.l)), t: Math.min(...rects.map((x) => x.t)), r: Math.max(...rects.map((x) => x.r)), b: Math.max(...rects.map((x) => x.b)) };
      const cx = (x) => Math.max(m, Math.min(W - cw - m, x)), cy = (y) => Math.max(m, Math.min(H - ch - m, y));
      if (R.r + gap + cw <= W - m) { cl = R.r + gap; ct = cy(R.t); }
      else if (R.l - gap - cw >= m) { cl = R.l - gap - cw; ct = cy(R.t); }
      else if (R.b + gap + ch <= H - m) { ct = R.b + gap; cl = cx(R.l); }
      else if (R.t - gap - ch >= m) { ct = R.t - gap - ch; cl = cx(R.l); }
      else { ct = cy(H - ch - m); cl = cx(R.l); }
    }
    const hole = (x) => { const q = Math.min(10, (x.r - x.l) / 2, (x.b - x.t) / 2); return 'M' + (x.l + q) + ' ' + x.t + 'H' + (x.r - q) + 'A' + q + ' ' + q + ' 0 0 1 ' + x.r + ' ' + (x.t + q) + 'V' + (x.b - q) + 'A' + q + ' ' + q + ' 0 0 1 ' + (x.r - q) + ' ' + x.b + 'H' + (x.l + q) + 'A' + q + ' ' + q + ' 0 0 1 ' + x.l + ' ' + (x.b - q) + 'V' + (x.t + q) + 'A' + q + ' ' + q + ' 0 0 1 ' + (x.l + q) + ' ' + x.t + 'Z'; };
    const last = tr.i === steps.length - 1, wait = !!(st.until && !st.until());
    return '<div class="cb-tdim" aria-hidden="true" style="clip-path: path(evenodd, \'M0 0H' + W + 'V' + H + 'H0Z' + rects.map(hole).join('') + '\')"></div>' +
      rects.map((x, i) => '<div class="cb-tring" data-key="ring' + i + '" style="left: ' + x.l + 'px; top: ' + x.t + 'px; width: ' + (x.r - x.l) + 'px; height: ' + (x.b - x.t) + 'px"></div>').join('') +
      '<div id="cb-tc" class="cb-tc" role="dialog" aria-label="' + esc(t('tourAria')) + '" style="left: ' + Math.round(cl) + 'px; top: ' + Math.round(ct) + 'px">' +
      '<div style="display: flex; align-items: center; justify-content: space-between; gap: 10px"><span class="cb-tstep">' + esc(t('stepOf', tr.i + 1, steps.length)) + '</span>' +
      '<button class="cb-wbtn" style="width: 28px; height: 28px" aria-label="' + esc(t('tourClose')) + '" data-act="tourEnd"><svg width="12" height="12" viewBox="0 0 14 14" aria-hidden="true"><path d="M3.5 3.5l7 7M10.5 3.5l-7 7" stroke="currentColor" stroke-width="1.6" stroke-linecap="round"></path></svg></button></div>' +
      '<p class="cb-ttext" aria-live="polite">' + esc(st.text || '') + '</p>' +
      (wait ? '<p class="cb-twait">' + esc(t('tourWait')) + '</p>' : '') +
      (st.until && !wait ? '<p class="cb-tok">' + esc(t('tourOk')) + '</p>' : '') +
      '<div class="cb-tact"><button class="cb-ghost" data-act="tourBack"' + (tr.i === 0 ? ' disabled' : '') + '>' + esc(t('back')) + '</button>' +
      '<button class="cb-btn" style="height: 36px; font-size: 13px" data-act="tourNext">' + esc(last ? t('finish') : t('next')) + '</button></div>' +
      (!last ? '<button class="cb-tskip" data-act="tourEnd">' + esc(t('tourSkip')) + '</button>' : '') +
      '</div>';
  }

  // ---------- render loop ----------
  let queued = false;
  function render() {
    if (!S) return;
    document.documentElement.lang = S.lang;
    if (VIEW === 'tray') {
      morph(root, trayHtml());
      const fly = document.getElementById('fly');
      if (fly) {
        const h = Math.ceil(fly.offsetHeight);
        if (h && h !== U.flyH) { U.flyH = h; send('flySize', { v: h }); }
      }
    } else {
      morph(root, mainHtml());
    }
    if (U.tour) requestAnimationFrame(measureTour);
  }
  function schedule() {
    if (queued) return;
    queued = true;
    requestAnimationFrame(() => { queued = false; render(); });
  }
  // Countdown and «done» marks change every second without a new snapshot.
  setInterval(() => { if (S && !document.hidden && !U.away) render(); }, 1000);

  // ---------- actions ----------
  const ACT = {
    tour: () => startTour(),
    tourNext: () => tourNext(),
    tourBack: () => tourBack(),
    tourEnd: () => endTour(),
    minimize: () => { if (U.tour) endTour(); send('minimize'); },
    quit: () => send('quit'),
    sync: () => send('sync'),
    login: () => send('login'),
    recheck: () => send('recheck'),
    openWindow: () => send('openWindow'),
    open: (el) => send('open', { url: el.getAttribute('data-url') }),
    tab: (el) => { U.tab = el.getAttribute('data-tab'); const sc = root.querySelector('.cb-scroll'); if (sc) sc.scrollTop = 0; },
    filter: (el) => { const f = el.getAttribute('data-f'); U.fstat = U.fstat === f ? '' : f; U.showAll = false; },
    more: () => { U.showAll = !U.showAll; },
    ovShown: () => send('ovShown', { on: !S.ov.shown }),
    ovTiles: () => send('ovList', { on: false }),
    ovListMode: () => send('ovList', { on: true }),
    ovAcross: () => send('ovVertical', { on: false }),
    ovDown: () => send('ovVertical', { on: true }),
    ovCorner: (el) => send('ovCorner', { corner: el.getAttribute('data-c') }),
    ovHideDone: () => send('ovHideDone', { on: !S.ov.hideDone }),
    ovReset: () => { U.keyWait = false; send('ovReset'); },
    askKey: (el) => { U.keyWait = !U.keyWait; el.focus(); },
    setAuto: () => send('setAuto', { on: !S.auto }),
    intDec: () => send('setInterval', { v: S.interval - 1 }),
    intInc: () => send('setInterval', { v: S.interval + 1 }),
    setStartup: () => send('setStartup', { on: !S.startup }),
    setLang: (el) => send('setLang', { lang: el.getAttribute('data-lang') }),
    signOut: () => send('signOut')
  };

  document.addEventListener('click', (e) => {
    const el = e.target.closest('[data-act]');
    if (!el || el.disabled) return;
    if (el.tagName === 'A') e.preventDefault();
    const fn = ACT[el.getAttribute('data-act')];
    if (fn) { fn(el, e); render(); }
  });
  document.addEventListener('input', (e) => {
    const el = e.target.closest('[data-input]');
    if (!el) return;
    const v = Number(el.value), cmd = el.getAttribute('data-input');
    // Show the new value at once; Rust confirms with the next snapshot.
    if (cmd === 'ovSize') S.ov.size = v; else if (cmd === 'ovOpacity') S.ov.opacity = v;
    send(cmd, { v });
    render();
  });
  document.addEventListener('mousedown', (e) => {
    if (e.button !== 0) return;
    if (e.target.closest('[data-drag]') && !e.target.closest('button')) { e.preventDefault(); send('drag'); }
  });
  document.addEventListener('contextmenu', (e) => e.preventDefault());
  document.addEventListener('keydown', (e) => {
    if (!U.keyWait) return;
    e.preventDefault();
    U.keyWait = false;
    if (e.key !== 'Escape' && e.keyCode) send('ovKey', { v: e.keyCode });
    render();
  });
  document.addEventListener('scroll', () => { if (U.tour) requestAnimationFrame(measureTour); }, true);

  // Drag the overlay inside the preview. Screen pixels go to Rust on release.
  document.addEventListener('pointerdown', (e) => {
    const box = e.target.closest('[data-ovbox]');
    if (!box || e.button !== 0) return;
    const g = ovGeo();
    U.drag = { x: e.clientX, y: e.clientY, ox: g.x, oy: g.y };
    U.dragPos = { x: g.x, y: g.y };
    box.setPointerCapture(e.pointerId);
    render();
  });
  document.addEventListener('pointermove', (e) => {
    if (!U.drag) return;
    const d = U.drag, g = ovGeo();
    U.dragPos = { x: Math.max(0, Math.min(g.W - g.bw, d.ox + e.clientX - d.x)), y: Math.max(0, Math.min(g.H - g.bh, d.oy + e.clientY - d.y)) };
    schedule();
  });
  function dragEnd() {
    if (!U.drag) return;
    const k = S.ov.screenW / 360, p = U.dragPos;
    U.drag = null;
    if (p) {
      S.ov.corner = ''; S.ov.x = Math.round(p.x * k); S.ov.y = Math.round(p.y * k);
      send('ovPos', { x: S.ov.x, y: S.ov.y });
    }
    U.dragPos = null;
    render();
  }
  document.addEventListener('pointerup', dragEnd);
  document.addEventListener('pointercancel', dragEnd);

  // ---------- bridge ----------
  window.__ss = {
    // The window went to the tray (false) or came back (true).
    visible(on) {
      U.away = !on;
      if (on) schedule();
    },
    state(next) {
      S = next;
      if (U.drag) return;
      schedule();
    },
    goals(next) {
      const prev = U.prevHave, bumped = {};
      next.items.forEach((it) => {
        if (prev[it.name] !== undefined && (it.have > prev[it.name] || it.left < (U.prevLeft || {})[it.name])) bumped[it.name] = true;
      });
      U.prevHave = {}; U.prevLeft = {};
      next.items.forEach((it) => { U.prevHave[it.name] = it.have; U.prevLeft[it.name] = it.left; });
      G = next;
      if (Object.keys(bumped).length) {
        U.bumped = bumped;
        setTimeout(() => { U.bumped = {}; schedule(); }, 2600);
      }
      schedule();
    }
  };
  send('ready');
})();
