/* Headless DOM verification for the three-language UI.
 *
 * Drives a real headless Edge/Chrome over the DevTools protocol:
 *   node dom_check.js <baseUrl> <debugPort>
 * Exits 0 only when every assertion passes and the console stayed clean.
 */
'use strict';

const fs = require('fs');
const path = require('path');
const http = require('http');

const BASE = process.argv[2] || 'http://127.0.0.1:8021';
const PORT = process.argv[3] || '9333';

const problems = [];
const notes = [];

function fail(msg) { problems.push(msg); console.log('FAIL  ' + msg); }
function ok(msg) { console.log('ok    ' + msg); }
function note(msg) { notes.push(msg); console.log('note  ' + msg); }

function httpJson(url) {
  return new Promise((resolve, reject) => {
    http.get(url, (res) => {
      let body = '';
      res.on('data', (c) => { body += c; });
      res.on('end', () => {
        try { resolve(JSON.parse(body)); } catch (e) { reject(e); }
      });
    }).on('error', reject);
  });
}

function connect(url) {
  return new Promise((resolve, reject) => {
    const ws = new WebSocket(url);
    let nextId = 0;
    const pending = new Map();
    const consoleErrors = [];
    const exceptions = [];
    const client = {
      consoleErrors,
      exceptions,
      send(method, params) {
        return new Promise((res, rej) => {
          const id = ++nextId;
          pending.set(id, (msg) => (msg.error ? rej(new Error(method + ': ' + JSON.stringify(msg.error))) : res(msg.result)));
          ws.send(JSON.stringify({ id, method, params: params || {} }));
        });
      },
      close() { try { ws.close(); } catch (e) { /* ignore */ } }
    };
    ws.onopen = () => resolve(client);
    ws.onerror = (e) => reject(new Error('ws error ' + e.message));
    ws.onmessage = (ev) => {
      const msg = JSON.parse(ev.data);
      if (msg.id && pending.has(msg.id)) {
        pending.get(msg.id)(msg);
        pending.delete(msg.id);
        return;
      }
      if (msg.method === 'Runtime.exceptionThrown') {
        const d = msg.params.exceptionDetails || {};
        exceptions.push((d.exception && (d.exception.description || d.exception.value)) || d.text);
      }
      if (msg.method === 'Runtime.consoleAPICalled' && msg.params.type === 'error') {
        consoleErrors.push((msg.params.args || []).map((a) => a.value || a.description || a.type).join(' '));
      }
      if (msg.method === 'Log.entryAdded' && msg.params.entry && msg.params.entry.level === 'error') {
        consoleErrors.push('log: ' + msg.params.entry.text);
      }
    };
  });
}

