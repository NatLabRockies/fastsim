/* Unit checks for i18n.js / hant.js in a bare Node context (no DOM). */
'use strict';

const fs = require('fs');
const path = require('path');

const FRONTEND = path.join(__dirname, '..', 'frontend');
const problems = [];
function check(name, got, want) {
  const ok = JSON.stringify(got) === JSON.stringify(want);
  console.log((ok ? 'ok    ' : 'FAIL  ') + name + (ok ? '' : `  got ${JSON.stringify(got)} want ${JSON.stringify(want)}`));
  if (!ok) problems.push(name);
}

function makeWindow(initial) {
  const store = Object.assign({}, initial);
  const win = {
    localStorage: {
      getItem: (k) => (k in store ? store[k] : null),
      setItem: (k, v) => { store[k] = String(v); },
    },
    document: { documentElement: {}, title: '' },
    __store: store,
  };
  win.window = win;
  return win;
}

function load(win) {
  for (const file of ['hant.js', 'i18n.js']) {
    const src = fs.readFileSync(path.join(FRONTEND, file), 'utf-8');
    new Function('window', src)(win);
  }
  return win;
}

// --- 1. defaults and normalisation ----------------------------------------
let win = load(makeWindow({}));
const I = win.FSI18N;
check('empty storage -> en', I.readStored(), 'en');
check('default lang is en', I.lang, 'en');
check('unknown stored value -> en', (win.__store['fastsim.lang'] = 'klingon', I.readStored()), 'en');
check("normalize('zh-TW')", I.normalize('zh-TW'), 'zh-Hant');
check("normalize('zh_HK')", I.normalize('zh_HK'), 'zh-Hant');
check("normalize('zh-Hant')", I.normalize('zh-Hant'), 'zh-Hant');
check("normalize('zh-CN')", I.normalize('zh-CN'), 'zh');
check("normalize('zh')", I.normalize('zh'), 'zh');
check("normalize('zh-Hans-CN')", I.normalize('zh-Hans-CN'), 'zh');
check("normalize('zh-SG')", I.normalize('zh-SG'), 'zh');
check("normalize('zh-Hant-TW')", I.normalize('zh-Hant-TW'), 'zh-Hant');
check("normalize('klingon')", I.normalize('klingon'), 'en');
check("normalize('en-US')", I.normalize('en-US'), 'en');
check("normalize('')", I.normalize(''), 'en');
check('normalize(null)', I.normalize(null), 'en');
check('normalize(42)', I.normalize(42), 'en');

// --- 2. column selection --------------------------------------------------
I.setLang('en');
check('en column', I.t('panel.title'), 'Parameters');
I.setLang('zh');
check('zh column', I.t('panel.title'), '参数');
check('isZh', [I.isZh(), I.isHant(), I.isZhLike()], [true, false, true]);
I.setLang('zh-Hant');
check('zh-Hant column', I.t('panel.title'), '參數');
check('isHant', [I.isZh(), I.isHant(), I.isZhLike()], [false, true, true]);
check('document title follows the language', win.document.title, 'FASTSim 工作台');
check('<html lang>', win.document.documentElement.lang, 'zh-Hant');
I.setLang('zh');
check('<html lang> zh', win.document.documentElement.lang, 'zh-CN');
I.setLang('en');
check('<html lang> en', win.document.documentElement.lang, 'en');

// --- 3. placeholders survive ---------------------------------------------
I.setLang('zh-Hant');
check('placeholder {n}', I.t('panel.modifiedCount', { n: 3 }), '3 項已修改');
check('placeholder {path}', I.t('err.unknownPath', { path: 'a.b' }).indexOf('a.b') > 0, true);
check('escaped braces', I.t('field.keys', { n: 2 }), '{{2 個鍵}}');

// --- 4. persistence -------------------------------------------------------
I.setLang('zh-Hant');
check('persisted value', win.__store['fastsim.lang'], 'zh-Hant');
check('readStored after setLang', I.readStored(), 'zh-Hant');
win = load(makeWindow({ 'fastsim.lang': 'zh-TW' }));
check('stored zh-TW boots as zh-Hant', win.FSI18N.readStored(), 'zh-Hant');

// --- 5. API text conversion (pick / zhText) -------------------------------
I.setLang('zh-Hant');
const obj = { label_en: 'Distance', label_zh: '行驶距离', description: 'Aerodynamic drag coefficient.',
              description_zh: '空气阻力系数。', group_zh: '动力总成' };
check('pick label_zh -> Traditional', I.pick(obj, 'label_en', 'label_zh', 'x'), '行駛距離');
check('pick description -> Traditional', I.pick(obj, 'description', 'description_zh', ''), '空氣阻力係數。');check('zhText group', I.zhText(obj.group_zh), '動力總成');
check('name_zh cycle', I.pick({ name: 'UDDS (City)', name_zh: 'UDDS（城市）' }, 'name', 'name_zh', ''), 'UDDS（城市）');
I.setLang('zh');
check('pick label_zh stays Simplified for zh', I.pick(obj, 'label_en', 'label_zh', 'x'), '行驶距离');
I.setLang('en');
check('pick english for en', I.pick(obj, 'label_en', 'label_zh', 'x'), 'Distance');
check('zhText is a no-op in en', I.zhText(obj.group_zh), '动力总成');

// --- 6. converter sanity ---------------------------------------------------
check('hant.js tables loaded', win.FSHant.charCount > 100 && win.FSHant.phraseCount > 20, true);
check('toHant(参数/数据)', [win.FSHant.toHant('参数'), win.FSHant.toHant('数据')], ['參數', '資料']);
check('toHant leaves emoji/units alone', win.FSHant.toHant('SOC [%] · 12.5 kW'), 'SOC [%] · 12.5 kW');
check('toHant empty', win.FSHant.toHant(''), '');

// --- 7. no missing columns ------------------------------------------------
const keys = Object.keys(I.DICT);
check('every key has 3 columns', keys.filter((k) => I.DICT[k].length !== 3), []);
check('DICT size', keys.length, 133);

console.log('\nRESULT: ' + (problems.length ? 'FAIL (' + problems.length + ')' : 'PASS'));
process.exit(problems.length ? 1 : 0);
