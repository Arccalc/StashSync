// StashSync window. Markup and behaviour follow the design board «Стало 7» (Combo7.dc.html);
// the data comes from the Rust side through window.__ss.state() / __ss.goals().
(function () {
  'use strict';

  const VIEW = new URLSearchParams(location.search).get('view') === 'tray' ? 'tray' : 'main';
  const OK = '#46C76A', WARN = '#F5BE2E', ERR = '#FF6A55', WAIT = '#6C64A8';
  const root = document.getElementById('app');

  let S = null; // snapshot from Rust
  let G = { received: false, legacy: false, items: [] };
  const U = { tab: 'sync', fstat: '', showAll: false, tour: null, trect: null, tch: 0, keyWait: false,
    drag: null, dragPos: null, fold: { full: true }, bumped: {}, order: null, prevHave: {}, snap: null, advTimer: null, scrolledFor: null, tourKey: '' };

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
  const ico20 = (inner) => '<svg width="20" height="20" viewBox="0 0 20 20" aria-hidden="true">' + inner + '</svg>';
  const ICO_OV = ico20('<rect x="2.5" y="4" width="15" height="10.5" rx="2" fill="none" stroke="currentColor" stroke-width="1.6"></rect><path d="M7 17.5h6M6 8h2.5M6 10.5h4" fill="none" stroke="currentColor" stroke-width="1.6" stroke-linecap="round"></path>');
  const SYNC_ICO = (n) => '<svg width="' + n + '" height="' + n + '" viewBox="0 0 20 20" aria-hidden="true"><path d="' + SYNC_PATH + '" fill="none" stroke="currentColor" stroke-width="1.6" stroke-linecap="round" stroke-linejoin="round"></path></svg>';
  const GAME_ICO = ico20('<rect x="2" y="5" width="16" height="10" rx="4" fill="none" stroke="currentColor" stroke-width="1.6"></rect><path d="M6 8.5v3M4.5 10h3" stroke="currentColor" stroke-width="1.6" stroke-linecap="round"></path><circle cx="13" cy="9" r="1" fill="currentColor"></circle><circle cx="15" cy="11" r="1" fill="currentColor"></circle>');
  const CALC_ICO = ico20('<rect x="3" y="3" width="14" height="14" rx="2.5" fill="none" stroke="currentColor" stroke-width="1.6"></rect><path d="M6 7h8M6 10.5h3M11 10.5h3M6 13.5h3M11 13.5h3" stroke="currentColor" stroke-width="1.6" stroke-linecap="round"></path>');
  const ARROW_R = '<svg width="18" height="18" viewBox="0 0 18 18" aria-hidden="true" style="flex: none; align-self: center; color: #6C64A8"><path d="M4 9h10M10 5l4 4-4 4" fill="none" stroke="currentColor" stroke-width="1.6" stroke-linecap="round" stroke-linejoin="round"></path></svg>';
  const EYE_ICO = ico20('<path d="M2 10s3-5.5 8-5.5S18 10 18 10s-3 5.5-8 5.5S2 10 2 10z" fill="none" stroke="currentColor" stroke-width="1.6"></path><circle cx="10" cy="10" r="2.4" fill="none" stroke="currentColor" stroke-width="1.6"></circle>');
  const HOME_ICO = ico20('<path d="M3.5 9.5L10 4l6.5 5.5V16h-13z" fill="none" stroke="currentColor" stroke-width="1.6" stroke-linejoin="round"></path><path d="M8 16v-4h4v4" fill="none" stroke="currentColor" stroke-width="1.6"></path>');
  const SHIELD_ICO = ico20('<path d="M10 3l6 2.5v4.2c0 3.6-2.6 6.3-6 7.3-3.4-1-6-3.7-6-7.3V5.5z" fill="none" stroke="currentColor" stroke-width="1.6" stroke-linejoin="round"></path><path d="M7.3 10l1.9 1.9 3.6-3.8" fill="none" stroke="currentColor" stroke-width="1.6" stroke-linecap="round" stroke-linejoin="round"></path>');
  const GLOBE_ICO = ico20('<circle cx="10" cy="10" r="7" fill="none" stroke="currentColor" stroke-width="1.6"></circle><path d="M3 10h14M10 3c2 2 2.8 4.4 2.8 7S12 15 10 17c-2-2-2.8-4.4-2.8-7S8 5 10 3z" fill="none" stroke="currentColor" stroke-width="1.4"></path>');
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

  // ---------- window size (boards Adapt1–5, Wide1–5 and «Стало 3») ----------
  // Width: xs < 360 ≤ s < 560 ≤ m < 720 ≤ wide (two columns). Height: hs < 480 ≤ hm < 600 ≤ hl.
  function lay() {
    const W = window.innerWidth, H = window.innerHeight;
    const w = W < 360 ? 'xs' : W < 560 ? 's' : W < 720 ? 'm' : 'wide';
    return { W, H, w, h: H < 480 ? 'hs' : H < 600 ? 'hm' : 'hl', wide: w === 'wide', hc: H < 600 ? 'low' : H < 740 ? 'mid' : 'tall',
      plain: Math.abs(W - 400) <= 1 && Math.abs(H - 680) <= 1,
      // Small mode (variant A): its own interface in a low window, or a narrow one that is not tall.
      // Narrow and tall (e.g. 340 × 820) keeps the usual one-column interface.
      small: H < 480 || (W < 360 && H < 600) };
  }
  let L = lay();
  const padL = () => (L.w === 'xs' ? 14 : 20), padR = () => (L.w === 'xs' ? 6 : 10);
  // Content width of the one-column layout (the scroller keeps a 10 px gutter for its bar).
  const innerW = () => Math.min(520, L.W - padL() - padR() - 10);
  const titleH = () => (L.small ? 36 : L.h === 'hs' ? 40 : 48), navH = () => (L.small ? 40 : L.h === 'hl' ? 60 : 48);
  const capW = (html) => (L.w === 'm' ? '<div style="max-width: 520px; margin: 0 auto">' + html + '</div>' : html);
  function scroller(body, pl, pr, extra) {
    return '<div class="cb-scroll" style="--pl: ' + pl + 'px; --pr: ' + pr + 'px; padding: 0 ' + pr + 'px 0 ' + pl + 'px;' + (extra || '') + '">' + body + '</div>';
  }

  // ---------- main window ----------
  function titlebar() {
    const closeLabel = S.closeQuits ? t('quitApp') : t('closeTray');
    const xs = L.w === 'xs' || L.small;
    return '<div data-drag="1" style="flex: none; height: ' + titleH() + 'px; display: flex; align-items: center; gap: ' + (L.small ? 6 : 10) + 'px; padding: 0 ' + (xs ? 4 : 8) + 'px 0 ' + (L.small ? 12 : xs ? 14 : 18) + 'px; border-bottom: 1px solid #2A2550">' +
      (L.small ? ARC(22, 15) + pillSmall() : ARC(26, 18)) +
      (xs ? '' : '<div class="cb-u" style="font-weight: 700; font-size: 14px; letter-spacing: -0.01em">StashSync</div>') +
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
      '<span data-tour="winbtns" style="display: flex; gap: ' + (xs ? 4 : 10) + 'px">' +
      '<button class="cb-wbtn" aria-label="' + esc(t('minimize')) + '" title="' + esc(t('minimize')) + '" data-act="minimize"><svg width="14" height="14" viewBox="0 0 14 14" aria-hidden="true"><path d="M3 7h8" stroke="currentColor" stroke-width="1.6" stroke-linecap="round"></path></svg></button>' +
      '<button class="cb-wbtn cb-wclose" aria-label="' + esc(closeLabel) + '" title="' + esc(closeLabel) + '" data-act="close"><svg width="14" height="14" viewBox="0 0 14 14" aria-hidden="true"><path d="M3.5 3.5l7 7M10.5 3.5l-7 7" stroke="currentColor" stroke-width="1.6" stroke-linecap="round"></path></svg></button>' +
      '</span></div>';
  }

  function routeCard() {
    const s = S, small = L.h !== 'hl' && !L.wide, tight = L.h === 'hs' && !L.wide;
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
    const d = small ? 32 : 40;
    const rows = defs.map((def, i) => {
      const isDone = i < 3 ? (i < setup) : s.site, isBusy = i === 2 && s.signing, isWarn = s.gameOn && i === 1;
      const cur = !isDone && !isBusy && !isWarn && i === setup;
      const isNum = !isDone && !isBusy && !isWarn;
      const bg = isDone ? '#1E2D36' : isWarn ? '#32282E' : cur ? '#1F2A4A' : '#1E1A3A';
      const ring = isDone ? 'rgba(70,199,106,.5)' : isWarn ? WARN : cur || isBusy ? '#45D3E6' : '#3A3466';
      const fg = isDone ? OK : isWarn ? WARN : cur || isBusy ? '#45D3E6' : WAIT;
      const titleColor = isDone || cur || isWarn || isBusy ? '#F2EEE6' : '#A9A3C7';
      const textColor = isWarn ? '#E9DFC4' : i === 2 && errText && !isBusy ? '#FFB3A6' : '#A9A3C7';
      const hasAction = isWarn || (cur && i === 2 && !s.signing);
      // In a very low window finished steps drop their second line, so «Sign in» stays in sight.
      const sub = tight && isDone ? '' : '<div style="font-size: ' + (small ? 12 : 13) + 'px; color: ' + textColor + '; margin-top: ' + (small ? 2 : 3) + 'px; line-height: 1.45"' + (i === 2 && errText ? ' title="' + esc(s.loginError) + '"' : '') + '>' + esc(def[1]) + '</div>';
      return '<div data-key="st' + i + '" style="position: relative; display: flex; gap: ' + (small ? 12 : 14) + 'px; padding: ' + (small ? (tight && isDone ? 4 : 7) : 12) + 'px 0; align-items: flex-start">' +
        '<span class="' + (cur ? 'cb-pulse' : '') + '" style="position: relative; z-index: 1; flex: none; width: ' + d + 'px; height: ' + d + 'px; border-radius: 50%; display: flex; align-items: center; justify-content: center; background: ' + bg + '; box-shadow: inset 0 0 0 1.5px ' + ring + '; color: ' + fg + '">' +
        (isDone ? CHECK('cb-check', 'currentColor') : '') +
        (isBusy ? '<svg class="cb-spin" width="20" height="20" viewBox="0 0 20 20" aria-hidden="true"><path d="M10 3a7 7 0 1 1-7 7" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round"></path></svg>' : '') +
        (isNum ? '<span class="cb-tnum" style="font-size: ' + (small ? 13 : 15) + 'px; font-weight: 700">' + (i + 1) + '</span>' : '') +
        (isWarn ? '<svg width="20" height="20" viewBox="0 0 20 20" aria-hidden="true"><path d="M10 6v5M10 14v.1" fill="none" stroke="currentColor" stroke-width="2.2" stroke-linecap="round"></path></svg>' : '') +
        '</span><div style="flex: 1; min-width: 0; padding-top: ' + (small ? 4 : 2) + 'px">' +
        '<div style="font-size: ' + (small ? 14 : 15) + 'px; font-weight: 600; color: ' + titleColor + '">' + esc(def[0]) + '</div>' + sub +
        (hasAction ? '<button class="cb-btn cb-fade" style="height: ' + (small ? 32 : 36) + 'px; margin-top: ' + (small ? 8 : 10) + 'px; font-size: 13px" data-act="' + (isWarn ? 'recheck' : 'login') + '">' + esc(isWarn ? t('rGameClosed') : t('signIn')) + '</button>' : '') +
        '</div></div>';
    }).join('');
    return '<div data-tour="route" style="position: relative; margin-top: ' + (small ? 12 : 18) + 'px; padding: ' + (small ? '4px 12px' : '8px 16px') + '; border-radius: 14px; background: #17142E"><div style="position: relative">' +
      '<div class="cb-line" style="left: ' + (d / 2 - 1) + 'px"><div class="cb-fill" style="height: ' + (Math.min(setup, 3) / 3 * 100) + '%"></div></div>' + rows +
      '</div></div>';
  }

  function routeHead() {
    const small = L.h !== 'hl' && !L.wide;
    return '<div class="cb-u" style="font-weight: 700; font-size: ' + (small ? 16 : 18) + 'px; letter-spacing: -0.02em">' + esc(t('rTitle')) + '</div>' +
      (L.h === 'hs' && !L.wide ? '' : '<div style="font-size: 13px; color: #A9A3C7; margin-top: 6px">' + esc(t('rSub')) + '</div>');
  }

  // What the user gets once signed in; fills the free space under the route.
  function afterBlock(big) {
    const s = S;
    return '<div style="margin-top: ' + (big ? 0 : 14) + 'px; font-size: 12px; font-weight: 700; color: #A9A3C7; letter-spacing: .04em; text-transform: uppercase">' + esc(t('aTitle')) + '</div>' +
      [[SYNC_PATH, t('a1'), t('a1s', s.interval)], ['', t('a2'), t('a2s', s.ov.key)]].map((a) =>
        '<div style="display: flex; gap: 12px; align-items: flex-start; padding: ' + (big ? 10 : 7) + 'px 0">' +
        '<span style="flex: none; width: 36px; height: 36px; border-radius: 10px; background: #262150; display: flex; align-items: center; justify-content: center; color: #45D3E6">' +
        (a[0] ? '<svg width="20" height="20" viewBox="0 0 20 20" aria-hidden="true"><path d="' + a[0] + '" fill="none" stroke="currentColor" stroke-width="1.6" stroke-linecap="round" stroke-linejoin="round"></path></svg>' : ICO_OV) +
        '</span><div><div style="font-size: 14px; font-weight: 600">' + esc(a[1]) + '</div><div style="font-size: 12px; color: #A9A3C7; margin-top: 2px; line-height: 1.45">' + esc(a[2]) + '</div></div></div>').join('');
  }

  function routeHtml() {
    return '<div class="cb-fade" data-key="route" style="padding-top: ' + (L.h === 'hl' ? 22 : 12) + 'px">' + routeHead() + routeCard() + (L.h === 'hl' ? afterBlock(false) : '') + '</div>';
  }

  // Wide first run: steps left; right, how it works and what the overlay will look like (board Wide4).
  function routeWide() {
    const s = S, o = s.ov;
    const lw = L.W < 960 ? 380 : 440;
    const inner = L.W - lw - 52 - 10, ph = L.H - titleH() - navH();
    const nodes = [[GAME_ICO, 'ARC Raiders', t('n1s')], [SYNC_ICO(20), 'StashSync', t('n2s', s.interval)], [CALC_ICO, 'ARC Calc', t('n3s')]];
    const node = (n) => '<div style="flex: 1; min-width: 0; padding: 12px; border-radius: 12px; background: #17142E; box-shadow: inset 0 0 0 1px #2A2550"><span class="cb-pic">' + n[0] + '</span>' +
      '<div style="margin-top: 10px; font-size: 14px; font-weight: 600">' + esc(n[1]) + '</div><div style="font-size: 12px; color: #A9A3C7; margin-top: 2px; line-height: 1.4">' + esc(n[2]) + '</div></div>';
    let chain, chainH;
    if (inner >= 420) { chain = '<div style="display: flex; gap: 6px">' + nodes.map(node).join(ARROW_R) + '</div>'; chainH = 130; }
    else {
      chain = '<div style="display: flex; flex-direction: column; gap: 6px">' + nodes.map((n) => '<div style="display: flex; gap: 12px; align-items: center"><span class="cb-pic">' + n[0] + '</span><div><div style="font-size: 14px; font-weight: 600">' + esc(n[1]) + '</div><div style="font-size: 12px; color: #A9A3C7">' + esc(n[2]) + '</div></div></div>').join('') + '</div>';
      chainH = 138;
    }
    const sw = Math.max(160, Math.floor(Math.min(inner, (ph - chainH - 170) * 16 / 9)));
    const left = '<div class="cb-fade" data-key="route" style="padding-top: 22px">' + routeHead() + routeCard() + '</div>';
    const right = '<div class="cb-fade" data-key="how" style="padding-top: 22px"><div class="cb-lbl">' + esc(t('howTitle')) + '</div><div style="margin-top: 10px">' + chain + '</div>' +
      '<div class="cb-lbl" style="margin-top: 22px">' + esc(t('aTitle')) + '</div><div style="margin-top: 10px">' + sceneZoom(sw) + '</div>' +
      '<div style="margin-top: 8px; font-size: 13px; color: #C9C3E6; line-height: 1.45">' + esc(t('afterText', s.interval, o.key)) + '</div></div>';
    return [left, right, lw];
  }

  // Top-left corner of the screen enlarged, so the overlay plate is readable in an illustration.
  function sceneZoom(sw) {
    const k = 1.6, o = S.ov, kk = sw * k / o.screenW * o.dpi * o.size / 100;
    const items = G.items.length ? G.items.filter((it) => !o.hideDone || it.left > 0).slice(0, 8) : [3, 1, 2, 5].map((n) => ({ left: n }));
    const tile = 58 * kk, f2 = (v) => v.toFixed(2) + 'px';
    const tiles = items.map((it) => '<span style="position: relative; width: ' + f2(tile) + '; height: ' + f2(tile) + '; flex: none; border-radius: ' + f2(8 * kk) + '; background: #15122B; box-shadow: inset 0 0 0 1px rgba(255,255,255,.12); display: flex; align-items: center; justify-content: center">' +
      (it.icon ? '<img src="' + it.icon + '" alt="" style="width: 78%; height: 78%; object-fit: contain">' : '') +
      '<span class="cb-tnum" style="position: absolute; right: 8%; bottom: 4%; font-weight: 700; line-height: 1; font-size: ' + f2(13 * kk) + '; text-shadow: 0 1px 2px #000">' + (it.left > 0 ? it.left : '✓') + '</span></span>').join('');
    return '<div class="cb-scene" style="position: relative; width: ' + sw + 'px; height: ' + Math.round(sw * 9 / 16) + 'px; border-radius: 10px; overflow: hidden; box-shadow: inset 0 0 0 1px #2F2A55; background-size: ' + k * 100 + '% auto; background-position: 0 0">' +
      '<div style="position: absolute; left: ' + f2(10 * k) + '; top: ' + f2(10 * k) + '; display: flex; flex-wrap: wrap; gap: ' + f2(8 * kk) + '; width: ' + f2(tile * 8 + 8 * kk * 7) + '; opacity: ' + o.opacity / 100 + '">' + tiles + '</div></div>';
  }

  function ringHtml(size, ringColor, ringTrack, frac, mode) {
    const h = size / 2, r = h - 3, Cm = 2 * Math.PI * r, dash = Cm * 0.21, icon = size >= 56 ? 22 : size >= 40 ? 18 : 16;
    const svg = (inner, rot) => '<svg width="' + size + '" height="' + size + '" viewBox="0 0 ' + size + ' ' + size + '" aria-hidden="true" style="position: absolute; inset: 0' + (rot ? '; transform: rotate(-90deg)' : '') + '"' + (rot ? '' : ' class="cb-spin"') + '>' + inner + '</svg>';
    const circle = (stroke, extra) => '<circle cx="' + h + '" cy="' + h + '" r="' + r + '" fill="none" stroke="' + stroke + '" stroke-width="3"' + (extra || '') + '></circle>';
    return svg(circle(ringTrack), true) +
      (mode === 'ring' ? svg(circle('#45D3E6', ' class="cb-prog" stroke-linecap="round" style="stroke-dasharray: ' + (Cm * frac).toFixed(1) + ' ' + Cm.toFixed(1) + '"'), true) : '') +
      (mode === 'spin' ? svg([ERR, WARN, OK, '#45D3E6'].map((c, i) => circle(c, ' stroke-linecap="round" stroke-dasharray="' + dash.toFixed(1) + ' ' + (Cm - dash).toFixed(1) + '" stroke-dashoffset="' + (-Cm / 4 * i).toFixed(1) + '"')).join(''), false) : '') +
      (mode === 'done' ? svg(circle(OK, ' class="cb-draw" stroke-linecap="round"'), true) : '') +
      (mode === 'bad' ? '<svg width="' + icon + '" height="' + icon + '" viewBox="0 0 20 20" aria-hidden="true"><path d="M10 5.5v5.5M10 14.3v.1" fill="none" stroke="currentColor" stroke-width="2.2" stroke-linecap="round"></path></svg>'
        : mode === 'done' ? '<svg width="' + icon + '" height="' + icon + '" viewBox="0 0 20 20" aria-hidden="true"><path class="cb-check" d="M5.5 10.5l3 3L14.5 7" fill="none" stroke="#46C76A" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"></path></svg>'
          : '<svg width="' + icon + '" height="' + icon + '" viewBox="0 0 20 20" aria-hidden="true"><path d="' + SYNC_PATH + '" fill="none" stroke="currentColor" stroke-width="1.7" stroke-linecap="round" stroke-linejoin="round"></path></svg>');
  }

  // mode: normal (ring 56, action row) | compact (ring 44, one-line text) | mini (ring 40, tighter).
  function heroTop(mode) {
    const s = S, c = clock();
    const syncing = s.phase === 'syncing', done = s.phase === 'done', idle = s.phase === 'idle';
    const nosite = !s.site, failed = s.failed, bad = idle && (failed || nosite);
    const st = status();
    const showRing = idle && s.auto && !failed && !nosite;
    const ringColor = failed ? ERR : nosite ? WARN : '#45D3E6';
    const ringTrack = !idle ? '#2A2550' : failed ? 'rgba(255,106,85,.7)' : nosite ? 'rgba(245,190,46,.7)' : '#2A2550';
    const headColor = !idle ? '#F2EEE6' : failed ? '#FFB3A6' : nosite ? '#F5D57A' : '#F2EEE6';
    const actLabel = failed ? t('retryNow') : t('updateNow');
    const changes = s.changes == null ? '' : s.changes ? t('changesN', s.changes) : t('noChanges');
    const size = mode === 'normal' ? 56 : mode === 'compact' ? 44 : 40;
    const ring = '<button class="cb-mini" data-act="sync"' + (idle ? '' : ' disabled') + ' aria-label="' + esc(actLabel) + '" title="' + esc(actLabel) + '" style="width: ' + size + 'px; height: ' + size + 'px; color: ' + ringColor + '">' +
      ringHtml(size, ringColor, ringTrack, c.frac, syncing ? 'spin' : done ? 'done' : bad ? 'bad' : showRing ? 'ring' : 'icon') + '</button>';
    const autoTxt = syncing || done ? '' : failed ? (s.auto ? t('retryIn', c.txt) : '') : nosite ? t('sendWhenOpen') : s.auto ? t('autoIn', c.txt) : t('autoOff');
    if (mode !== 'normal') {
      const low = (v) => v.charAt(0).toLowerCase() + v.slice(1);
      const sub = syncing ? t('subReading')
        : done ? (nosite ? t('subNowhere') : t('subSiteUpdated', hhmm(s.lastSync || Date.now())))
        : failed ? (s.auto ? t('retryIn', c.txt) : t('subFailed'))
        : nosite ? t('sendWhenOpen')
        : s.lastSync ? hhmm(s.lastSync) + changes + (s.auto ? ' · ' + low(t('autoIn', c.txt)) : '') : t('subNever');
      return '<div data-tour="hero" style="display: flex; align-items: center; gap: 12px; padding: ' + (mode === 'compact' ? '14px 0 12px' : '10px 0') + '; border-bottom: 1px solid #2A2550">' + ring +
        '<div style="flex: 1; min-width: 0"><div style="font-size: 14px; font-weight: 600; color: ' + headColor + '; white-space: nowrap; overflow: hidden; text-overflow: ellipsis">' + esc(st[1]) + '</div>' +
        '<div class="cb-tnum" title="' + esc(sub) + '" style="font-size: 12px; color: #A9A3C7; margin-top: 2px; white-space: nowrap; overflow: hidden; text-overflow: ellipsis">' + esc(sub) + '</div>' +
        // With an error the ring alone is not enough: the retry stays a visible button.
        (failed && idle ? '<button class="cb-act" style="height: 28px; margin-top: 2px" data-act="sync"><svg width="14" height="14" viewBox="0 0 20 20" aria-hidden="true"><path d="' + SYNC_PATH + '" fill="none" stroke="currentColor" stroke-width="1.8" stroke-linecap="round" stroke-linejoin="round"></path></svg>' + esc(actLabel) + '</button>' : '') +
        '</div></div>';
    }
    const sub = syncing ? t('subReading')
      : done ? (nosite ? t('subNowhere') : t('subSiteUpdated', hhmm(s.lastSync || Date.now())))
      : failed ? (s.buildWait ? t('subFailedBuild') : t('subFailed'))
      : nosite && s.lastSync ? t('subNowhere')
      : s.lastSync ? t('updatedAt', hhmm(s.lastSync)) + changes : t('subNever');
    const chain = [
      ['Steam', s.steamOn ? t('chReady') : t('chNotRunning'), s.steamOn ? OK : WAIT],
      [t('chLogin'), failed ? t('chNoAnswer') : t('chSaved'), failed ? ERR : OK],
      ['ARC Calc', nosite ? t('chClosed') : t('chOpen'), nosite ? WARN : OK]
    ];
    return '<div data-tour="hero" style="display: flex; align-items: flex-start; gap: 14px; padding: 20px 0 16px; border-bottom: 1px solid #2A2550">' + ring +
      '<div style="flex: 1; min-width: 0; padding-top: 2px">' +
      '<div style="font-size: 15px; font-weight: 600; color: ' + headColor + '">' + esc(st[1]) + '</div>' +
      '<div style="font-size: 13px; color: #A9A3C7; margin-top: 3px; line-height: 1.4">' + esc(sub) + '</div>' +
      (failed || nosite ? '<div style="display: flex; flex-wrap: wrap; gap: 4px 12px; margin-top: 6px">' + chain.map((n) => '<span title="' + esc(n[1]) + '" style="display: flex; align-items: center; gap: 6px; font-size: 12px; color: #C9C3E6"><span style="width: 7px; height: 7px; border-radius: 50%; background: ' + n[2] + '"></span>' + esc(n[0]) + '</span>').join('') + '</div>' : '') +
      '<div style="display: flex; flex-wrap: wrap; align-items: center; justify-content: space-between; gap: 0 10px; margin-top: 6px; min-height: 32px">' +
      (idle && !nosite ? '<button class="cb-act" data-act="sync"><svg width="16" height="16" viewBox="0 0 20 20" aria-hidden="true"><path d="' + SYNC_PATH + '" fill="none" stroke="currentColor" stroke-width="1.8" stroke-linecap="round" stroke-linejoin="round"></path></svg>' + esc(actLabel) + '</button>' : '') +
      '<span class="cb-tnum" style="font-size: 12px; color: #A9A3C7; white-space: nowrap">' + esc(autoTxt) + '</span>' +
      '</div></div></div>';
  }

  function noticeHtml() {
    if (S.site || S.phase === 'syncing') return '';
    return '<div class="cb-row" data-key="notice" style="margin-top: 16px; padding: 14px 16px; border-radius: 14px; background: #262150; display: flex; gap: 12px; align-items: flex-start">' +
      '<span style="flex: none; width: 10px; height: 10px; border-radius: 50%; margin-top: 5px; background: ' + WARN + '; box-shadow: 0 0 0 4px rgba(245,190,46,.16)"></span>' +
      '<div style="flex: 1; min-width: 0"><div style="font-size: 14px; font-weight: 600">' + esc(t('nTitle')) + '</div>' +
      '<div style="font-size: 13px; color: #C9C3E6; margin-top: 4px; line-height: 1.45">' + esc(t('nText')) + '</div>' +
      '<button class="cb-ghost" style="margin-top: 10px" data-act="open" data-url="https://arccalc.github.io">' + esc(t('openSite')) + '</button></div></div>';
  }

  // ---------- goals ----------
  function goalData() {
    const rows = goalRows();
    let have = 0, need = 0, uY = 0, uR = 0, left = 0;
    const cnt = { full: 0, partial: 0, none: 0 };
    rows.forEach((r) => {
      cnt[r.st]++; left += r.it.left;
      if (!G.legacy) { have += Math.min(r.it.have, r.it.need); need += r.it.need; }
      if (r.rank === 1) uY += r.it.left; else if (r.rank === 0) uR += r.it.left;
    });
    if (U.fstat && !cnt[U.fstat]) U.fstat = '';
    // Rounded like the board, but never «100%» while something is still missing.
    const pct = !need ? 0 : have >= need ? 100 : Math.min(99, Math.round(have / need * 100));
    return { rows, shown: rows.filter((r) => !U.fstat || r.st === U.fstat), have, uY, uR, left, cnt, pct };
  }
  const statDefs = () => [['full', t('fEnough'), OK], ['partial', t('fPartial'), WARN], ['none', t('fNone'), ERR]];

  function goalsEmptyHtml() {
    return '<div class="cb-row" data-key="nogoals" style="margin-top: 18px; padding: 22px 18px; border-radius: 14px; background: #17142E; display: flex; flex-direction: column; align-items: flex-start; gap: 6px">' +
      '<span style="opacity: .55; display: flex">' + ARC(46, 34) + '</span>' +
      '<div style="margin-top: 8px; font-size: 16px; font-weight: 600">' + esc(t('gEmpty')) + '</div>' +
      '<div style="font-size: 13px; color: #A9A3C7; line-height: 1.45">' + esc(t('gEmptyText')) + '</div>' +
      '<button class="cb-ghost" style="margin-top: 10px" data-act="open" data-url="https://arccalc.github.io">' + esc(t('openSite')) + '</button></div>';
  }
  function barHtml(d, style) {
    return '<div class="cb-bar" aria-hidden="true"' + (style ? ' style="' + style + '"' : '') + '>' + statDefs().map(([id, , c]) => '<span class="cb-bar-seg" style="flex-grow: ' + ({ full: d.have, partial: d.uY, none: d.uR }[id]) + '; background: ' + c + '; opacity: ' + (U.fstat && U.fstat !== id ? 0.3 : 1) + '"></span>').join('') + '</div>';
  }
  // kind: legend (chips in a row) | grid (three equal two-line chips for narrow content).
  function chipsHtml(d, kind) {
    const btn = (id, c, inner, extraCls, style) => '<button data-key="lg-' + id + '" class="cb-lgb' + extraCls + (U.fstat === id ? ' cb-lg-on' : U.fstat ? ' cb-lg-dim' : '') + '" aria-pressed="' + (U.fstat === id) + '"' + (d.cnt[id] ? '' : ' disabled') + ' data-act="filter" data-f="' + id + '"' + (style || '') + '>' + inner + '</button>';
    if (kind === 'grid') {
      return '<div class="cb-legend cb-lg3" role="group" aria-label="' + esc(t('showInList')) + '">' + statDefs().map(([id, label, c]) =>
        btn(id, c, '<span style="display: flex; align-items: center; gap: 6px"><span class="cb-lg" style="background: ' + c + '"></span><b>' + d.cnt[id] + '</b></span><span style="font-size: 12px; white-space: nowrap">' + esc(label) + '</span>', ' cb-lgb3')).join('') + '</div>';
    }
    return '<div class="cb-legend" role="group" aria-label="' + esc(t('showInList')) + '">' + statDefs().map(([id, label, c]) =>
      btn(id, c, '<span class="cb-lg" style="background: ' + c + '"></span>' + esc(label) + ' <b>' + d.cnt[id] + '</b>', '')).join('') + '</div>';
  }
  function staleHtml() {
    return !S.site && S.lastPush ? '<div style="margin-top: 10px; font-size: 12px; color: #A9A3C7">' + esc(t('lastSent', dayTime(S.lastPush))) + '</div>' : '';
  }
  const showAllBtn = (style) => (U.fstat ? '<button class="cb-act cb-fade" style="height: 26px; ' + style + '" data-act="filter" data-f="">' + esc(t('showAll')) + '</button>' : '');

  // chips: legend | grid | none (the wide window has its own filter list).
  function goalsHeadNormal(d, chips) {
    return '<div data-tour="goals-head">' +
      // «Show all» sits on the title row: in the chip row it wrapped and pushed the list down.
      '<div style="display: flex; align-items: center; justify-content: space-between; gap: 8px; min-height: 18px"><span style="font-size: 13px; color: #A9A3C7">' + esc(t('gTitle')) + '</span>' +
      (chips === 'none' ? '' : showAllBtn('margin: -4px -10px -4px 0')) + '</div>' +
      '<div style="display: flex; align-items: flex-end; justify-content: space-between; gap: 12px; margin-top: 4px">' +
      (G.legacy ? '<div style="font-size: 12px; color: #A9A3C7; line-height: 1.4; max-width: 200px">' + esc(t('gLegacy')) + '</div>'
        : '<div style="display: flex; align-items: baseline; gap: 8px"><span class="cb-u" style="font-weight: 800; font-size: 44px; letter-spacing: -0.03em; line-height: 1">' + d.pct + '%</span><span style="font-size: 13px; color: #C9C3E6">' + esc(t('collected')) + '</span></div>') +
      '<div style="text-align: right; font-size: 13px; color: #A9A3C7; line-height: 1.4; white-space: nowrap">' + esc(t('leftToGet')) + '<br><span class="cb-u" style="font-weight: 700; font-size: 18px; color: #F2EEE6">' + esc(t('pcs', d.left)) + '</span></div>' +
      '</div>' + staleHtml() +
      (G.legacy ? '' : chips === 'none' ? barHtml(d) : '<div data-tour="filters">' + barHtml(d) + chipsHtml(d, chips) + '</div>') +
      '</div>';
  }
  // Low windows: % and bar on one line, the label shortened.
  function goalsHeadCompact(d, chips) {
    return '<div data-tour="goals-head" style="margin-top: 12px">' +
      '<div style="display: flex; align-items: center; justify-content: space-between; gap: 8px; min-height: 18px; font-size: 12px; color: #A9A3C7"><span style="white-space: nowrap; overflow: hidden; text-overflow: ellipsis">' + esc(t('gShort')) + '</span>' +
      (U.fstat && chips !== 'none' ? showAllBtn('margin: -4px -10px -4px 0') : '<span style="white-space: nowrap">' + esc(t('leftWord')) + ' <b class="cb-u" style="font-size: 13px; color: #F2EEE6">' + esc(t('pcs', d.left)) + '</b></span>') + '</div>' +
      (G.legacy ? '<div style="margin-top: 4px; font-size: 12px; color: #A9A3C7; line-height: 1.4">' + esc(t('gLegacy')) + '</div>'
        : '<div style="display: flex; align-items: center; gap: 12px; margin-top: 4px"><span class="cb-u cb-tnum" style="font-weight: 800; font-size: 26px; letter-spacing: -0.03em; line-height: 1">' + d.pct + '%</span>' +
          '<div style="flex: 1; min-width: 0">' + barHtml(d, 'height: 8px; margin: 0 2px') + '</div></div>') +
      staleHtml() +
      (G.legacy || chips === 'none' ? '' : '<div data-tour="filters">' + chipsHtml(d, chips) + '</div>') +
      '</div>';
  }
  function goalListHtml(d, n, icon) {
    const list = U.showAll ? d.shown : d.shown.slice(0, n), hidden = d.shown.length - n;
    const items = list.map((r) => {
      const it = r.it;
      const num = G.legacy
        ? '<span style="font-size: 15px; font-weight: 700; color: ' + ERR + '">' + it.left + '</span>'
        : '<span style="font-size: 15px; font-weight: 700; color: ' + r.color + '">' + Math.min(it.have, it.need) + '</span> ' + esc(t('of')) + ' ' + it.need;
      return '<div data-key="g-' + esc(it.name) + '" style="display: grid; grid-template-columns: ' + icon + 'px minmax(0, 1fr) 72px; gap: 12px; align-items: center; padding: 8px 0; border-top: 1px solid #2A2550">' +
        (it.icon ? '<img src="' + it.icon + '" alt="" style="width: ' + icon + 'px; height: ' + icon + 'px; object-fit: contain; border-radius: 10px; background: #262150; padding: 3px">' : '<span style="width: ' + icon + 'px; height: ' + icon + 'px; border-radius: 10px; background: #262150"></span>') +
        '<div style="min-width: 0"><div style="font-size: 14px; white-space: nowrap; overflow: hidden; text-overflow: ellipsis">' + esc(it.label) + '</div>' +
        '<div style="margin-top: 6px; height: 4px; border-radius: 4px; background: #2A2550; overflow: hidden"><div class="cb-seg8" style="height: 4px; width: ' + r.w + '%; background: ' + r.color + '"></div></div></div>' +
        '<span class="' + (U.bumped[it.name] ? 'cb-up ' : '') + 'cb-tnum" style="justify-self: end; font-size: 13px; color: #A9A3C7; white-space: nowrap">' + num + '</span></div>';
    }).join('');
    const more = hidden > 0 ? '<div style="border-top: 1px solid #2A2550; padding-top: 6px"><button class="cb-act cb-act-mute" aria-expanded="' + U.showAll + '" data-act="more">' + esc(U.showAll ? t('showLess') : t('showMore', hidden)) +
      '<svg class="' + (U.showAll ? 'cb-chev cb-chev-on' : 'cb-chev') + '" width="14" height="14" viewBox="0 0 14 14" aria-hidden="true"><path d="M3.5 5.5L7 9l3.5-3.5" fill="none" stroke="currentColor" stroke-width="1.6" stroke-linecap="round" stroke-linejoin="round"></path></svg></button></div>' : '';
    return [items, more];
  }

  // Narrow stash: status and the goal summary stay put, only the resource list scrolls.
  // Returns [fixed, scrolling].
  function stashHtml() {
    const mode = L.h === 'hl' ? 'normal' : L.h === 'hm' ? 'compact' : 'mini';
    const gm = L.h === 'hl' ? 'normal' : 'compact';
    if (!G.received || !G.items.length) {
      return ['', '<div class="cb-fade" data-key="hero">' + heroTop(mode) + noticeHtml() + (G.received ? goalsEmptyHtml() : '') + '<div style="height: 24px"></div></div>'];
    }
    if (L.w === 'xs') return stashGroups();
    const d = goalData();
    const chips = L.h === 'hs' ? 'none' : innerW() < 340 ? 'grid' : 'legend';
    const items = goalListHtml(d, d.shown.length, L.w === 'xs' ? 32 : 36)[0];
    // Below 760 px the «open the site» card would leave no room for the list; it scrolls with it.
    const tall = L.H >= 760;
    const head = '<div class="cb-fade" data-key="hero" style="padding-bottom: ' + (gm === 'normal' ? 12 : 10) + 'px">' + heroTop(mode) + (tall ? noticeHtml() : '') +
      '<div data-key="goals" style="margin-top: ' + (gm === 'normal' ? 18 : 0) + 'px">' + (gm === 'normal' ? goalsHeadNormal(d, chips) : goalsHeadCompact(d, chips)) + '</div></div>';
    const list = (tall ? '' : '<div style="padding-bottom: 12px">' + noticeHtml() + '</div>') + '<div data-tour="list" data-key="list">' + items + '</div><div style="height: 16px"></div>';
    return ['<div style="padding: 0 ' + (padR() + 10) + 'px 0 ' + padL() + 'px">' + capW(head) + '</div>', list];
  }

  // Narrow and tall window (variant A «Группы», board Narrow1): sections by status instead of filter chips;
  // a press on a section header folds it, «Enough» starts folded. Returns [fixed, scrolling].
  function stashGroups() {
    const d = goalData(), tall = L.H >= 760;
    const head = '<div data-tour="goals-head" style="margin-top: 12px">' +
      '<div style="display: flex; align-items: baseline; justify-content: space-between; gap: 8px">' +
      (G.legacy ? '<span style="font-size: 12px; color: #A9A3C7; line-height: 1.4">' + esc(t('gLegacy')) + '</span>'
        : '<span style="display: flex; align-items: baseline; gap: 6px"><span class="cb-u cb-tnum" style="font-weight: 800; font-size: 30px; letter-spacing: -0.03em; line-height: 1">' + d.pct + '%</span><span style="font-size: 12px; color: #C9C3E6">' + esc(t('collected')) + '</span></span>') +
      '<span style="font-size: 12px; color: #A9A3C7; white-space: nowrap">' + esc(t('leftWord')) + ' <b class="cb-u" style="font-size: 14px; color: #F2EEE6">' + esc(t('pcs', d.left)) + '</b></span></div>' +
      (G.legacy ? '' : barHtml(d, 'height: 8px; margin: 8px 2px 0')) + staleHtml() + '</div>';
    const fixed = '<div class="cb-fade" data-key="hero" style="padding-bottom: 6px">' + heroTop('compact') + (tall ? noticeHtml() : '') + head + '</div>';
    const row = (r) => {
      const it = r.it;
      const num = G.legacy ? '<b style="color: ' + ERR + '">' + it.left + '</b>' : '<b style="color: ' + r.color + '">' + Math.min(it.have, it.need) + '</b> ' + esc(t('of')) + ' ' + it.need;
      return '<div class="cb-nrow" data-key="g-' + esc(it.name) + '">' +
        (it.icon ? '<img src="' + it.icon + '" alt="">' : '<span class="cb-nimg"></span>') +
        '<div style="min-width: 0"><div class="cb-nname">' + esc(it.label) + '</div>' +
        (G.legacy ? '' : '<div class="cb-ntrack"><div class="cb-seg8" style="height: 3px; width: ' + r.w + '%; background: ' + r.color + '"></div></div>') + '</div>' +
        '<span class="' + (U.bumped[it.name] ? 'cb-up ' : '') + 'cb-tnum cb-nnum">' + num + '</span></div>';
    };
    const groups = statDefs().slice().reverse().map(([k, label, c]) => {
      const rows = d.rows.filter((r) => r.st === k);
      if (!rows.length) return '';
      const folded = !!U.fold[k];
      return '<button class="cb-ngh" data-key="gh-' + k + '" data-act="fold" data-g="' + k + '" aria-expanded="' + !folded + '"><span class="cb-lg" style="background: ' + c + '"></span>' + esc(label) + ' <b class="cb-tnum">' + rows.length + '</b>' +
        '<svg class="cb-chev' + (folded ? ' cb-fold' : '') + '" width="14" height="14" viewBox="0 0 14 14" aria-hidden="true"><path d="M3.5 5.5L7 9l3.5-3.5" fill="none" stroke="currentColor" stroke-width="1.6" stroke-linecap="round" stroke-linejoin="round"></path></svg></button>' +
        (folded ? '' : rows.map(row).join(''));
    }).join('');
    const list = (tall ? '' : '<div style="padding-bottom: 8px">' + noticeHtml() + '</div>') + '<div data-tour="list" data-key="groups">' + groups + '</div><div style="height: 16px"></div>';
    return ['<div style="padding: 0 ' + (padR() + 10) + 'px 0 ' + padL() + 'px">' + fixed + '</div>', list];
  }

  // ---------- wide stash (board Wide1, Wide5) ----------
  function ovCardHtml(thumb) {
    const o = S.ov, sub = t(o.shown ? 'ovShownKey' : 'ovHiddenKey', o.key);
    const text = '<div style="flex: 1; min-width: 0"><div class="cb-clab">' + esc(t('ovTitle')) + '</div><div class="cb-csub">' + esc(sub) + '</div></div>';
    if (thumb) {
      const g = ovGeo(128);
      return '<div class="cb-card" data-key="ovcard-t" style="margin-top: 16px; padding: 12px; display: flex; gap: 14px; align-items: center">' +
        '<div class="cb-scene" style="position: relative; flex: none; width: 128px; height: ' + g.H + 'px; border-radius: 8px; overflow: hidden">' + ovBox(g, false) + '</div>' + text + sw(o.shown, 'ovShown', t('ovTitle')) + '</div>';
    }
    return '<div class="cb-card" data-key="ovcard" style="margin-top: 16px; padding: 6px 16px"><div style="display: flex; align-items: center; gap: 10px; min-height: 44px">' +
      '<span style="color: #A9A3C7; display: flex">' + ICO_OV + '</span>' + text + sw(o.shown, 'ovShown', t('ovTitle')) + '</div></div>';
  }
  function filterListHtml(d) {
    const items = [['', t('allResBtn'), null, d.rows.length]].concat(statDefs().map(([id, label, c]) => [id, label, c, d.cnt[id]]));
    return '<div data-tour="filters" style="margin-top: 16px"><div class="cb-lbl" style="margin-bottom: 8px">' + esc(t('showInList')) + '</div><div role="group" style="display: flex; flex-direction: column; gap: 4px">' +
      items.map(([id, label, c, n]) => '<button class="cb-fl" data-key="fl-' + (id || 'all') + '" data-on="' + (U.fstat === id) + '" aria-pressed="' + (U.fstat === id) + '" data-act="filter" data-f="' + id + '"' + (n || !id ? '' : ' disabled') + '>' +
        (c ? '<span class="cb-lg" style="background: ' + c + '"></span>' : '<span style="width: 16px; flex: none"></span>') + esc(label) + '<b class="cb-tnum">' + n + '</b></button>').join('') + '</div></div>';
  }
  function tilesHtml(d) {
    const big = L.W >= 1000, s = big ? 56 : 44;
    const title = U.fstat ? statDefs().find((x) => x[0] === U.fstat)[1] + ' · ' + d.shown.length : t('allRes', d.rows.length);
    const tiles = d.shown.map((r) => {
      const it = r.it;
      const num = G.legacy ? '<b style="color: ' + ERR + '; font-size: ' + (big ? 18 : 15) + 'px">' + it.left + '</b>'
        : '<b style="color: ' + r.color + '; font-size: ' + (big ? 18 : 15) + 'px">' + Math.min(it.have, it.need) + '</b> ' + esc(t('of')) + ' ' + it.need;
      return '<div class="cb-tile" data-key="t-' + esc(it.name) + '"><div style="display: flex; align-items: flex-start; gap: 10px">' +
        (it.icon ? '<img src="' + it.icon + '" alt="" style="width: ' + s + 'px; height: ' + s + 'px; object-fit: contain; border-radius: 10px; background: #262150; padding: 4px">' : '<span style="width: ' + s + 'px; height: ' + s + 'px; border-radius: 10px; background: #262150"></span>') +
        '<span class="' + (U.bumped[it.name] ? 'cb-up ' : '') + 'cb-tnum" style="margin-left: auto; font-size: 13px; color: #A9A3C7; white-space: nowrap">' + num + '</span></div>' +
        '<div class="cb-tname">' + esc(it.label) + '</div>' +
        '<div style="margin-top: auto; height: 4px; border-radius: 4px; background: #2A2550; overflow: hidden"><div class="cb-seg8" style="height: 4px; width: ' + r.w + '%; background: ' + r.color + '"></div></div></div>';
    }).join('');
    return '<div class="cb-fade" data-key="tiles" style="padding-top: 20px"><div style="display: flex; align-items: baseline; justify-content: space-between; gap: 8px; margin-bottom: 12px"><span style="font-size: 15px; font-weight: 600">' + esc(title) + '</span>' +
      '<span style="font-size: 12px; color: #A9A3C7">' + esc(t('missingFirst')) + '</span></div>' +
      '<div data-tour="list" style="display: grid; grid-template-columns: repeat(auto-fill, minmax(' + (big ? 190 : 150) + 'px, 1fr)); gap: 10px; padding-bottom: 20px">' + tiles + '</div></div>';
  }
  // No goals yet: pale placeholder tiles and, once the site has answered, what to do.
  function emptyTilesHtml() {
    const ghost = '<div class="cb-tile" style="height: 112px"><div style="display: flex"><span style="width: 44px; height: 44px; border-radius: 10px; background: #262150"></span><span style="margin-left: auto; width: 40px; height: 12px; border-radius: 4px; background: #262150"></span></div>' +
      '<span style="width: 70%; height: 10px; border-radius: 4px; background: #262150"></span><div style="margin-top: auto; height: 4px; border-radius: 4px; background: #2A2550"></div></div>';
    const steps = [t('e1'), t('e2'), t('e3')].map((x, i) => '<div style="display: flex; gap: 10px; align-items: baseline; margin-top: 8px"><span class="cb-tnum" style="flex: none; width: 20px; height: 20px; border-radius: 50%; background: #262150; color: #C9C3E6; font-size: 12px; font-weight: 700; display: inline-flex; align-items: center; justify-content: center">' + (i + 1) + '</span><span style="font-size: 13px; color: #C9C3E6; line-height: 1.45">' + esc(x) + '</span></div>').join('');
    return '<div data-key="tiles-empty" style="padding-top: 20px"><div style="display: grid; grid-template-columns: repeat(auto-fill, minmax(150px, 1fr)); gap: 10px; opacity: .3">' + new Array(12).join(ghost) + ghost + '</div>' +
      (G.received ? '<div class="cb-fade" style="position: absolute; inset: 0; display: flex; align-items: center; justify-content: center; background: radial-gradient(closest-side, rgba(30,26,58,.9), rgba(30,26,58,.2))">' +
        '<div style="width: 360px; max-width: calc(100% - 32px); padding: 22px 22px 20px; border-radius: 14px; background: #1E1A3A; box-shadow: 0 0 0 1px #2F2A55, 0 20px 50px rgba(0,0,0,.45)"><span style="opacity: .7; display: flex">' + ARC(46, 34) + '</span>' +
        '<div style="margin-top: 10px; font-size: 16px; font-weight: 600">' + esc(t('gEmpty')) + '</div>' + steps +
        '<button class="cb-btn" style="height: 36px; margin-top: 16px; font-size: 13px" data-act="open" data-url="https://arccalc.github.io">' + esc(t('openSite')) + '</button></div></div>' : '') + '</div>';
  }
  function stashWide() {
    const low = L.hc === 'low', lw = L.W < 960 ? 340 : 380;
    let left = heroTop(low ? 'compact' : 'normal') + noticeHtml(), right;
    if (!G.received || !G.items.length) {
      left += ovCardHtml(L.hc === 'tall');
      right = emptyTilesHtml();
    } else {
      const d = goalData();
      if (low) left += goalsHeadCompact(d, 'grid') + ovCardHtml(false);
      else if (S.site) left += '<div style="margin-top: 18px">' + goalsHeadNormal(d, 'none') + '</div>' + filterListHtml(d) + ovCardHtml(L.hc === 'tall');
      // The notice card already explains the problem; the filters shrink to three buttons.
      else left += '<div style="margin-top: 18px">' + goalsHeadNormal(d, 'grid') + '</div>';
      right = tilesHtml(d);
    }
    return [left + '<div style="height: 20px"></div>', right, lw];
  }

  // ---------- overlay ----------
  // Overlay preview at true scale: the primary screen drawn W px wide, tiles from overlay_win.rs (TILE 58, GAP 8, 8 per line, list 280 × 28).
  function ovGeo(Wp) {
    const o = S.ov, W = Wp || U.sceneW || 360, H = Math.round(W * o.screenH / o.screenW);
    const kk = W / o.screenW * o.dpi * o.size / 100;
    const items = G.items.filter((it) => !o.hideDone || it.left > 0);
    const n = items.length, tile = 58 * kk, gap = 8 * kk, per = 8;
    let bw, bh;
    if (o.list) { bw = 280 * kk; bh = 28 * kk * n; } else {
      const cols = o.vertical ? Math.ceil(n / per) : Math.min(n, per), rows = o.vertical ? Math.min(n, per) : Math.ceil(n / per);
      bw = cols * tile + Math.max(0, cols - 1) * gap; bh = rows * tile + Math.max(0, rows - 1) * gap;
    }
    let x, y;
    if (U.dragPos && !Wp) { x = U.dragPos.x; y = U.dragPos.y; }
    else if (o.corner) {
      const m = 6 * W / 360;
      x = o.corner[1] === 'l' ? m : W - bw - m; y = o.corner[0] === 't' ? m : H - bh - m;
    } else { x = o.x * W / o.screenW; y = o.y * W / o.screenW; }
    return { W, H, kk, tile, gap, per, items, bw, bh, x: Math.max(0, x), y: Math.max(0, y) };
  }
  function ovTilesHtml(items, kk, tilePx, animate) {
    const o = S.ov, f2 = (v) => v.toFixed(2) + 'px';
    return items.map((it, i) => {
      const left = it.left > 0 ? it.left : '✓', cls = !animate ? '' : U.bumped[it.name] ? 'cb-bump' : 'cb-row';
      const isz = o.list ? f2(22 * kk) : '78%';
      const icon = it.icon ? '<img src="' + it.icon + '" alt="" draggable="false" style="width: ' + isz + '; height: ' + isz + '; object-fit: contain">' : '<span style="width: ' + isz + '"></span>';
      if (o.list) {
        return '<span data-key="ov-' + esc(it.name) + '" class="' + cls + '" style="display: flex; align-items: center; gap: ' + f2(6 * kk) + '; height: ' + f2(28 * kk) + '; padding: 0 ' + f2(6 * kk) + '; font-size: ' + f2(Math.max(4, 13 * kk)) + '; color: #F2EEE6; text-shadow: 0 1px 2px #000; animation-delay: ' + (i * 0.04) + 's">' +
          icon + '<span style="flex: 1; white-space: nowrap; overflow: hidden">' + esc(it.label) + '</span><span class="cb-tnum" style="font-weight: 700">' + left + '</span></span>';
      }
      return '<span data-key="ov-' + esc(it.name) + '" class="' + cls + '" style="position: relative; width: ' + f2(tilePx) + '; height: ' + f2(tilePx) + '; flex: none; border-radius: ' + f2(8 * kk) + '; background: #15122B; box-shadow: inset 0 0 0 1px rgba(255,255,255,.12); display: flex; align-items: center; justify-content: center; animation-delay: ' + (i * 0.04) + 's">' +
        icon + '<span class="cb-tnum" style="position: absolute; right: 8%; bottom: 4%; font-weight: 700; line-height: 1; font-size: ' + f2(Math.max(4, 13 * kk)) + '; text-shadow: 0 1px 2px #000">' + left + '</span></span>';
    }).join('');
  }
  function ovBox(g, live) {
    const o = S.ov, f2 = (v) => v.toFixed(2) + 'px';
    const box = 'left: ' + g.x.toFixed(1) + 'px; top: ' + g.y.toFixed(1) + 'px; opacity: ' + o.opacity / 100 + '; display: flex; gap: ' + f2(g.gap) + '; ' +
      (o.list ? 'flex-direction: column; gap: 0; width: ' + f2(280 * g.kk) + '; border-radius: ' + f2(6 * g.kk) + '; background: #15122B;'
        : o.vertical ? 'flex-direction: column; flex-wrap: wrap; height: ' + f2(g.tile * g.per + g.gap * (g.per - 1)) + ';' : 'flex-wrap: wrap; width: ' + f2(g.tile * g.per + g.gap * (g.per - 1)) + ';');
    return '<div class="' + (live ? (U.drag ? 'cb-ov' : 'cb-ov cb-move') : '') + '"' + (live ? ' data-ovbox="1"' : '') + ' style="position: absolute; ' + box + (live ? '' : ' pointer-events: none;') + '">' + ovTilesHtml(g.items, g.kk, g.tile, live) + '</div>';
  }
  function ovTitleRow() {
    const o = S.ov, small = L.w === 'xs' || L.h === 'hs';
    return '<div style="display: flex; align-items: center; gap: 12px"><div style="flex: 1; min-width: 0">' +
      '<div class="cb-u" style="font-weight: 700; font-size: ' + (small ? 16 : 18) + 'px; letter-spacing: -0.02em' + (L.w === 'xs' ? '' : '; white-space: nowrap; overflow: hidden; text-overflow: ellipsis') + '">' + esc(t('ovTitle')) + '</div>' +
      (L.h === 'hl' ? '<div style="font-size: 13px; color: #A9A3C7; margin-top: 4px; line-height: 1.4">' + esc(t('ovSub')) + '</div>' : '') + '</div>' +
      '<button class="cb-ghost" aria-pressed="' + o.shown + '" data-act="ovShown">' + esc(o.shown ? t('hide') : t('show')) + ' <span style="font-size: 12px; font-weight: 700; color: #A9A3C7">' + esc(o.key) + '</span></button></div>';
  }
  function sceneHtml(cw) {
    const o = S.ov, g = ovGeo(cw);
    U.sceneW = cw;
    return '<div class="cb-scene" data-tour="scene" style="position: relative; width: ' + cw + 'px; height: ' + g.H + 'px; border-radius: 10px; overflow: hidden; box-shadow: inset 0 0 0 1px #2F2A55">' +
      (g.items.length ? ovBox(ovGeo(), true)
        : '<div style="position: absolute; inset: 0; display: flex; align-items: center; justify-content: center; font-size: 13px; color: #F2EEE6; text-shadow: 0 1px 3px #000">' + esc(t('gEmpty')) + '</div>') +
      '</div>' +
      '<div style="margin-top: 8px; font-size: 12px; color: #A9A3C7; line-height: 1.45">' + esc(t('ovHint', o.screenW, o.screenH)) + '</div>';
  }
  // stack: labels above the segmented buttons (narrowest window).
  function ovSettingsHtml(stack, mt) {
    const o = S.ov;
    const sizeP = (o.size - 50) / 110 * 100, opP = (o.opacity - 20) / 80 * 100;
    const corners = [['tl', t('cTL'), 'left: 4px; top: 4px'], ['tr', t('cTR'), 'right: 4px; top: 4px'], ['bl', t('cBL'), 'left: 4px; bottom: 4px'], ['br', t('cBR'), 'right: 4px; bottom: 4px']];
    const seg = (on, act, label, disabled) => '<button data-on="' + on + '" data-act="' + act + '"' + (disabled ? ' disabled' : '') + '>' + esc(label) + '</button>';
    const segRow = (label, a, b) => (stack
      ? '<div class="cb-cdiv" style="padding: 12px 0"><div class="cb-clab" style="margin-bottom: 8px">' + esc(label) + '</div><div class="cb-seg">' + a + b + '</div></div>'
      : '<div class="cb-crow cb-cdiv"><span class="cb-clab">' + esc(label) + '</span><div class="cb-seg" style="width: 200px">' + a + b + '</div></div>');
    return '<div class="cb-fade" data-key="overlay-set">' +
      '<div data-tour="ov-look"><div class="cb-card" style="margin-top: ' + mt + 'px">' +
      '<div class="cb-crow"><span class="cb-clab">' + esc(t('corner')) + '</span><div style="display: flex; gap: 6px">' +
      corners.map((c) => '<button class="cb-corner" data-on="' + (o.corner === c[0]) + '" aria-label="' + esc(c[1]) + '" title="' + esc(c[1]) + '" data-act="ovCorner" data-c="' + c[0] + '"><span style="' + c[2] + '"></span></button>').join('') + '</div></div>' +
      segRow(t('style'), seg(!o.list, 'ovTiles', t('tiles')), seg(o.list, 'ovListMode', t('list'))) +
      segRow(t('row'), seg(!o.vertical, 'ovAcross', t('across'), o.list), seg(o.vertical, 'ovDown', t('down'), o.list)) +
      '</div><div class="cb-card">' +
      '<div class="cb-cslide"><div class="cb-cline"><label for="cb-size" class="cb-clab">' + esc(t('size')) + '</label><span class="cb-tnum" style="font-size: 13px; font-weight: 600; color: #C9C3E6">' + o.size + '%</span></div><input id="cb-size" class="cb-range" type="range" min="50" max="160" step="5" value="' + o.size + '" data-input="ovSize" style="--p: ' + sizeP + '%"></div>' +
      '<div class="cb-cslide cb-cdiv"><div class="cb-cline"><label for="cb-op" class="cb-clab">' + esc(t('opacity')) + '</label><span class="cb-tnum" style="font-size: 13px; font-weight: 600; color: #C9C3E6">' + o.opacity + '%</span></div><input id="cb-op" class="cb-range" type="range" min="20" max="100" step="1" value="' + o.opacity + '" data-input="ovOpacity" style="--p: ' + opP + '%"></div>' +
      '</div></div>' +
      '<div class="cb-card" data-tour="ov-key">' +
      '<div class="cb-crow"><div style="min-width: 0"><div class="cb-clab">' + esc(t('key')) + '</div><div class="cb-csub">' + esc(t('keySub')) + '</div></div><button class="cb-key" data-wait="' + U.keyWait + '" aria-label="' + esc(t('key') + ': ' + o.key) + '" data-act="askKey">' + esc(U.keyWait ? '…' : o.key) + '</button></div>' +
      '<div class="cb-crow cb-cdiv"><div style="min-width: 0"><div class="cb-clab">' + esc(t('hideDone')) + '</div><div class="cb-csub">' + esc(t('hideDoneSub')) + '</div></div>' + sw(o.hideDone, 'ovHideDone', t('hideDone')) + '</div>' +
      '</div>' +
      '<button class="cb-ghost" data-act="ovReset" style="margin-top: 16px">' + esc(t('reset')) + '</button><div style="height: 24px"></div>' +
      '</div>';
  }
  // Narrow: the preview stays put while there is room for it, otherwise it scrolls with the settings.
  function overlayHtml() {
    const o = S.ov, cw = innerW();
    const top = '<div class="cb-fade" data-key="overlay" style="padding-top: ' + (L.h === 'hl' ? 20 : 14) + 'px">' + ovTitleRow() + '<div style="margin-top: 14px">' + sceneHtml(cw) + '</div></div>';
    const fixed = L.h === 'hl' && cw * o.screenH / o.screenW <= L.H * 0.4;
    if (fixed) return ['<div style="padding: 0 ' + (padR() + 10) + 'px 0 ' + padL() + 'px">' + capW(top) + '</div>', ovSettingsHtml(L.W < 360, 16)];
    return ['', top + ovSettingsHtml(L.W < 360, 16)];
  }
  // The plate as it will be in the game: real pixels at the current «Size».
  function realStrip(maxW) {
    const o = S.ov, items = G.items.filter((it) => !o.hideDone || it.left > 0);
    if (!items.length) return '';
    const kk = o.size / 100, tile = 58 * kk, gap = 8 * kk;
    const full = o.list ? 280 * kk : Math.min(items.length, 8) * (tile + gap) - gap;
    const shown = o.list ? items.slice(0, 3) : items.slice(0, 8);
    return '<div style="margin-top: 16px"><div style="display: flex; align-items: baseline; justify-content: space-between; gap: 8px; margin-bottom: 8px">' +
      '<span class="cb-lbl">' + esc(t('realSize')) + '</span><span class="cb-tnum" style="font-size: 12px; color: #A9A3C7; white-space: nowrap">' + esc(t('sizePct', o.size)) + '</span></div>' +
      '<div style="position: relative; overflow: hidden; display: flex; ' + (o.list ? 'flex-direction: column; width: ' + (280 * kk + 20).toFixed(1) + 'px; max-width: 100%' : 'gap: ' + gap.toFixed(2) + 'px') + '; padding: 10px; border-radius: 10px; background: #100E22; opacity: ' + o.opacity / 100 + '">' +
      ovTilesHtml(shown, kk, tile, false) +
      (full + 20 > maxW ? '<span style="position: absolute; right: 0; top: 0; bottom: 0; width: 48px; background: linear-gradient(90deg, rgba(16,14,34,0), #100E22)"></span>' : '') + '</div></div>';
  }
  function realStripH() {
    const o = S.ov, n = G.items.filter((it) => !o.hideDone || it.left > 0).length;
    if (!n) return 0;
    return Math.round(o.list ? 28 * o.size / 100 * Math.min(3, n) : 58 * o.size / 100) + 62;
  }
  // Wide: big preview left (it keeps the screen's proportions), settings right (board Wide2).
  function overlayWide() {
    const o = S.ov, ph = L.H - titleH() - navH(), inner = L.W - 340 - 48 - 10;
    const strip = L.H < 600 ? 0 : realStripH();
    const pw = Math.max(200, Math.floor(Math.min(inner, (ph - 130 - strip) * o.screenW / o.screenH)));
    const left = '<div class="cb-fade" data-key="overlay" style="padding-top: 20px; width: ' + pw + 'px">' + ovTitleRow() + '<div style="margin-top: 14px">' + sceneHtml(pw) + '</div>' + (strip ? realStrip(pw) : '') + '</div>';
    return [left, ovSettingsHtml(false, 20), 340];
  }

  // ---------- settings ----------
  function setAutoHtml() {
    const s = S;
    return '<div class="cb-card" data-tour="set-auto" style="margin-top: 0">' +
      '<div class="cb-crow"><div style="min-width: 0"><div class="cb-clab">' + esc(t('autoUpdate')) + '</div><div class="cb-csub">' + esc(t('autoUpdateSub')) + '</div></div>' + sw(s.auto, 'setAuto', t('autoUpdate')) + '</div>' +
      '<div class="cb-crow cb-cdiv" style="opacity: ' + (s.auto ? 1 : 0.45) + '"><span class="cb-clab">' + esc(t('every')) + '</span>' +
      '<div style="display: flex; align-items: center; height: 40px; border-radius: 10px; background: #15122B; box-shadow: inset 0 0 0 1px #2F2A55">' +
      '<button class="cb-step" aria-label="' + esc(t('rarer')) + '" data-act="intDec"' + (!s.auto || s.interval <= 1 ? ' disabled' : '') + '>−</button>' +
      '<span class="cb-tnum" style="min-width: 64px; text-align: center; font-weight: 600; font-size: 15px">' + esc(t('minutes', s.interval)) + '</span>' +
      '<button class="cb-step" aria-label="' + esc(t('oftener')) + '" data-act="intInc"' + (!s.auto || s.interval >= 30 ? ' disabled' : '') + '>+</button>' +
      '</div></div></div>';
  }
  function setStartHtml() {
    const s = S;
    return '<div class="cb-card"><div class="cb-crow"><span class="cb-clab">' + esc(t('startup')) + '</span>' + sw(s.startup, 'setStartup', t('startup')) + '</div>' +
      '<div class="cb-crow cb-cdiv"><div style="min-width: 0"><div class="cb-clab">' + esc(t('closeQuits')) + '</div><div class="cb-csub">' + esc(t('closeQuitsSub')) + '</div></div>' + sw(s.closeQuits, 'setCloseQuits', t('closeQuits')) + '</div></div>';
  }
  function setLangHtml(mt) {
    return '<div class="cb-card" style="margin-top: ' + mt + 'px; padding: 14px 16px"><div class="cb-clab" style="margin-bottom: 10px">' + esc(t('language')) + '</div>' +
      '<div class="cb-seg">' + ['en', 'ru', 'de', 'fr'].map((l) => '<button data-on="' + (S.lang === l) + '" data-act="setLang" data-lang="' + l + '">' + l.toUpperCase() + '</button>').join('') + '</div></div>';
  }
  function setLoginHtml() {
    const s = S;
    return '<div class="cb-card" data-tour="set-login"><div class="cb-crow"><div style="min-width: 0"><div class="cb-clab">' + esc(t('login')) + '</div><div class="cb-csub">' + esc(s.signedIn ? t('chSaved') : t('notSigned')) + '</div></div>' +
      '<button class="cb-ghost" data-act="signOut"' + (s.signedIn ? '' : ' disabled') + '>' + esc(t('signOut')) + '</button></div></div>';
  }
  // Shown only when the window is not 400 × 680.
  function setSizeHtml(stacked) {
    const now = t('winNow', L.W, L.H), btn = '<button class="cb-ghost" data-act="resetSize">' + esc(t('winReset')) + '</button>';
    if (stacked) {
      return '<div class="cb-card" style="margin-top: 0; padding: 14px 16px"><div class="cb-clab">' + esc(t('winSize')) + '</div>' +
        '<div style="display: flex; align-items: center; justify-content: space-between; gap: 8px; margin-top: 8px"><span class="cb-csub cb-tnum" style="margin: 0">' + esc(now) + '</span>' + btn + '</div></div>';
    }
    return '<div class="cb-card"><div class="cb-crow"><div style="min-width: 0"><div class="cb-clab">' + esc(t('winSize')) + '</div><div class="cb-csub cb-tnum">' + esc(now) + '</div></div>' + btn + '</div></div>';
  }
  function settingsTitle() {
    return '<div class="cb-u" style="font-weight: 700; font-size: ' + (L.w === 'xs' || L.h === 'hs' ? 16 : 18) + 'px; letter-spacing: -0.02em">' + esc(t('settings')) + '</div>';
  }
  function settingsHtml() {
    const s = S;
    return '<div class="cb-fade" data-key="settings" style="padding-top: ' + (L.h === 'hl' ? 20 : 14) + 'px">' + settingsTitle() +
      '<div style="margin-top: 16px">' + setAutoHtml() + '</div>' + setStartHtml() + setLangHtml(12) + setLoginHtml() + (L.plain ? '' : setSizeHtml(false)) +
      '<div style="margin-top: 16px; display: flex; justify-content: space-between; gap: 12px; font-size: 12px; color: #A9A3C7">' +
      '<span style="white-space: nowrap">StashSync ' + esc(s.version) + '</span>' +
      '<a href="#" data-act="open" data-url="https://github.com/Arccalc/StashSync" style="text-align: right">' + esc(t('privacy')) + '</a></div><div style="height: 24px"></div>' +
      '</div>';
  }
  // What the program reads and where it sends it, as four facts in a 2 × 2 grid («Стало 3»).
  function privacyHtml() {
    const items = [[EYE_ICO, t('pv1'), t('pv1s')], [HOME_ICO, t('pv2'), t('pv2s')], [SHIELD_ICO, t('pv3'), t('pv3s')], [GLOBE_ICO, t('pv4'), t('pv4s')]];
    return '<div class="cb-card" style="padding: 14px 16px"><div class="cb-clab">' + esc(t('privacy')) + '</div>' +
      '<div style="display: grid; grid-template-columns: repeat(2, minmax(0, 1fr)); gap: 16px 20px; margin-top: 14px">' +
      items.map((x) => '<div style="display: flex; gap: 12px; align-items: flex-start"><span class="cb-pic">' + x[0] + '</span><div style="min-width: 0"><div style="font-size: 13px; font-weight: 600">' + esc(x[1]) + '</div>' +
        '<div style="font-size: 12px; color: #A9A3C7; margin-top: 2px; line-height: 1.45">' + esc(x[2]) + '</div></div></div>').join('') + '</div>' +
      '<div style="margin-top: 14px; padding-top: 10px; border-top: 1px solid #2A2550; display: flex; flex-wrap: wrap; justify-content: space-between; gap: 4px 12px; font-size: 12px; color: #A9A3C7">' +
      '<span>' + esc(t('pvMore')) + '</span><span>StashSync ' + esc(S.version) + ' · <a href="#" data-act="open" data-url="https://github.com/Arccalc/StashSync">' + esc(t('pvGit')) + '</a></span></div></div>';
  }
  // ≥ 960: switches and sign-in left; language, size and the privacy facts right. Below 960 the facts go under both columns.
  function settingsWide() {
    const three = L.W >= 960;
    const col1 = setAutoHtml() + setStartHtml() + (three ? setLoginHtml() : '');
    const pair = '<div style="display: grid; grid-template-columns: repeat(2, minmax(0, 1fr)); gap: 16px">' + setLangHtml(0) + setSizeHtml(true) + '</div>';
    const grid = three
      ? '<div style="display: grid; grid-template-columns: minmax(0, 1fr) minmax(0, 2fr); gap: 16px; margin-top: 16px; align-items: start"><div>' + col1 + '</div><div>' + pair + privacyHtml() + '</div></div>'
      : '<div style="display: grid; grid-template-columns: repeat(2, minmax(0, 1fr)); gap: 16px; margin-top: 16px; align-items: start"><div>' + col1 + '</div><div>' + setLangHtml(0) + setLoginHtml() + setSizeHtml(false) + '</div></div>' + privacyHtml();
    return '<div class="cb-fade" data-key="settings" style="max-width: 1040px; margin: 0 auto; padding-top: 20px">' + settingsTitle() + grid + '<div style="height: 24px"></div></div>';
  }

  function navHtml() {
    const small = L.h !== 'hl', centred = L.w === 'm' || L.wide;
    const st = (small ? 'height: 48px; gap: 2px; font-size: 12px;' : '') + (centred ? ' flex: 0 1 160px;' : '');
    const ico = (inner) => '<svg width="' + (small ? 18 : 20) + '" height="' + (small ? 18 : 20) + '" viewBox="0 0 20 20" aria-hidden="true">' + inner + '</svg>';
    const tab = (id, label, icon, tour) => '<button class="cb-tab"' + (st ? ' style="' + st + '"' : '') + (tour ? ' data-tour="' + tour + '"' : '') + ' data-on="' + (U.tab === id) + '" data-act="tab" data-tab="' + id + '">' + icon + esc(label) + '</button>';
    return '<nav style="flex: none; display: flex; justify-content: center; border-top: 1px solid #2A2550; background: #1A1634">' +
      tab('sync', t('tabStash'), ico('<path d="' + SYNC_PATH + '" fill="none" stroke="currentColor" stroke-width="1.6" stroke-linecap="round" stroke-linejoin="round"></path>')) +
      tab('overlay', t('tabOverlay'), ico('<rect x="2.5" y="4" width="15" height="10" rx="2" fill="none" stroke="currentColor" stroke-width="1.6"></rect><rect x="4.8" y="6.2" width="2.6" height="2.6" rx=".6" fill="currentColor"></rect><rect x="8.4" y="6.2" width="2.6" height="2.6" rx=".6" fill="currentColor"></rect><path d="M7 17h6" stroke="currentColor" stroke-width="1.6" stroke-linecap="round"></path>'), 'tab-overlay') +
      tab('settings', t('tabSettings'), ico('<path d="M3 6h8.5M15.5 6H17M3 14h2.5M9.5 14H17" fill="none" stroke="currentColor" stroke-width="1.6" stroke-linecap="round"></path><circle cx="13.5" cy="6" r="2" fill="none" stroke="currentColor" stroke-width="1.6"></circle><circle cx="7.5" cy="14" r="2" fill="none" stroke="currentColor" stroke-width="1.6"></circle>'), 'tab-settings') +
      '</nav>';
  }

  // Edge zones of the borderless window: a press starts the native resize drag.
  const RESIZE = ['n', 's', 'e', 'w', 'ne', 'nw', 'se', 'sw'].map((d) => '<div class="cb-rz cb-rz-' + d + '" data-rz="' + d + '"></div>').join('');

  // ---------- small window (variant A «Виджет», board Small1–4): height < 480, or width < 360 and height < 600 ----------
  // Own interface instead of the shrunk default: icon tabs, resources as tiles like the in-game plate.
  function navSmall() {
    const tab = (id, label, icon, tour) => '<button' + (tour ? ' data-tour="' + tour + '"' : '') + ' data-on="' + (U.tab === id) + '" data-act="tab" data-tab="' + id + '" title="' + esc(label) + '" aria-label="' + esc(label) + '">' + icon + '</button>';
    const ico = (inner) => '<svg width="18" height="18" viewBox="0 0 20 20" aria-hidden="true">' + inner + '</svg>';
    return '<nav class="cb-snav">' +
      tab('sync', t('tabStash'), ico('<path d="' + SYNC_PATH + '" fill="none" stroke="currentColor" stroke-width="1.6" stroke-linecap="round" stroke-linejoin="round"></path>')) +
      tab('overlay', t('tabOverlay'), ico('<rect x="2.5" y="4" width="15" height="10" rx="2" fill="none" stroke="currentColor" stroke-width="1.6"></rect><rect x="4.8" y="6.2" width="2.6" height="2.6" rx=".6" fill="currentColor"></rect><rect x="8.4" y="6.2" width="2.6" height="2.6" rx=".6" fill="currentColor"></rect><path d="M7 17h6" stroke="currentColor" stroke-width="1.6" stroke-linecap="round"></path>'), 'tab-overlay') +
      tab('settings', t('tabSettings'), ico('<path d="M3 6h8.5M15.5 6H17M3 14h2.5M9.5 14H17" fill="none" stroke="currentColor" stroke-width="1.6" stroke-linecap="round"></path><circle cx="13.5" cy="6" r="2" fill="none" stroke="currentColor" stroke-width="1.6"></circle><circle cx="7.5" cy="14" r="2" fill="none" stroke="currentColor" stroke-width="1.6"></circle>'), 'tab-settings') +
      '</nav>';
  }
  // Status in the title bar: dot + time of the last update; the full status is the tooltip.
  function pillSmall() {
    if (!S.signedIn) return '';
    const st = status();
    const txt = S.phase === 'syncing' ? '…' : S.lastSync ? hhmm(S.lastSync) : '—';
    return '<span class="cb-pill cb-spill ' + st[0] + '" title="' + esc(st[1]) + '"><span class="cb-dot"></span><span class="cb-tnum">' + esc(txt) + '</span></span>';
  }

  function stashSmall() {
    const s = S, c = clock();
    const syncing = s.phase === 'syncing', done = s.phase === 'done', idle = s.phase === 'idle';
    const bad = idle && (s.failed || !s.site);
    const ringColor = s.failed ? ERR : !s.site ? WARN : '#45D3E6';
    const ringTrack = !idle ? '#2A2550' : s.failed ? 'rgba(255,106,85,.7)' : !s.site ? 'rgba(245,190,46,.7)' : '#2A2550';
    const label = (s.failed ? t('retryNow') : t('updateNow')) + (idle && s.auto && !bad ? ' · ' + t('autoIn', c.txt) : '');
    const ring = '<button class="cb-mini" data-act="sync"' + (idle ? '' : ' disabled') + ' aria-label="' + esc(label) + '" title="' + esc(label) + '" style="width: 34px; height: 34px; color: ' + ringColor + '">' +
      ringHtml(34, ringColor, ringTrack, c.frac, syncing ? 'spin' : done ? 'done' : bad ? 'bad' : idle && s.auto ? 'ring' : 'icon') + '</button>';
    // Error and «site closed» get one line with the action; the ring alone would not say what is wrong.
    const alert = s.failed && idle ? [ERR, t('stFailed'), s.buildWait ? t('subFailedBuild') : t('subFailed'), '<button class="cb-act" style="height: 26px; margin: 0 -8px 0 0; font-size: 12px" data-act="sync">' + esc(t('retryShort')) + '</button>']
      : !s.site && !syncing ? [WARN, t('stNoSite'), t('nText'), '<button class="cb-act" style="height: 26px; margin: 0 -8px 0 0; font-size: 12px" data-act="open" data-url="https://arccalc.github.io">' + esc(t('openSiteShort')) + '</button>'] : null;
    const alertHtml = alert ? '<div class="cb-fade" data-key="salert" style="display: flex; align-items: center; gap: 8px; padding: 0 0 8px; font-size: 12px; color: #C9C3E6"><span style="flex: none; width: 7px; height: 7px; border-radius: 50%; background: ' + alert[0] + '"></span>' +
      '<span style="flex: 1; min-width: 0; white-space: nowrap; overflow: hidden; text-overflow: ellipsis" title="' + esc(alert[2]) + '">' + esc(alert[1]) + '</span>' + alert[3] + '</div>' : '';
    if (!G.received || !G.items.length) {
      const head = '<div data-tour="hero" style="display: flex; align-items: center; gap: 10px; padding: 10px 0 8px"><span style="flex: 1; font-size: 14px; font-weight: 600">' + esc(status()[1]) + '</span>' + ring + '</div>' + alertHtml;
      const empty = G.received ? '<div class="cb-row" data-key="nogoals" style="padding: 14px; border-radius: 12px; background: #17142E; display: flex; flex-direction: column; align-items: flex-start; gap: 6px">' +
        '<span style="opacity: .55; display: flex">' + ARC(34, 25) + '</span><div style="font-size: 14px; font-weight: 600">' + esc(t('gEmpty')) + '</div>' +
        '<button class="cb-ghost" style="height: 30px; font-size: 12px" data-act="open" data-url="https://arccalc.github.io">' + esc(t('openSite')) + '</button></div>' : '';
      return ['<div style="padding: 0 12px">' + head + '</div>', empty];
    }
    const d = goalData();
    const summary = '<div data-tour="hero" style="display: flex; align-items: center; gap: 10px; padding: 10px 0 8px">' +
      (G.legacy ? '' : '<span class="cb-u cb-tnum" data-tour="goals-head" style="font-weight: 800; font-size: 24px; letter-spacing: -0.03em; line-height: 1">' + d.pct + '%</span>') +
      '<div style="flex: 1; min-width: 0">' + (G.legacy ? '' : barHtml(d, 'height: 6px; margin: 0 2px')) +
      '<div class="cb-tnum" style="display: flex; justify-content: space-between; gap: 6px; margin-top: 5px; font-size: 12px; color: #A9A3C7; white-space: nowrap">' +
      '<span>' + esc(t('leftWord')) + ' <b style="color: #F2EEE6">' + esc(t('pcs', d.left)) + '</b></span>' + (idle && s.auto && !bad ? '<span>' + esc(t('autoShort', c.txt)) + '</span>' : '') + '</div></div>' + ring + '</div>';
    const tiles = d.rows.map((r) => {
      const it = r.it, have = Math.min(it.have, it.need);
      const num = G.legacy ? '<span style="color: ' + ERR + '">' + it.left + '</span>' : '<span style="color: ' + r.color + '">' + have + '</span>/' + it.need;
      return '<button class="cb-stile' + (U.bumped[it.name] ? ' cb-bump' : '') + '" data-key="st-' + esc(it.name) + '" data-act="tileTip" data-name="' + esc(it.name) + '" title="' + esc(it.label) + '" aria-label="' + esc(it.label) + '">' +
        (it.icon ? '<img src="' + it.icon + '" alt="">' : '') + '<b class="cb-tnum">' + num + '</b>' +
        (G.legacy ? '' : '<i style="width: ' + r.w + '%; background: ' + r.color + '"></i>') + '</button>';
    }).join('');
    // Bigger tiles when the window has the height for them.
    const grid = '<div data-tour="list" class="cb-fade" data-key="tiles" style="display: grid; grid-template-columns: repeat(auto-fill, minmax(' + (L.H >= 480 || L.W >= 400 ? 92 : 58) + 'px, 1fr)); gap: 6px; padding-bottom: 14px">' + tiles + '</div>';
    return ['<div style="padding: 0 12px">' + summary + alertHtml + '</div>', grid];
  }
  // The name of a tile for a moment (tiles have no labels).
  function tipHtml() {
    if (!U.tip) return '';
    const it = G.items.find((x) => x.name === U.tip);
    if (!it) return '';
    return '<div class="cb-stip" role="status">' + esc(it.label) + ' · <b>' + esc(it.left > 0 ? t('leftShort', it.left) : '✓') + '</b></div>';
  }

  function overlaySmall() {
    const o = S.ov, inner = L.W - 24 - 10, mw = Math.max(96, Math.min(124, inner - 130));
    const mh = Math.round(mw * o.screenH / o.screenW);
    const corners = [['tl', t('cTL'), 'left: 5%; top: 7%'], ['tr', t('cTR'), 'right: 5%; top: 7%'], ['bl', t('cBL'), 'left: 5%; bottom: 7%'], ['br', t('cBR'), 'right: 5%; bottom: 7%']];
    const iseg = (on, act, label, icon, disabled) => '<button data-on="' + on + '" data-act="' + act + '" title="' + esc(label) + '" aria-label="' + esc(label) + '"' + (disabled ? ' disabled' : '') + '>' + icon + '</button>';
    const SQ = '<svg width="14" height="14" viewBox="0 0 14 14" aria-hidden="true"><rect x="1.5" y="1.5" width="4.5" height="4.5" rx="1" fill="currentColor"></rect><rect x="8" y="1.5" width="4.5" height="4.5" rx="1" fill="currentColor"></rect><rect x="1.5" y="8" width="4.5" height="4.5" rx="1" fill="currentColor"></rect><rect x="8" y="8" width="4.5" height="4.5" rx="1" fill="currentColor"></rect></svg>';
    const LST = '<svg width="14" height="14" viewBox="0 0 14 14" aria-hidden="true"><path d="M2 3.5h10M2 7h10M2 10.5h10" stroke="currentColor" stroke-width="1.6" stroke-linecap="round"></path></svg>';
    const ACR = '<svg width="14" height="14" viewBox="0 0 14 14" aria-hidden="true"><path d="M2 7h9M8 4l3 3-3 3" fill="none" stroke="currentColor" stroke-width="1.6" stroke-linecap="round" stroke-linejoin="round"></path></svg>';
    const DWN = '<svg width="14" height="14" viewBox="0 0 14 14" aria-hidden="true"><path d="M7 2v9M4 8l3 3 3-3" fill="none" stroke="currentColor" stroke-width="1.6" stroke-linecap="round" stroke-linejoin="round"></path></svg>';
    const sizeP = (o.size - 50) / 110 * 100, opP = (o.opacity - 20) / 80 * 100;
    return '<div class="cb-fade" data-key="ov-small" style="padding-top: 10px">' +
      '<div class="cb-scard"><div class="cb-sline"><div style="min-width: 0"><div class="cb-slab">' + esc(t('ovTitle')) + '</div><div class="cb-ssub">' + esc(t(o.shown ? 'ovShownKey' : 'ovHiddenKey', o.key)) + '</div></div>' + sw(o.shown, 'ovShown', t('ovTitle')) + '</div></div>' +
      // A tiny screen instead of the preview: a press on a corner puts the plate there.
      '<div class="cb-scard" data-tour="scene" style="margin-top: 8px; padding: 10px 12px; display: flex; gap: 12px; align-items: center">' +
      '<div class="cb-mscr" style="width: ' + mw + 'px; height: ' + mh + 'px">' + corners.map((k) => '<button data-on="' + (o.corner === k[0]) + '" data-act="ovCorner" data-c="' + k[0] + '" style="' + k[2] + '" title="' + esc(k[1]) + '" aria-label="' + esc(k[1]) + '"></button>').join('') + '</div>' +
      '<div data-tour="ov-look" style="flex: 1; min-width: 0; display: flex; flex-direction: column; gap: 8px"><div class="cb-ssub">' + esc(t('cornerTap')) + '</div>' +
      '<span class="cb-sseg">' + iseg(!o.list, 'ovTiles', t('tiles'), SQ) + iseg(o.list, 'ovListMode', t('list'), LST) + '</span>' +
      '<span class="cb-sseg">' + iseg(!o.vertical, 'ovAcross', t('across'), ACR, o.list) + iseg(o.vertical, 'ovDown', t('down'), DWN, o.list) + '</span></div></div>' +
      '<div class="cb-scard" style="margin-top: 8px; padding: 8px 12px 4px">' +
      '<div class="cb-cline" style="margin-bottom: 6px"><label for="cb-size" class="cb-slab">' + esc(t('size')) + '</label><span class="cb-tnum cb-ssub">' + o.size + '%</span></div><input id="cb-size" class="cb-range" type="range" min="50" max="160" step="5" value="' + o.size + '" data-input="ovSize" style="--p: ' + sizeP + '%">' +
      '<div class="cb-cline" style="margin: 6px 0"><label for="cb-op" class="cb-slab">' + esc(t('opacity')) + '</label><span class="cb-tnum cb-ssub">' + o.opacity + '%</span></div><input id="cb-op" class="cb-range" type="range" min="20" max="100" step="1" value="' + o.opacity + '" data-input="ovOpacity" style="--p: ' + opP + '%"></div>' +
      '<div class="cb-scard" data-tour="ov-key" style="margin-top: 8px"><div class="cb-sline"><span class="cb-slab" title="' + esc(t('keySub')) + '">' + esc(t('key')) + '</span><button class="cb-key" style="height: 30px; min-width: 48px; font-size: 13px" data-wait="' + U.keyWait + '" aria-label="' + esc(t('key') + ': ' + o.key) + '" data-act="askKey">' + esc(U.keyWait ? '…' : o.key) + '</button></div>' +
      '<div class="cb-sline"><span class="cb-slab" title="' + esc(t('hideDoneSub')) + '">' + esc(t('hideDone')) + '</span>' + sw(o.hideDone, 'ovHideDone', t('hideDone')) + '</div></div>' +
      '<button class="cb-ghost" data-act="ovReset" style="margin-top: 10px; height: 30px; font-size: 12px">' + esc(t('reset')) + '</button><div style="height: 14px"></div></div>';
  }

  function settingsSmall() {
    const s = S;
    return '<div class="cb-fade" data-key="set-small" style="padding-top: 10px">' +
      '<div class="cb-scard" data-tour="set-auto"><div class="cb-sline"><span class="cb-slab" title="' + esc(t('autoUpdateSub')) + '">' + esc(t('autoUpdate')) + '</span>' + sw(s.auto, 'setAuto', t('autoUpdate')) + '</div>' +
      '<div class="cb-sline" style="opacity: ' + (s.auto ? 1 : 0.45) + '"><span class="cb-slab">' + esc(t('every')) + '</span><span class="cb-sstp">' +
      '<button class="cb-step" style="width: 28px; height: 28px" aria-label="' + esc(t('rarer')) + '" data-act="intDec"' + (!s.auto || s.interval <= 1 ? ' disabled' : '') + '>−</button>' +
      '<span class="cb-tnum" style="min-width: 48px; text-align: center">' + esc(t('minutes', s.interval)) + '</span>' +
      '<button class="cb-step" style="width: 28px; height: 28px" aria-label="' + esc(t('oftener')) + '" data-act="intInc"' + (!s.auto || s.interval >= 30 ? ' disabled' : '') + '>+</button></span></div>' +
      '<div class="cb-sline"><span class="cb-slab">' + esc(t('startupShort')) + '</span>' + sw(s.startup, 'setStartup', t('startup')) + '</div>' +
      '<div class="cb-sline"><span class="cb-slab" title="' + esc(t('closeQuitsSub')) + '">' + esc(t('closeQuitsShort')) + '</span>' + sw(s.closeQuits, 'setCloseQuits', t('closeQuits')) + '</div></div>' +
      '<div class="cb-scard" style="margin-top: 8px"><div class="cb-sline"><span class="cb-slab">' + esc(t('language')) + '</span><span class="cb-slang">' +
      ['en', 'ru', 'de', 'fr'].map((l) => '<button data-on="' + (s.lang === l) + '" data-act="setLang" data-lang="' + l + '">' + l.toUpperCase() + '</button>').join('') + '</span></div>' +
      '<div class="cb-sline" data-tour="set-login"><div style="min-width: 0"><div class="cb-slab">' + esc(t('login')) + '</div><div class="cb-ssub">' + esc(s.signedIn ? t('chSaved') : t('notSigned')) + '</div></div>' +
      '<button class="cb-ghost" style="height: 30px; font-size: 12px" data-act="signOut"' + (s.signedIn ? '' : ' disabled') + '>' + esc(t('signOut')) + '</button></div>' +
      '<div class="cb-sline"><div style="min-width: 0"><div class="cb-slab">' + esc(t('winShort')) + '</div><div class="cb-ssub cb-tnum">' + L.W + '×' + L.H + '</div></div>' +
      '<button class="cb-ghost" style="height: 30px; font-size: 12px" data-act="resetSize" title="' + esc(t('winReset')) + '">400×680</button></div></div>' +
      '<div style="display: flex; justify-content: space-between; gap: 8px; padding: 10px 2px 14px; font-size: 12px; color: #A9A3C7"><span class="cb-tnum">StashSync ' + esc(s.version) + '</span>' +
      '<a href="#" data-act="open" data-url="https://github.com/Arccalc/StashSync" title="' + esc(t('privacy')) + '">' + esc(t('privShort')) + '</a></div></div>';
  }

  // First run: only the step to do now; finished steps fold into one line.
  function routeSmall() {
    const s = S;
    const steps = [s.steamOn, !s.gameOn, s.signedIn];
    let cur = 0; while (cur < 3 && steps[cur]) cur++;
    const warn = s.gameOn && cur === 1;
    const errText = (() => {
      const e = s.loginError || '';
      if (!e) return '';
      if (/nobody is signed in/i.test(e)) return t('errSteamNoUser');
      if (/open steam/i.test(e)) return t('errOpenSteam');
      if (/close the game/i.test(e)) return '';
      return t('errLogin');
    })();
    const defs = [[t('rSteam'), s.steamOn ? t('rSteamOk') : t('rSteamWait')], s.gameOn ? [t('rGameOpen'), t('rGameOpenText')] : [t('rGameClosed'), t('rGameClosedText')],
      [t('rLogin'), errText || t('rLoginText')], [t('rSite'), t('rSiteText')]];
    const i = Math.min(cur, 3), def = defs[i];
    const busy = i === 2 && s.signing;
    const ringC = warn ? WARN : '#45D3E6';
    const seg = [0, 1, 2, 3].map((k) => '<span style="background: ' + (k < cur || (k === 3 && s.site && cur >= 3) ? OK : k === i ? ringC : '#2A2550') + '"></span>').join('');
    const btn = warn ? '<button class="cb-btn" style="width: 100%; height: 36px; margin-top: 12px; font-size: 13px" data-act="recheck">' + esc(t('rGameClosed')) + '</button>'
      : i === 2 && !busy ? '<button class="cb-btn" style="width: 100%; height: 36px; margin-top: 12px; font-size: 13px" data-act="login">' + esc(t('signIn')) + '</button>' : '';
    const doneList = defs.slice(0, Math.min(cur, 3)).map((x) => '<span style="display: inline-flex; align-items: center; gap: 4px">' + CHECK('', 'currentColor').replace('width="20" height="20"', 'width="12" height="12"') + esc(x[0]) + '</span>').join('');
    return '<div class="cb-fade" data-key="route-small" style="padding-top: 12px">' +
      '<div class="cb-u" style="font-weight: 700; font-size: 15px">' + esc(t('rTitleShort')) + '</div>' +
      '<div style="display: flex; align-items: center; gap: 8px; margin-top: 10px"><div class="cb-sstep" style="flex: 1">' + seg + '</div><span class="cb-tnum cb-ssub">' + esc(t('nOfM', i + 1, 4)) + '</span></div>' +
      '<div class="cb-scard" data-tour="route" style="margin-top: 12px; padding: 14px">' +
      '<div style="display: flex; gap: 12px; align-items: center"><span class="cb-tnum' + (busy ? '' : ' cb-pulse') + '" style="flex: none; width: 32px; height: 32px; border-radius: 50%; background: ' + (warn ? '#32282E' : '#1F2A4A') + '; box-shadow: inset 0 0 0 1.5px ' + ringC + '; color: ' + ringC + '; display: flex; align-items: center; justify-content: center; font-weight: 700">' +
      (busy ? '<svg class="cb-spin" width="18" height="18" viewBox="0 0 20 20" aria-hidden="true"><path d="M10 3a7 7 0 1 1-7 7" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round"></path></svg>' : warn ? '!' : i + 1) + '</span>' +
      '<div style="min-width: 0"><div style="font-size: 15px; font-weight: 600">' + esc(def[0]) + '</div><div class="cb-ssub" style="margin-top: 2px; line-height: 1.4; color: ' + (warn ? '#E9DFC4' : i === 2 && errText ? '#FFB3A6' : '#A9A3C7') + '">' + esc(def[1]) + '</div></div></div>' + btn + '</div>' +
      (doneList ? '<div style="display: flex; flex-wrap: wrap; gap: 4px 12px; margin-top: 10px; font-size: 12px; color: #9BE8B2">' + doneList + '</div>' : '') +
      (i < 3 ? '<div style="margin-top: 6px; font-size: 12px; color: #A9A3C7">' + esc(t('nextStep', (i + 2) + ' · ' + defs[i + 1][0])) + '</div>' : '') +
      '<div style="height: 14px"></div></div>';
  }

  function mainSmall() {
    let fixed = '', body;
    if (U.tab === 'overlay') body = overlaySmall();
    else if (U.tab === 'settings') body = settingsSmall();
    else if (S.signedIn) [fixed, body] = stashSmall();
    else body = routeSmall();
    // A wide but low window: cards and the route stay readable instead of stretching across.
    const cap = (html) => (U.tab === 'sync' && S.signedIn ? html : '<div style="max-width: 560px; margin: 0 auto">' + html + '</div>');
    return titlebar() + '<div style="flex: none">' + cap(fixed) + '</div>' + scroller(cap(body), 12, 2, ' flex: 1; min-height: 0') + navSmall() + tipHtml() + tourHtml() + RESIZE;
  }

  function mainHtml() {
    if (L.small) return mainSmall();
    let fixed = '', body;
    if (L.wide && U.tab !== 'settings') {
      // Two columns, each with its own scroller.
      const [left, right, lw] = U.tab === 'overlay' ? overlayWide() : S.signedIn ? stashWide() : routeWide();
      const ovTab = U.tab === 'overlay';
      body = '<div style="flex: 1; min-height: 0; display: flex">' +
        (ovTab ? scroller(left, 20, 18, ' flex: 1; min-width: 0') + scroller(right, 20, 2, ' flex: 0 0 ' + lw + 'px; border-left: 1px solid #2A2550')
          : scroller(left, 20, 10, ' flex: 0 0 ' + lw + 'px; border-right: 1px solid #2A2550') + scroller(right, 24, 4, ' flex: 1; min-width: 0; position: relative')) + '</div>';
    } else if (L.wide) {
      body = scroller(settingsWide(), 20, 10, ' flex: 1; min-height: 0');
    } else {
      let inner;
      if (U.tab === 'overlay') [fixed, inner] = overlayHtml();
      else if (U.tab === 'sync' && S.signedIn) [fixed, inner] = stashHtml();
      else inner = U.tab === 'settings' ? settingsHtml() : routeHtml();
      body = scroller(capW(inner), padL(), padR(), ' flex: 1; min-height: 0');
    }
    // The fixed slot is always there, so the scroller and the tab bar keep their nodes across tabs.
    return titlebar() + '<div style="flex: none">' + fixed + '</div>' + body + navHtml() + tourHtml() + RESIZE;
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
      // Filter steps only where the filter chips are (not in the small tiles or the narrow sections).
      if (G.items.length && !G.legacy && !L.small && L.w !== 'xs') {
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
    steps.push({ at: ['winbtns'], enter: { tab: 'sync' }, text: t('t_tray', s.closeQuits) });
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
      // keep the ring inside the window, so its edge shows for tabs, title-bar buttons and «?» at the border
      return { l: Math.round(Math.max(2, b.left - 6)), t: Math.round(Math.max(2, top - 6)), r: Math.round(Math.min(innerWidth - 2, b.right + 6)), b: Math.round(Math.min(innerHeight - 2, bottom + 6)) };
    }).filter((x) => x.b - x.t > 8);
    const card = document.getElementById('cb-tc'), ch = card ? card.offsetHeight : 0;
    const key = rects.map((x) => [x.l, x.t, x.r, x.b].join(',')).join('|') + '#' + ch;
    if (key !== U.tourKey) { U.tourKey = key; U.trect = rects.length ? rects : null; U.tch = ch; render(); }
  }
  function tourHtml() {
    const tr = U.tour; if (!tr) return '';
    const steps = tourSteps(), st = steps[tr.i] || {}, rects = U.trect || [];
    const W = window.innerWidth, H = window.innerHeight, cw = Math.min(300, W - 24), ch = U.tch || 220, gap = 16, m = Math.min(16, (W - cw) / 2);
    let cl = W / 2 - cw / 2, ct = H / 2 - ch / 2;
    if (rects.length) {
      const R = { l: Math.min(...rects.map((x) => x.l)), t: Math.min(...rects.map((x) => x.t)), r: Math.max(...rects.map((x) => x.r)), b: Math.max(...rects.map((x) => x.b)) };
      const cx = (x) => Math.max(m, Math.min(W - cw - m, x)), cy = (y) => Math.max(m, Math.min(H - ch - m, y));
      if (R.r + gap + cw <= W - m) { cl = R.r + gap; ct = cy(R.t); }
      else if (R.l - gap - cw >= m) { cl = R.l - gap - cw; ct = cy(R.t); }
      else if (R.b + gap + ch <= H - m) { ct = R.b + gap; cl = cx(R.l); }
      else if (R.t - gap - ch >= m) { ct = R.t - gap - ch; cl = cx(R.l); }
      // no free side: put the card on the side with more room so it covers as little of the target as possible
      else { ct = H - m - R.b >= R.t - m ? cy(H - ch - m) : cy(m); cl = cx(R.l); }
    }
    const hole = (x) => { const q = Math.min(10, (x.r - x.l) / 2, (x.b - x.t) / 2); return 'M' + (x.l + q) + ' ' + x.t + 'H' + (x.r - q) + 'A' + q + ' ' + q + ' 0 0 1 ' + x.r + ' ' + (x.t + q) + 'V' + (x.b - q) + 'A' + q + ' ' + q + ' 0 0 1 ' + (x.r - q) + ' ' + x.b + 'H' + (x.l + q) + 'A' + q + ' ' + q + ' 0 0 1 ' + x.l + ' ' + (x.b - q) + 'V' + (x.t + q) + 'A' + q + ' ' + q + ' 0 0 1 ' + (x.l + q) + ' ' + x.t + 'Z'; };
    const last = tr.i === steps.length - 1, wait = !!(st.until && !st.until());
    // Low windows: a compact card (smaller text, «Skip» in the button row) so it covers less of the target.
    const sm = H < 480;
    const skip = !last ? '<button class="cb-tskip" data-act="tourEnd">' + esc(t('tourSkip')) + '</button>' : '';
    const nav = '<button class="cb-ghost"' + (sm ? ' style="height: 32px"' : '') + ' data-act="tourBack"' + (tr.i === 0 ? ' disabled' : '') + '>' + esc(t('back')) + '</button>' +
      '<button class="cb-btn" style="height: ' + (sm ? 32 : 36) + 'px; font-size: 13px" data-act="tourNext">' + esc(last ? t('finish') : t('next')) + '</button>';
    return '<div class="cb-tdim" aria-hidden="true" style="clip-path: path(evenodd, \'M0 0H' + W + 'V' + H + 'H0Z' + rects.map(hole).join('') + '\')"></div>' +
      rects.map((x, i) => '<div class="cb-tring" data-key="ring' + i + '" style="left: ' + x.l + 'px; top: ' + x.t + 'px; width: ' + (x.r - x.l) + 'px; height: ' + (x.b - x.t) + 'px"></div>').join('') +
      '<div id="cb-tc" class="cb-tc' + (sm ? ' cb-tc-sm' : '') + '" role="dialog" aria-label="' + esc(t('tourAria')) + '" style="width: ' + cw + 'px; left: ' + Math.round(cl) + 'px; top: ' + Math.round(ct) + 'px">' +
      '<div style="display: flex; align-items: center; justify-content: space-between; gap: 10px"><span class="cb-tstep">' + esc(t('stepOf', tr.i + 1, steps.length)) + '</span>' +
      '<button class="cb-wbtn" style="width: 28px; height: 28px" aria-label="' + esc(t('tourClose')) + '" data-act="tourEnd"><svg width="12" height="12" viewBox="0 0 14 14" aria-hidden="true"><path d="M3.5 3.5l7 7M10.5 3.5l-7 7" stroke="currentColor" stroke-width="1.6" stroke-linecap="round"></path></svg></button></div>' +
      '<p class="cb-ttext" aria-live="polite">' + esc(st.text || '') + '</p>' +
      (wait ? '<p class="cb-twait">' + esc(t('tourWait')) + '</p>' : '') +
      (st.until && !wait ? '<p class="cb-tok">' + esc(t('tourOk')) + '</p>' : '') +
      (sm ? '<div class="cb-tact">' + skip + nav + '</div>' : '<div class="cb-tact">' + nav + '</div>' + skip) +
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
      L = lay();
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
    close: () => { if (U.tour) endTour(); send('close'); },
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
    setCloseQuits: () => send('setCloseQuits', { on: !S.closeQuits }),
    setLang: (el) => send('setLang', { lang: el.getAttribute('data-lang') }),
    signOut: () => send('signOut'),
    resetSize: () => send('resetSize'),
    fold: (el) => { const k = el.getAttribute('data-g'); U.fold[k] = !U.fold[k]; },
    tileTip: (el) => { U.tip = el.getAttribute('data-name'); clearTimeout(U.tipTimer); U.tipTimer = setTimeout(() => { U.tip = null; schedule(); }, 1800); }
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
    const rz = e.target.closest('[data-rz]');
    if (rz) { e.preventDefault(); send('resize', { dir: rz.getAttribute('data-rz') }); return; }
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
  // The layout follows the window size (resize drag, «Back to 400 × 680»).
  window.addEventListener('resize', () => { if (VIEW === 'main') schedule(); });

  // Drag the overlay inside the preview. Screen pixels go to Rust on release.
  document.addEventListener('pointerdown', (e) => {
    const box = e.target.closest('[data-ovbox]');
    if (!box || e.button !== 0) return;
    const g = ovGeo();
    U.drag = { x: e.clientX, y: e.clientY, ox: g.x, oy: g.y, w: g.W };
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
    const k = S.ov.screenW / U.drag.w, p = U.dragPos;
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