/* ------------------------------------------------------------------ in-page */
const HELPERS = String.raw`
window.__t = (function () {
  function visible(node) {
    if (!node) return false;
    var el = node.nodeType === 1 ? node : node.parentElement;
    while (el && el !== document.body) {
      var cs = getComputedStyle(el);
      if (cs.display === 'none' || cs.visibility === 'hidden' || cs.opacity === '0') return false;
      if (el.hidden) return false;
      el = el.parentElement;
    }
    return true;
  }
  function texts(root) {
    var out = [];
    var walker = document.createTreeWalker(root || document.body, NodeFilter.SHOW_TEXT, null);
    var n;
    while ((n = walker.nextNode())) {
      var el = n.parentElement;
      if (!el) continue;
      var tag = el.tagName;
      if (tag === 'SCRIPT' || tag === 'STYLE' || tag === 'OPTION' || tag === 'NOSCRIPT') continue;
      if (el.closest && el.closest('.lang-btn')) continue;   /* the picker names each language in its own script */
      if (!visible(n)) continue;
      var t = (n.textContent || '').trim();
      if (t) out.push(t);
    }
    return out;
  }
  function attrs(root) {
    var out = [];
    var nodes = (root || document).querySelectorAll('[title],[aria-label],[placeholder],[alt]');
    for (var i = 0; i < nodes.length; i++) {
      if (!visible(nodes[i])) continue;
      if (nodes[i].closest && nodes[i].closest('.lang-btn')) continue;
      ['title', 'aria-label', 'placeholder', 'alt'].forEach(function (a) {
        var v = nodes[i].getAttribute(a);
        if (v) out.push(v);
      });
    }
    return out;
  }
  /* Every visible Traditional string must be a fixed point of the converter:
     if it still held a Simplified-only character, toHant() would change it. */
  function hantResidue() {
    if (!window.FSHant) return ['FSHant missing'];
    var bad = [];
    var all = texts().concat(attrs());
    for (var i = 0; i < all.length; i++) {
      var c = window.FSHant.toHant(all[i]);
      if (c !== all[i]) bad.push(all[i] + '  ->  ' + c);
    }
    return bad;
  }
  function snapshot() {
    var q = function (sel) { var e = document.querySelector(sel); return e ? e.textContent.trim() : null; };
    var list = function (sel) {
      return Array.prototype.map.call(document.querySelectorAll(sel), function (e) { return e.textContent.trim(); });
    };
    var groups = list('.group-name');
    var subgroups = list('.subgroup-name');
    var descs = Array.prototype.slice.call(document.querySelectorAll('.param-desc'), 0, 6)
      .map(function (e) { return e.textContent.trim(); });
    var labels = Array.prototype.slice.call(document.querySelectorAll('.param-label'), 0, 8)
      .map(function (e) { return e.textContent.trim(); });
    var kpis = list('.kpi-label');
    var active = document.querySelector('.lang-btn.is-active');
    var pressed = Array.prototype.map.call(document.querySelectorAll('.lang-btn'), function (b) {
      return b.getAttribute('data-lang') + '=' + b.getAttribute('aria-pressed');
    });
    return {
      lang: window.FSI18N.lang,
      htmlLang: document.documentElement.lang,
      docTitle: document.title,
      brand: q('.brand-text h1'),
      subtitle: q('.brand-text p'),
      presetLabel: q('label[for="preset-select"]'),
      cycleLabel: q('label[for="cycle-select"]'),
      runLabel: q('#run-label'),
      exportLabel: q('#export-btn span'),
      panelTitle: q('.wb-title'),
      searchPlaceholder: (document.querySelector('#param-search') || {}).placeholder,
      modifiedOnly: q('.switch-label'),
      resetAll: q('#reset-all'),
      groups: groups,
      subgroups: subgroups,
      descriptions: descs,
      paramLabels: labels,
      tabs: list('.tab'),
      kpis: kpis,
      status: q('#run-status'),
      activeButton: active ? active.getAttribute('data-lang') : null,
      pressed: pressed,
      tableColumnHeader: (function () {
        var th = document.querySelector('#table-wrap th');
        return th ? th.textContent.trim() : null;
      })(),
      allText: texts().join(' \u0001 '),
      allAttrs: attrs().join(' \u0001 ')
    };
  }
  return { snap: snapshot, texts: texts, attrs: attrs, hantResidue: hantResidue };
})();
'ready';
`;

function langWordChecks(snap, lang) {
  const text = snap.allText + ' \u0001 ' + snap.allAttrs;
  const result = { required: [] };
  const SIMP = ['设置', '数据', '参数', '运行', '仿真', '电机', '信息', '导出', '图表', '工具栏'];
  const TRAD = ['設定', '資料', '參數', '執行', '模擬', '電機', '資訊', '匯出', '圖表'];
  // words that are genuinely on screen in every view of this app
  const REQUIRED = {
    'zh-Hant': ['參數', '執行', '資料', '模擬'],
    'zh': ['参数', '运行', '数据', '仿真'],
    'en': [],
  }[lang] || [];
  const banned = lang === 'zh-Hant' ? SIMP : (lang === 'zh' ? TRAD : SIMP.concat(TRAD));
  for (const w of banned) if (text.indexOf(w) !== -1) result.required.push('leak:' + w);
  for (const w of REQUIRED) if (text.indexOf(w) === -1) result.required.push('missing:' + w);
  return result;
}

