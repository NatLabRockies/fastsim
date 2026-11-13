/* ==========================================================================
   i18n.js — single multi-language dictionary + static/applier helpers.
   Every visible string in the UI resolves through here (or through the
   *_zh fields the API supplies). Adding a language = adding one column.

   Languages (and the column each one reads):
     en        index 0   English (default)
     zh-Hant   index 2   繁體中文  (Taiwan / Hong Kong usage)
     zh        index 1   简体中文

   API-supplied Chinese (`description_zh`, `group_zh`, `subgroup_zh`,
   `label_zh`, `name_zh`) is Simplified only; in 繁體 mode it is converted by
   hant.js (see FSHant.toHant), so the backend payload never has to change.
   Depends on: /hant.js (optional — conversion degrades to a no-op without it).
   ========================================================================== */
(function (global) {
  'use strict';

  var LS_KEY = 'fastsim.lang';

  /* key: [english, 简体中文, 繁體中文] */
  var DICT = {
    'app.title': ['FASTSim Studio', 'FASTSim 工作台', 'FASTSim 工作台'],
    'app.subtitle': ['Vehicle Simulation Workbench', '车辆仿真工作台', '車輛模擬工作台'],
    'app.docTitle': ['FASTSim Studio', 'FASTSim 工作台', 'FASTSim 工作台'],

    'toolbar.preset': ['Vehicle Preset', '车型预设', '車型預設'],
    'toolbar.cycle': ['Drive Cycle', '行驶工况', '行駛工況'],
    'toolbar.run': ['Run Simulation', '运行仿真', '執行模擬'],
    'toolbar.running': ['Simulating…', '仿真中…', '模擬中…'],
    'toolbar.export': ['Export Python Code', '导出 Python 代码', '匯出 Python 程式碼'],
    'toolbar.langLabel': ['Language', '语言', '語言'],
    'toolbar.collapse': ['Collapse parameter panel', '收起参数面板', '收起參數面板'],
    'toolbar.expand': ['Expand parameter panel', '展开参数面板', '展開參數面板'],
    'toolbar.zenOn': ['Hide parameters', '隐藏参数面板', '隱藏參數面板'],
    'toolbar.zenOff': ['Show parameters', '显示参数面板', '顯示參數面板'],

    'panel.title': ['Parameters', '参数', '參數'],
    'panel.search': ['Search parameters…', '搜索参数…', '搜尋參數…'],
    'panel.modifiedOnly': ['Modified only', '仅已修改', '僅已修改'],
    'panel.resetAll': ['Reset all', '全部重置', '全部重設'],
    'panel.resetAllConfirm': [
      'Discard all parameter changes for this preset and restore defaults?',
      '放弃该预设下的所有参数修改并恢复默认值？',
      '放棄該預設下的所有參數修改並恢復預設值？'
    ],
    'panel.modifiedCount': ['{n} modified', '{n} 项已修改', '{n} 項已修改'],
    'panel.changedInGroup': ['{n} modified in this group', '本组 {n} 项已修改', '本組 {n} 項已修改'],
    'panel.noMatch': ['No parameters match your filters.', '没有符合筛选条件的参数。', '沒有符合篩選條件的參數。'],
    'panel.loading': ['Loading parameter schema…', '正在加载参数结构…', '正在載入參數結構…'],
    'panel.loadFailed': ['Parameter schema unavailable.', '参数结构不可用。', '參數結構不可用。'],

    'field.reset': ['Reset to default', '重置为默认值', '重設為預設值'],
    'field.readonly': [
      'Structural value — shown for reference only, it cannot be overridden.',
      '结构性参数——仅作展示，不可覆盖。',
      '結構性參數——僅供顯示，不可覆寫。'
    ],
    'field.modified': ['Modified', '已修改', '已修改'],
    'field.default': ['default {v}', '默认 {v}', '預設 {v}'],
    'field.emptyValue': ['—', '—', '—'],
    'field.items': ['[{n} items]', '[{n} 项]', '[{n} 項]'],
    'field.keys': ['{{{n} keys}}', '{{{n} 个键}}', '{{{n} 個鍵}}'],

    'group.Vehicle': ['Vehicle', '整车', '整車'],
    'group.Chassis': ['Chassis', '底盘', '底盤'],
    'group.Powertrain': ['Powertrain', '动力总成', '動力總成'],
    'group.Thermal': ['Thermal', '热管理', '熱管理'],
    'group.Other': ['Other', '其他', '其他'],

    'subgroup.Body': ['Body', '车身', '車身'],
    'subgroup.Aero': ['Aero', '空气动力学', '空氣動力學'],
    'subgroup.Rolling & Traction': ['Rolling & Traction', '滚动与牵引', '滾動與牽引'],
    'subgroup.Wheels': ['Wheels', '车轮', '車輪'],
    'subgroup.Dimensions & Mass': ['Dimensions & Mass', '尺寸与质量', '尺寸與質量'],
    'subgroup.Fuel Storage': ['Fuel Storage', '燃料储存', '燃料儲存'],
    'subgroup.Fuel Converter': ['Fuel Converter', '燃料转换器', '燃料轉換器'],
    'subgroup.Electric Machine': ['Electric Machine', '电机', '電機'],
    'subgroup.Battery (ESS)': ['Battery (ESS)', '电池（储能）', '電池（儲能）'],
    'subgroup.Transmission': ['Transmission', '传动系统', '傳動系統'],
    'subgroup.Controls': ['Controls', '控制参数', '控制參數'],
    'subgroup.Cabin & HVAC': ['Cabin & HVAC', '座舱与空调', '座艙與空調'],
    'subgroup.Battery Thermal': ['Battery Thermal', '电池热管理', '電池熱管理'],
    'subgroup.Other': ['Other', '其他', '其他'],

    'empty.title': ['Nothing simulated yet', '尚未运行仿真', '尚未執行模擬'],
    'empty.body': [
      'Pick a vehicle preset and a drive cycle in the top bar, then press Run Simulation.',
      '在顶部选择车型预设与行驶工况，然后点击「运行仿真」。',
      '在頂部選擇車型預設與行駛工況，然後點選「執行模擬」。'
    ],
    'loading.title': ['Running FASTSim…', '正在运行 FASTSim…', '正在執行 FASTSim…'],
    'loading.body': [
      'Driving the cycle through the powertrain model.',
      '正在将行驶工况输入动力总成模型。',
      '正在將行駛工況輸入動力總成模型。'
    ],

    'tab.interactive': ['Interactive View', '交互视图', '互動檢視'],
    'tab.figure': ['Matplotlib Figure', 'Matplotlib 图', 'Matplotlib 圖'],
    'tab.table': ['Data Table', '数据表', '資料表'],

    'kpi.empty': ['No KPI values were returned for this run.', '本次运行未返回 KPI 数值。', '本次執行未回傳 KPI 數值。'],

    'chart.speed': ['Vehicle Speed', '车速', '車速'],
    'chart.power': ['Powertrain Power Split', '动力总成功率分配', '動力總成功率分配'],
    'chart.soc': ['Battery State of Charge', '电池荷电状态', '電池電量狀態'],
    'chart.distance': ['Cumulative Distance', '累计行驶距离', '累計行駛距離'],
    'chart.energy': ['Cumulative Fuel & Energy', '累计燃料与能量', '累計燃料與能量'],
    'chart.na': ['No data for this powertrain', '该动力总成无此数据', '該動力總成無此資料'],
    'chart.renderFailed': ['Chart could not be drawn: {msg}', '图表绘制失败：{msg}', '圖表繪製失敗：{msg}'],
    'chart.plotlyMissing': [
      'The local Plotly bundle (/vendor/plotly.min.js) could not be loaded.',
      '本地 Plotly 库（/vendor/plotly.min.js）加载失败。',
      '本地 Plotly 庫（/vendor/plotly.min.js）載入失敗。'
    ],
    'chart.noSeries': ['The server returned no timeseries for this run.', '本次运行未返回时间序列数据。', '本次執行未回傳時間序列資料。'],

    'axis.time': ['Time [s]', '时间 [s]', '時間 [s]'],
    'axis.speed': ['Speed [m/s]', '车速 [m/s]', '車速 [m/s]'],
    'axis.power': ['Power [kW]', '功率 [kW]', '功率 [kW]'],
    'axis.soc': ['SOC [%]', 'SOC [%]', 'SOC [%]'],
    'axis.energy': ['Energy [kWh]', '能量 [kWh]', '能量 [kWh]'],
    'axis.distance': ['Distance [mi]', '距离 [mi]', '距離 [mi]'],

    'trace.target': ['Target speed', '目标车速', '目標車速'],
    'trace.achieved': ['Achieved speed', '实际车速', '實際車速'],
    'trace.engine': ['Engine', '发动机', '發動機'],
    'trace.motor': ['Motor', '电机', '電機'],
    'trace.battery': ['Battery', '电池', '電池'],
    'trace.fuel': ['Fuel energy', '燃料能量', '燃料能量'],
    'trace.energy': ['Total energy', '总能量', '總能量'],
    'trace.soc': ['SOC', 'SOC', 'SOC'],
    'trace.distance': ['Distance', '距离', '距離'],
    'trace.grade': ['Grade', '坡度', '坡度'],
    'trace.fuelPower': ['Fuel power', '燃料功率', '燃料功率'],

    'figure.downloadPng': ['Download PNG', '下载 PNG', '下載 PNG'],
    'figure.downloadPngAria': ['Download the figure as a high-resolution PNG', '将图像下载为高分辨率 PNG', '將圖像下載為高解析度 PNG'],
    'figure.svg': ['Download SVG', '下载 SVG', '下載 SVG'],
    'figure.svgAria': ['Download the figure as a scalable vector SVG', '将图像下载为可缩放矢量 SVG', '將圖像下載為可縮放向量 SVG'],
    'figure.svgUnavailable': [
      'This run returned a PNG only — SVG export is not offered for it.',
      '本次运行仅返回 PNG，无法提供 SVG 导出。',
      '本次執行僅回傳 PNG，無法提供 SVG 匯出。'
    ],
    'figure.missing': [
      'The server did not return a figure for this run.',
      '本次运行服务器未返回图像。',
      '本次執行伺服器未回傳圖像。'
    ],
    'figure.alt': ['FASTSim simulation figure', 'FASTSim 仿真图', 'FASTSim 模擬圖'],
    'figure.meta': ['{w}×{h} px · PNG', '{w}×{h} px · PNG', '{w}×{h} px · PNG'],
    'figure.metaBoth': ['{w}×{h} px · PNG + SVG', '{w}×{h} px · PNG + SVG', '{w}×{h} px · PNG + SVG'],
    'figure.metaSvgOnly': ['vector SVG', '矢量 SVG', '向量 SVG'],

    'table.empty': ['No timeseries returned for this run.', '本次运行未返回时间序列。', '本次執行未回傳時間序列。'],
    'table.note': ['All {n} time steps.', '共 {n} 个时间步。', '共 {n} 個時間步。'],
    'table.noteDown': [
      'Showing every {k}-th step ({n} of {m}) to keep the table responsive.',
      '为保持流畅，每隔 {k} 步显示 1 步（{n} / {m}）。',
      '為保持流暢，每隔 {k} 步顯示 1 步（{n} / {m}）。'
    ],
    'table.column': ['Column', '列', '欄位'],

    'export.title': ['Exported Python', '导出的 Python 代码', '匯出的 Python 程式碼'],
    'export.hint': [
      'A self-contained script that reproduces this exact configuration.',
      '可复现当前配置的独立脚本。',
      '可重現此設定的獨立腳本。'
    ],
    'export.copy': ['Copy', '复制', '複製'],
    'export.copied': ['Copied', '已复制', '已複製'],
    'export.download': ['Download .py', '下载 .py', '下載 .py'],
    'export.close': ['Close', '关闭', '關閉'],
    'export.loading': ['Generating code…', '正在生成代码…', '正在生成程式碼…'],
    'export.error': ['Could not generate code: {msg}', '无法生成代码：{msg}', '無法生成程式碼：{msg}'],
    'export.empty': [
      'The server returned no code.',
      '服务器未返回代码。',
      '伺服器未回傳程式碼。'
    ],
    'export.meta': ['{lines} lines · {kb} KB', '{lines} 行 · {kb} KB', '{lines} 行 · {kb} KB'],
    'export.fileName': ['fastsim_export.py', 'fastsim_export.py', 'fastsim_export.py'],

    'err.sim': ['Simulation failed', '仿真失败', '模擬失敗'],
    'err.export': ['Export failed', '导出失败', '匯出失敗'],
    'err.schema': ['Could not load the parameter schema', '无法加载参数结构', '無法載入參數結構'],
    'err.missing': ['Select a vehicle preset and a drive cycle first.', '请先选择车型预设与行驶工况。', '請先選擇車型預設與行駛工況。'],
    'err.network': [
      'Could not reach the server. Is the backend running on this origin?',
      '无法连接服务器，后端是否已在本源运行？',
      '無法連線伺服器，後端是否已在此來源執行？'
    ],
    'err.badJson': ['The server response was not valid JSON.', '服务器响应不是有效的 JSON。', '伺服器回應不是有效的 JSON。'],
    'err.http': ['HTTP {status} {statusText}', 'HTTP {status} {statusText}', 'HTTP {status} {statusText}'],
    'err.presetMismatch': [
      'The server returned parameters for a different preset ({got}) than requested ({want}). The panel was not updated.',
      '服务器返回的预设（{got}）与请求（{want}）不一致，参数面板未更新。',
      '伺服器回傳的預設（{got}）與請求（{want}）不一致，參數面板未更新。'
    ],
    'err.unknownPath': [
      'The server rejected an unknown parameter path: {path}. That override was removed — run again.',
      '服务器拒绝了未知参数路径：{path}。该覆盖已被移除——请重新运行。',
      '伺服器拒絕了未知參數路徑：{path}。該覆寫已被移除——請重新執行。'
    ],
    'err.unknownPathGeneric': [
      'The server rejected an unknown parameter path. The offending override was removed — run again.',
      '服务器拒绝了未知参数路径，相关覆盖已被移除——请重新运行。',
      '伺服器拒絕了未知參數路徑，相關覆寫已被移除——請重新執行。'
    ],
    'err.retry': ['Retry', '重试', '重試'],
    'err.dismiss': ['Dismiss', '关闭', '關閉'],
    'err.useMock': ['Use mock data', '使用模拟数据', '使用模擬資料'],

    'status.ready': ['Ready', '就绪', '就緒'],
    'status.busy': ['Running simulation…', '正在运行仿真…', '正在執行模擬…'],
    'status.done': ['Simulation complete', '仿真完成', '模擬完成'],
    'status.doneDetail': [
      '{preset} · {cycle} · {n} steps · {ms} ms',
      '{preset} · {cycle} · {n} 步 · {ms} ms',
      '{preset} · {cycle} · {n} 步 · {ms} ms'
    ],
    'status.overrides': ['{n} parameter overrides applied', '已应用 {n} 项参数覆盖', '已套用 {n} 項參數覆寫'],
    'status.overrideOne': ['1 parameter override applied', '已应用 1 项参数覆盖', '已套用 1 項參數覆寫'],
    'status.noOverrides': ['no overrides', '无参数覆盖', '無參數覆寫'],
    'status.exporting': ['Generating Python…', '正在生成 Python 代码…', '正在生成 Python 程式碼…'],

    'stale.badge': ['Out of date — press Run again', '结果已过期——请重新运行', '結果已過期——請重新執行'],
    'mock.badge': ['MOCK DATA', '模拟数据', '模擬資料'],
    'mock.note': [
      'Mock mode (?mock=1): responses come from mock/*.json, not the backend.',
      '模拟模式（?mock=1）：响应来自 mock/*.json，而非后端。',
      '模擬模式（?mock=1）：回應來自 mock/*.json，而非後端。'
    ],
    'mock.bannerTitle': ['Mock mode is on', '模拟模式已开启', '模擬模式已開啟'],

  };

  var lang = 'en';

  /* language tag -> dictionary column */
  var LANGS = ['en', 'zh-Hant', 'zh'];
  var COLUMN = { 'en': 0, 'zh-Hant': 2, 'zh': 1 };
  var HTML_LANG = { 'en': 'en', 'zh-Hant': 'zh-Hant', 'zh': 'zh-CN' };

  /** Accept 'zh-Hant', 'zh-TW', 'zh_HK', 'zh-CN', 'cn', 'en-US' … -> canonical tag. */
  function normalize(l) {
    if (l == null) return 'en';
    var v = String(l).trim().toLowerCase().replace(/_/g, '-');
    if (!v) return 'en';
    if (v === 'tw' || v === 'hk' || v === 'mo' || v === 'zh-hant' || v === 'zh-hant-tw'
        || v === 'zh-hant-hk') {
      return 'zh-Hant';
    }
    if (v.indexOf('zh') === 0 || v === 'cn') {
      /* 'zh' + any Taiwan / Hong Kong / Macau region or script tag */
      if (v.indexOf('zh-hant') === 0 || v.indexOf('zh-tw') === 0
          || v.indexOf('zh-hk') === 0 || v.indexOf('zh-mo') === 0) {
        return 'zh-Hant';
      }
      return 'zh';   /* zh, zh-CN, zh-SG, zh-Hans, zh-Hans-CN, cn … */
    }
    if (v === 'en' || v === 'eng' || v.indexOf('en-') === 0) return 'en';
    return 'en';
  }

  /** Simplified -> Traditional for API-supplied text (no-op for the other modes). */
  function toHant(text) {
    var conv = global.FSHant;
    if (!conv || typeof conv.toHant !== 'function') return text;
    return conv.toHant(text);
  }

  /** Localise an already-Simplified API string into the active Chinese variant. */
  function zhText(text) {
    if (text == null || text === '') return text;
    return lang === 'zh-Hant' ? toHant(text) : text;
  }

  function readStored() {
    try {
      var v = global.localStorage.getItem(LS_KEY);
      if (v) return normalize(v);
    } catch (e) { /* storage blocked — default to English */ }
    return 'en';
  }

  function fmt(str, vars) {
    if (!vars) return str;
    return str.replace(/\{(\w+)\}/g, function (m, k) {
      return Object.prototype.hasOwnProperty.call(vars, k) ? String(vars[k]) : m;
    });
  }

  /** Translate a key. Missing keys fall back to the english column, then the key. */
  function t(key, vars) {
    var row = DICT[key];
    var out = row ? row[COLUMN[lang]] : null;
    if (out == null || out === '') out = row ? row[0] : key;
    return fmt(out, vars);
  }

  /** Translate, but use `fallback` (usually an API-provided english label) if the key is unknown. */
  function tOr(key, fallback, vars) {
    if (!DICT[key]) return fallback == null ? key : fallback;
    return t(key, vars);
  }

  /** Pick the matching variant of an API field pair, e.g. label_en/label_zh. */
  function pick(obj, enField, zhField, fallback) {
    if (!obj) return fallback == null ? '' : fallback;
    var en = obj[enField];
    var zh = obj[zhField];
    if (lang === 'zh' || lang === 'zh-Hant') {
      if (zh != null && zh !== '') return zhText(zh);
      if (en != null && en !== '') return en;
    } else {
      if (en != null && en !== '') return en;
      if (zh != null && zh !== '') return zhText(zh);
    }
    return fallback == null ? '' : fallback;
  }

  function setLang(next) {
    lang = normalize(next);
    try { global.localStorage.setItem(LS_KEY, lang); } catch (e) { /* ignore */ }
    try { global.document.documentElement.lang = HTML_LANG[lang] || 'en'; } catch (e) { /* ignore */ }
    try { global.document.title = t('app.docTitle'); } catch (e) { /* ignore */ }
    return lang;
  }

  /**
   * Fill every [data-i18n] node in `root` from the dictionary.
   * data-i18n-placeholder / -title / -aria-label handle the attribute cases.
   */
  function applyStatic(root) {
    var scope = root || global.document;
    if (!scope || !scope.querySelectorAll) return;

    var nodes = scope.querySelectorAll('[data-i18n]');
    for (var i = 0; i < nodes.length; i++) {
      var key = nodes[i].getAttribute('data-i18n');
      if (key) nodes[i].textContent = t(key);
    }
    var ph = scope.querySelectorAll('[data-i18n-placeholder]');
    for (var j = 0; j < ph.length; j++) {
      ph[j].setAttribute('placeholder', t(ph[j].getAttribute('data-i18n-placeholder')));
    }
    var ti = scope.querySelectorAll('[data-i18n-title]');
    for (var k = 0; k < ti.length; k++) {
      ti[k].setAttribute('title', t(ti[k].getAttribute('data-i18n-title')));
    }
    var al = scope.querySelectorAll('[data-i18n-aria-label]');
    for (var m = 0; m < al.length; m++) {
      al[m].setAttribute('aria-label', t(al[m].getAttribute('data-i18n-aria-label')));
    }
  }

  global.FSI18N = {
    DICT: DICT,
    LANGS: LANGS,
    COLUMN: COLUMN,
    get lang() { return lang; },
    isZh: function () { return lang === 'zh'; },
    isHant: function () { return lang === 'zh-Hant'; },
    isZhLike: function () { return lang === 'zh' || lang === 'zh-Hant'; },
    normalize: normalize,
    toHant: toHant,
    zhText: zhText,
    t: t,
    tOr: tOr,
    pick: pick,
    fmt: fmt,
    setLang: setLang,
    readStored: readStored,
    applyStatic: applyStatic,
    has: function (key) { return !!DICT[key]; }
  };
})(window);
