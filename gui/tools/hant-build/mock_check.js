/* Mock-mode check: ?mock=1 serves pre-baked Simplified fixtures, so this proves
 * the client-side conversion also covers the offline fixture path.
 *   node mock_check.js <baseUrl> <debugPort>
 */
'use strict';

const http = require('http');

const BASE = process.argv[2] || 'http://127.0.0.1:8021';
const PORT = process.argv[3] || '9333';

function httpJson(url) {
  return new Promise((resolve, reject) => {
    http.get(url, (res) => {
      let body = '';
      res.on('data', (c) => { body += c; });
      res.on('end', () => { try { resolve(JSON.parse(body)); } catch (e) { reject(e); } });
    }).on('error', reject);
  });
}

function connect(url) {
  return new Promise((resolve, reject) => {
    const ws = new WebSocket(url);
    let id = 0;
    const pending = new Map();
    const errors = [];
    const client = {
      errors,
      send(method, params) {
        return new Promise((res, rej) => {
          const i = ++id;
          pending.set(i, (m) => (m.error ? rej(new Error(method + ' ' + JSON.stringify(m.error))) : res(m.result)));
          ws.send(JSON.stringify({ id: i, method, params: params || {} }));
        });
      },
      close() { try { ws.close(); } catch (e) { /* ignore */ } }
    };
    ws.onopen = () => resolve(client);
    ws.onerror = (e) => reject(new Error('ws: ' + e.message));
    ws.onmessage = (ev) => {
      const m = JSON.parse(ev.data);
      if (m.id && pending.has(m.id)) { pending.get(m.id)(m); pending.delete(m.id); return; }
      if (m.method === 'Runtime.exceptionThrown') {
        const d = m.params.exceptionDetails || {};
        errors.push((d.exception && (d.exception.description || d.exception.value)) || d.text);
      }
      if (m.method === 'Runtime.consoleAPICalled' && m.params.type === 'error') {
        errors.push((m.params.args || []).map((a) => a.value || a.description).join(' '));
      }
    };
  });
}

const PROBE = String.raw`
(function () {
  function visibleTexts() {
    var out = [];
    var w = document.createTreeWalker(document.body, NodeFilter.SHOW_TEXT, null);
    var n;
    while ((n = w.nextNode())) {
      var el = n.parentElement;
      if (!el || el.closest('.lang-btn')) continue;
      var tag = el.tagName;
      if (tag === 'SCRIPT' || tag === 'STYLE' || tag === 'OPTION') continue;
      var cs = getComputedStyle(el);
      if (cs.display === 'none' || cs.visibility === 'hidden') continue;
      var t = (n.textContent || '').trim();
      if (t) out.push(t);
    }
    return out;
  }
  var texts = visibleTexts();
  var all = texts.join(' \u0001 ');
  var residue = [];
  if (window.FSHant) {
    texts.forEach(function (t) { var c = window.FSHant.toHant(t); if (c !== t) residue.push(t + ' -> ' + c); });
  }
  return {
    lang: window.FSI18N.lang,
    mock: window.__fastsim.state.mock,
    brand: (document.querySelector('.brand-text h1') || {}).textContent,
    tabs: Array.prototype.map.call(document.querySelectorAll('.tab'), function (e) { return e.textContent.trim(); }),
    kpiLabels: Array.prototype.map.call(document.querySelectorAll('.kpi-label'), function (e) { return e.textContent.trim(); }),
    groups: Array.prototype.slice.call(document.querySelectorAll('.group-name'), 0, 5).map(function (e) { return e.textContent.trim(); }),
    residue: residue,
    leaks: ['设置', '数据', '参数', '运行', '仿真', '电机', '信息'].filter(function (w) { return all.indexOf(w) !== -1; })
  };
})()
`;

async function main() {
  const targets = await httpJson(`http://127.0.0.1:${PORT}/json/list`);
  const page = targets.find((t) => t.type === 'page');
  const client = await connect(page.webSocketDebuggerUrl);
  await client.send('Page.enable');
  await client.send('Runtime.enable');
  const evaluate = async (expr) => {
    const r = await client.send('Runtime.evaluate', { expression: expr, returnByValue: true, userGesture: true });
    if (r.exceptionDetails) throw new Error('eval threw: ' + JSON.stringify(r.exceptionDetails.exception || r.exceptionDetails.text));
    return r.result.value;
  };

  await client.send('Page.navigate', { url: BASE + '/?mock=1' });
  const deadline = Date.now() + 120000;
  while (Date.now() < deadline) {
    if (await evaluate('!!(window.__fastsim && window.__fastsim.state.mock && window.__fastsim.state.params.length)')) break;
    await new Promise((r) => setTimeout(r, 250));
  }
  await evaluate("document.querySelector('.lang-btn[data-lang=\"zh-Hant\"]').click()");
  await evaluate("window.__fastsim.runSimulation()");
  let ok = false;
  const deadline2 = Date.now() + 120000;
  while (Date.now() < deadline2) {
    if (await evaluate('!!window.__fastsim.state.result')) { ok = true; break; }
    await new Promise((r) => setTimeout(r, 250));
  }
  const probe = await evaluate(PROBE);
  console.log(JSON.stringify({ simulated: ok, ...probe }, null, 1));
  const problems = [];
  if (!probe.mock) problems.push('mock flag is not set');
  if (probe.lang !== 'zh-Hant') problems.push('lang is ' + probe.lang);
  if (probe.tabs.join('|') !== '互動檢視|Matplotlib 圖|資料表') problems.push('tabs: ' + probe.tabs.join('|'));
  if (probe.residue.length) problems.push('simplified residue: ' + probe.residue.slice(0, 5).join(' | '));
  if (probe.leaks.length) problems.push('simplified words leak: ' + probe.leaks.join(','));
  if (!probe.kpiLabels.length) problems.push('no KPI labels after the mock run');
  if (probe.kpiLabels.some((l) => ['行驶距离', '燃油经济性', '车轮牵引能量', '工况跟随率'].indexOf(l) !== -1)) {
    problems.push('KPI labels still Simplified: ' + probe.kpiLabels.join(','));
  }
  if (client.errors.length) problems.push('console errors: ' + client.errors.join(' | '));
  console.log('RESULT: ' + (problems.length ? 'FAIL ' + JSON.stringify(problems) : 'PASS'));
  client.close();
  return problems.length ? 1 : 0;
}

main().then((c) => process.exit(c)).catch((e) => { console.error('HARNESS ERROR: ' + (e && e.stack || e)); process.exit(2); });