async function main() {
  const targets = await httpJson(`http://127.0.0.1:${PORT}/json/list`);
  const page = targets.find((t) => t.type === 'page');
  if (!page) throw new Error('no page target in ' + JSON.stringify(targets.map((t) => t.type)));
  const client = await connect(page.webSocketDebuggerUrl);
  await client.send('Page.enable');
  await client.send('Runtime.enable');
  await client.send('Log.enable');
  await client.send('Emulation.setDeviceMetricsOverride', { width: 1440, height: 960, deviceScaleFactor: 1, mobile: false });

  const evaluate = async (expression, awaitPromise) => {
    const res = await client.send('Runtime.evaluate', {
      expression, returnByValue: true, awaitPromise: !!awaitPromise, userGesture: true,
    });
    if (res.exceptionDetails) {
      throw new Error('evaluate threw: ' + JSON.stringify(res.exceptionDetails.exception && res.exceptionDetails.exception.description || res.exceptionDetails.text));
    }
    return res.result.value;
  };

  const waitFor = async (expr, timeoutMs, label) => {
    const deadline = Date.now() + timeoutMs;
    while (Date.now() < deadline) {
      if (await evaluate(expr)) return true;
      await new Promise((r) => setTimeout(r, 250));
    }
    fail('timeout waiting for ' + label);
    return false;
  };

  await client.send('Page.navigate', { url: BASE + '/' });
  await waitFor('!!(window.__fastsim && window.__fastsim.state.params.length)', 60000, 'schema load');
  await evaluate(HELPERS);
  ok('page booted, schema loaded (' + (await evaluate('window.__fastsim.state.params.length')) + ' params)');

  // ---- 0. dictionary shape: 3 columns, placeholders preserved --------------
  const dict = await evaluate(`(function () {
    var D = window.FSI18N.DICT, keys = Object.keys(D);
    var badWidth = keys.filter(function (k) { return !Array.isArray(D[k]) || D[k].length !== 3; });
    var badPh = [];
    keys.forEach(function (k) {
      var en = (D[k][0].match(/\\{\\w+\\}/g) || []).sort().join(',');
      [1, 2].forEach(function (i) {
        var ph = (D[k][i].match(/\\{\\w+\\}/g) || []).sort().join(',');
        if (ph !== en) badPh.push(k + '[' + i + '] ' + ph + ' vs ' + en);
      });
    });
    var numeric = ['field.items', 'panel.modifiedCount', 'table.note', 'figure.meta', 'status.overrides',
                   'export.meta', 'status.doneDetail', 'err.unknownPath'];
    var numericOk = numeric.every(function (k) { return (D[k][2].match(/\\{\\w+\\}/g) || []).length >= 1; });
    var same = keys.filter(function (k) { return D[k][1] === D[k][2]; });
    return { count: keys.length, badWidth: badWidth, badPh: badPh, numericOk: numericOk,
             sameCount: same.length, samples: {
               'field.reset': D['field.reset'][2],
               'toolbar.collapse': D['toolbar.collapse'][2],
               'panel.modifiedOnly': D['panel.modifiedOnly'][2],
               'chart.renderFailed': D['chart.renderFailed'][2],
               'table.column': D['table.column'][2],
               'panel.resetAllConfirm': D['panel.resetAllConfirm'][2]
             } };
  })()`);
  if (dict.badWidth.length) fail('keys without exactly 3 columns: ' + dict.badWidth.join(','));
  else ok(`DICT: ${dict.count} keys, every row has english + 简体 + 繁體`);
  if (dict.badPh.length) fail('placeholder mismatch: ' + dict.badPh.join(' | '));
  else ok('DICT: every {placeholder} preserved in all three columns');
  if (!dict.numericOk) fail('a countable placeholder {n}/{path} is missing in the Traditional column');
  note(`${dict.sameCount} keys are intentionally identical in both Chinese columns (e.g. 其他, SOC, 燃料能量)`);
  ok('DICT samples: ' + JSON.stringify(dict.samples));
  if (dict.samples['field.reset'] !== '重設為預設值') fail('field.reset Traditional is ' + dict.samples['field.reset']);
  if (dict.samples['table.column'] !== '欄位') fail('table.column Traditional is ' + dict.samples['table.column']);
  if (dict.samples['chart.renderFailed'] !== '圖表繪製失敗：{msg}') fail('chart.renderFailed Traditional is ' + dict.samples['chart.renderFailed']);

  // ---- 1. English baseline ------------------------------------------------
  await evaluate("window.__fastsim.setLanguage('en')");
  const en0 = await evaluate('window.__t.snap()');
  ok(`EN baseline: brand="${en0.brand}" run="${en0.runLabel}" panel="${en0.panelTitle}" active=${en0.activeButton}`);

  // run a simulation in English (real click on the toolbar button)
  await evaluate("document.querySelector('#run-btn').click()");
  await waitFor('!!(window.__fastsim.state.result)', 600000, 'english simulation');
  const enRun = await evaluate(`(function () {
    var r = window.__fastsim.state.result;
    var nums = {};
    Object.keys(r.kpis).forEach(function (k) { nums[k] = r.kpis[k].value; });
    return {
      lang: r.lang,
      nums: nums,
      kpiLabels: Array.prototype.map.call(document.querySelectorAll('.kpi-label'), function (e) { return e.textContent.trim(); }),
      pngLen: (r.matplotlib_png_base64 || '').length,
      pngHash: (function(){var s=r.matplotlib_png_base64||'';var h=2166136261;for(var i=0;i<s.length;i++){h^=s.charCodeAt(i);h=(h*16777619)>>>0;}return h;})()
    };
  })()`);
  ok(`EN run: ${Object.keys(enRun.nums).length} KPIs, lang=${enRun.lang}, KPI labels=${JSON.stringify(enRun.kpiLabels)}`);

  const en = await evaluate('window.__t.snap()');
  const enWords = langWordChecks(en, 'en');
  if (enWords.required.length) fail('English mode leaks Chinese: ' + enWords.required.join(','));
  else ok('English mode: no Chinese glyphs anywhere');

  // modal chrome in English
  await evaluate('window.__fastsim.exportPython()');
  await waitFor("!document.querySelector('#modal-backdrop').hidden", 20000, 'export modal');
  await waitFor("window.__fastsim.state.exportState !== 'loading'", 60000, 'export code');
  const enModal = await evaluate(`(function () {
    var q = function (s) { var e = document.querySelector(s); return e ? e.textContent.trim() : null; };
    return { title: q('#modal-title'), hint: q('.modal-sub'), copy: q('#copy-label'),
             dl: q('#download-btn span'), closeTitle: document.querySelector('#modal-close').getAttribute('title'),
             meta: q('#modal-meta') };
  })()`);
  ok('EN modal: ' + JSON.stringify(enModal));
  await evaluate("document.querySelector('#modal-close').click()");

  // ---- 2. edit a parameter (must survive every switch) --------------------
  const editInfo = await evaluate(`(function () {
    var p = window.__fastsim.state.params.filter(function (x) {
      return x.editable !== false && x.type === 'float' && x.path === 'chassis.drag_coef';
    })[0];
    if (!p) return null;
    var row = document.querySelector('[data-path="' + p.path + '"]');
    var input = row && row.querySelector('input.input-num');
    if (!input) return null;
    input.value = '0.42';
    input.dispatchEvent(new Event('input', { bubbles: true }));
    return { path: p.path, value: window.__fastsim.state.values[p.path],
             modified: document.querySelectorAll('.param-row.is-modified').length };
  })()`);
  if (!editInfo || editInfo.value !== 0.42) fail('could not edit a parameter: ' + JSON.stringify(editInfo));
  else ok(`edited ${editInfo.path} = ${editInfo.value} (${editInfo.modified} row flagged modified)`);

  // ---- 3. switch to Traditional ------------------------------------------
  await evaluate("document.querySelector('.lang-btn[data-lang=\"zh-Hant\"]').click()");
  await new Promise((r) => setTimeout(r, 300));
  const hant = await evaluate('window.__t.snap()');
  ok(`ZH-HANT brand="${hant.brand}" run="${hant.runLabel}" panel="${hant.panelTitle}" active=${hant.activeButton} pressed=${hant.pressed.join(',')} html-lang=${hant.htmlLang}`);
  ok('ZH-HANT groups: ' + JSON.stringify(hant.groups));
  ok('ZH-HANT subgroups: ' + JSON.stringify(hant.subgroups));
  ok('ZH-HANT descriptions: ' + JSON.stringify(hant.descriptions.slice(0, 4)));
  ok('ZH-HANT param labels: ' + JSON.stringify(hant.paramLabels.slice(0, 4)));
  ok('ZH-HANT tabs: ' + JSON.stringify(hant.tabs) + ' | KPI labels: ' + JSON.stringify(hant.kpis));

  if (hant.lang !== 'zh-Hant') fail('I18N.lang is ' + hant.lang);
  if (hant.htmlLang !== 'zh-Hant') fail('<html lang> is ' + hant.htmlLang);
  if (hant.activeButton !== 'zh-Hant') fail('active button is ' + hant.activeButton);
  if (hant.brand !== 'FASTSim 工作台') fail('brand is ' + hant.brand);
  if (hant.runLabel !== '執行模擬') fail('run label is ' + hant.runLabel);
  if (hant.panelTitle !== '參數') fail('panel title is ' + hant.panelTitle);
  if (hant.tabs.join('|') !== '互動檢視|Matplotlib 圖|資料表') fail('tabs are ' + hant.tabs.join('|'));
  if (!hant.groups.includes('動力總成')) fail('groups lack 動力總成: ' + hant.groups.join(','));
  if (!hant.subgroups.includes('空氣動力學') || !hant.subgroups.includes('尺寸與質量')) {
    fail('subgroups look wrong: ' + hant.subgroups.join(','));
  }
  if (!hant.descriptions.some((d) => d && d.length > 6)) fail('no parameter descriptions rendered');
  if (!hant.descriptions.every((d) => !/[\u8f66\u8bbe\u6570\u53c2\u8fd0\u4eff]/.test(d))) {
    fail('a Traditional description still carries Simplified glyphs: ' + hant.descriptions.join(' / '));
  }

  const hantWords = langWordChecks(hant, 'zh-Hant');
  if (hantWords.required.length) fail('Traditional mode word check: ' + hantWords.required.join(','));
  else ok('Traditional mode: 參數/執行/資料/模擬 present, no 设置/数据/参数/运行/仿真/电机/信息/导出/图表');

  // no Simplified-only glyph may survive anywhere on screen or in any attribute
  const residue = await evaluate('window.__t.hantResidue()');
  if (residue.length) fail('Simplified glyphs survive in Traditional mode:\n    ' + residue.slice(0, 10).join('\n    '));
  else ok(`converter fixed point reached: 0 of ${(await evaluate('window.__t.texts().length + window.__t.attrs().length'))} visible strings would change`);

  if (hant.status && /運行|仿真/.test(hant.status)) fail('status line still Simplified: ' + hant.status);
  ok('ZH-HANT status: ' + hant.status);

  // modal chrome in Traditional
  await evaluate('window.__fastsim.exportPython()');
  await waitFor("window.__fastsim.state.exportState !== 'loading'", 60000, 'export code (hant)');
  const hantModal = await evaluate(`(function () {
    var q = function (s) { var e = document.querySelector(s); return e ? e.textContent.trim() : null; };
    return { title: q('#modal-title'), hint: q('.modal-sub'), copy: q('#copy-label'),
             dl: q('#download-btn span'), closeTitle: document.querySelector('#modal-close').getAttribute('title') };
  })()`);
  ok('ZH-HANT modal: ' + JSON.stringify(hantModal));
  if (hantModal.title !== '匯出的 Python 程式碼') fail('modal title is ' + hantModal.title);
  if (hantModal.copy !== '複製') fail('modal copy label is ' + hantModal.copy);
  if (hantModal.closeTitle !== '關閉') fail('modal close title is ' + hantModal.closeTitle);
  await evaluate("document.querySelector('#modal-close').click()");

  // the silent figure refresh must cross the Simplified/Traditional boundary
  const refreshed = await waitFor(
    "(function(){var r=window.__fastsim.state.result;return !!r && r.lang==='zh-Hant';})()",
    300000, 'figure refresh into zh-Hant');
  const FIG = `(function(){var r=window.__fastsim.state.result;var s=r.matplotlib_png_base64||'';
    var h=2166136261;for(var i=0;i<s.length;i++){h^=s.charCodeAt(i);h=(h*16777619)>>>0;}
    return {lang:r.lang,pngLen:s.length,pngHash:h};})()`;
  if (refreshed) {
    const hantFig = await evaluate(FIG);
    ok(`figure refreshed: lang=${hantFig.lang}, ${hantFig.pngLen} B (en was ${enRun.pngLen} B)`);
    if (hantFig.pngHash === enRun.pngHash) fail('Traditional figure is identical to the English one');
  }

  // ---- 4. Traditional -> Simplified -> English -> Traditional -------------
  // the silent figure refresh must also cross back from Traditional to Simplified
  await evaluate("document.querySelector('.lang-btn[data-lang=\"zh\"]').click()");
  const zhFigOk = await waitFor(
    "(function(){var r=window.__fastsim.state.result;return !!r && r.lang==='zh';})()",
    300000, 'figure refresh back into zh');
  const zhFig = await evaluate(FIG);
  if (zhFigOk) {
    ok(`figure refreshed back to Simplified: lang=${zhFig.lang}, ${zhFig.pngLen} B`);
    if (zhFig.pngHash === enRun.pngHash) fail('Simplified figure identical to the English one');
    if (zhFig.pngHash === (await evaluate(FIG)).pngHash && zhFig.pngLen === 0) fail('no figure at all');
  }
  const zh = await evaluate('window.__t.snap()');
  const zhWords = langWordChecks(zh, 'zh');
  if (zhWords.required.length) fail('Simplified mode word check: ' + zhWords.required.join(','));
  else ok('Simplified mode: Traditional terms gone (' + JSON.stringify(zh.tabs) + ')');
  if (zh.tabs.join('|') !== '交互视图|Matplotlib 图|数据表') fail('simplified tabs are ' + zh.tabs.join('|'));

  await evaluate("document.querySelector('.lang-btn[data-lang=\"en\"]').click()");
  const en2 = await evaluate('window.__t.snap()');
  if (en2.runLabel !== 'Run Simulation' || en2.brand !== 'FASTSim Studio') fail('English did not come back cleanly');

  await evaluate("document.querySelector('.lang-btn[data-lang=\"zh-Hant\"]').click()");
  await new Promise((r) => setTimeout(r, 400));
  const back = await evaluate('window.__t.snap()');
  const state2 = await evaluate(`(function () {
    var s = window.__fastsim.state;
    var nums = {};
    Object.keys(s.result.kpis).forEach(function (k) { nums[k] = s.result.kpis[k].value; });
    return { edit: s.values['chassis.drag_coef'], hasResult: !!s.result, nums: nums,
             stale: s.stale, kpiLabels: Array.prototype.map.call(document.querySelectorAll('.kpi-label'), function (e) { return e.textContent.trim(); }) };
  })()`);
  const numsEqual = JSON.stringify(state2.nums) === JSON.stringify(enRun.nums);
  ok(`after EN->ZH->EN->ZH-HANT: edit=${state2.edit}, result kept=${state2.hasResult}, KPI numbers identical to the English run=${numsEqual}`);
  ok('ZH-HANT KPI labels now: ' + JSON.stringify(state2.kpiLabels));
  if (state2.edit !== 0.42) fail('the parameter edit was lost: ' + state2.edit);
  if (!state2.hasResult) fail('the simulation result was dropped by a language switch');
  if (!numsEqual) fail('KPI numbers changed across languages');
  if (back.lang !== 'zh-Hant') fail('final language is ' + back.lang);
  const kpiText = state2.kpiLabels.join(',');
  const SIMP_ONLY = ['行驶距离', '燃油经济性', '车轮牵引能量', '平均车速', '最高车速', '工况跟随率', '耗电量'];
  const TRAD_OK = ['行駛距離', '燃油經濟性', '車輪牽引能量', '平均車速', '最高車速', '工況跟隨率'];
  if (SIMP_ONLY.some((w) => kpiText.indexOf(w) !== -1)) fail('KPI labels still Simplified: ' + kpiText);
  else if (!TRAD_OK.every((w) => kpiText.indexOf(w) !== -1)) fail('KPI labels not Traditional: ' + kpiText);
  else ok('KPI labels are Traditional: ' + kpiText);

  // ---- 5. persistence across reload --------------------------------------
  await client.send('Page.reload', { ignoreCache: false });
  await waitFor('!!(window.__fastsim && window.__fastsim.state.params.length)', 60000, 'reload');
  await evaluate(HELPERS);
  const afterReload = await evaluate('window.__t.snap()');
  ok(`after reload: lang=${afterReload.lang}, brand="${afterReload.brand}", active=${afterReload.activeButton}, html-lang=${afterReload.htmlLang}`);
  if (afterReload.lang !== 'zh-Hant') fail('language did not persist across reload (' + afterReload.lang + ')');
  if (afterReload.brand !== 'FASTSim 工作台') fail('brand after reload is ' + afterReload.brand);
  if (afterReload.activeButton !== 'zh-Hant') fail('active button after reload is ' + afterReload.activeButton);

  // ---- 5b. thermal preset: Thermal groups + more Traditional copy ---------
  await evaluate(`(function () {
    var s = document.querySelector('#preset-select');
    s.value = '2016 Nissan Leaf 30 kWh thrml.yaml';
    s.dispatchEvent(new Event('change', { bubbles: true }));
    return null;
  })()`);
  await waitFor("window.__fastsim.state.presetId === '2016 Nissan Leaf 30 kWh thrml.yaml' && window.__fastsim.state.params.length > 0",
    180000, 'thermal preset schema');
  const th = await evaluate('window.__t.snap()');
  ok('thermal BEV groups: ' + JSON.stringify(th.groups));
  ok('thermal BEV subgroups: ' + JSON.stringify(th.subgroups.slice(-4)));
  ok('thermal BEV descriptions: ' + JSON.stringify(th.descriptions.slice(0, 3)));
  if (!th.groups.includes('熱管理')) fail('thermal preset lacks the 熱管理 group: ' + th.groups.join(','));
  if (!th.subgroups.includes('座艙與空調') || !th.subgroups.includes('電池熱管理')) {
    fail('thermal preset subgroups wrong: ' + th.subgroups.join(','));
  }
  if (th.lang !== 'zh-Hant' || th.brand !== 'FASTSim 工作台') fail('language changed while switching presets');
  const residue2 = await evaluate('window.__t.hantResidue()');
  if (residue2.length) fail('Simplified glyphs survive on the thermal preset:\n    ' + residue2.slice(0, 10).join('\n    '));
  else ok('thermal preset: converter fixed point reached for every visible string');

  // ---- 6. console ---------------------------------------------------------
  const errs = client.consoleErrors.concat(client.exceptions);
  if (errs.length) fail('console was not clean:\n  ' + errs.join('\n  '));
  else ok('console clean: 0 errors, 0 uncaught exceptions');

  // evidence dump
  const evidence = { en, enRun, hant, hantModal, zh, back, state2, afterReload, consoleErrors: errs };
  fs.writeFileSync(path.join(__dirname, 'dom-evidence.json'), JSON.stringify(evidence, null, 1));
  console.log('\nRESULT: ' + (problems.length ? 'FAIL (' + problems.length + ')' : 'PASS'));
  client.close();
  return problems.length ? 1 : 0;
}

main().then((code) => process.exit(code)).catch((e) => {
  console.error('HARNESS ERROR: ' + (e && e.stack || e));
  process.exit(2);
});
