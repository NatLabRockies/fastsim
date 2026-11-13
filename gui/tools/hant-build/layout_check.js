/* Layout probe: the three-button language switch must not overflow the toolbar. */
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

async function main() {
  const targets = await httpJson(`http://127.0.0.1:${PORT}/json/list`);
  const page = targets.find((t) => t.type === 'page');
  const ws = new WebSocket(page.webSocketDebuggerUrl);
  let id = 0;
  const pending = new Map();
  ws.onmessage = (ev) => {
    const m = JSON.parse(ev.data);
    if (m.id && pending.has(m.id)) { pending.get(m.id)(m); pending.delete(m.id); }
  };
  await new Promise((r) => { ws.onopen = r; });
  const send = (method, params) => new Promise((res, rej) => {
    const i = ++id;
    pending.set(i, (m) => (m.error ? rej(new Error(method)) : res(m.result)));
    ws.send(JSON.stringify({ id: i, method, params: params || {} }));
  });
  const evaluate = async (expr) => {
    const r = await send('Runtime.evaluate', { expression: expr, returnByValue: true });
    if (r.exceptionDetails) throw new Error(JSON.stringify(r.exceptionDetails));
    return r.result.value;
  };

  await send('Page.enable');
  await send('Runtime.enable');
  const problems = [];
  for (const width of [1440, 1180, 900, 480]) {
    await send('Emulation.setDeviceMetricsOverride', { width, height: 900, deviceScaleFactor: 1, mobile: false });
    for (const lang of ['en', 'zh-Hant', 'zh']) {
      await evaluate(`window.__fastsim.setLanguage('${lang}')`);
      await new Promise((r) => setTimeout(r, 120));
      const probe = await evaluate(`(function () {
        var sw = document.querySelector('.lang-switch');
        var wrap = sw ? sw.getBoundingClientRect() : null;
        var bar = document.querySelector('.topbar').getBoundingClientRect();
        var btns = Array.prototype.map.call(document.querySelectorAll('.lang-btn'), function (b) {
          var r = b.getBoundingClientRect();
          return { label: b.textContent.trim(), w: Math.round(r.width), h: Math.round(r.height),
                   clipped: b.scrollWidth > b.clientWidth + 1 };
        });
        return {
          docOverflow: document.documentElement.scrollWidth - document.documentElement.clientWidth,
          insideBar: wrap ? (wrap.left >= bar.left - 1 && wrap.right <= bar.right + 1) : null,
          btns: btns
        };
      })()`);
      const clipped = probe.btns.filter((b) => b.clipped).map((b) => b.label);
      console.log(`${width}px ${lang.padEnd(7)} overflow=${probe.docOverflow} insideToolbar=${probe.insideBar} buttons=${JSON.stringify(probe.btns.map((b) => b.w))} clipped=${JSON.stringify(clipped)}`);
      if (probe.docOverflow > 0) problems.push(`${width}px/${lang}: page overflows by ${probe.docOverflow}px`);
      if (probe.insideBar === false) problems.push(`${width}px/${lang}: switch outside the toolbar box`);
      if (clipped.length) problems.push(`${width}px/${lang}: clipped labels ${clipped.join(',')}`);
    }
  }
  console.log('RESULT: ' + (problems.length ? 'FAIL ' + JSON.stringify(problems) : 'PASS'));
  ws.close();
  return problems.length ? 1 : 0;
}

main().then((c) => process.exit(c)).catch((e) => { console.error(e); process.exit(2); });
