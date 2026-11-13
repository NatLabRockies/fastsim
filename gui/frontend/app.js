/* ==========================================================================
   app.js — FASTSim Studio frontend controller.
   Consumes CONTRACT.md only:
     GET  /api/schema[?preset=<id>]  -> {presets, cycles, parameters[, preset]}
     POST /api/simulate              -> {kpis, timeseries, matplotlib_png_base64}
     POST /api/export-python         -> {code}
   Depends on: FSI18N (i18n.js), FSCharts (charts.js), Plotly (vendor).
   ========================================================================== */
(function (global) {
  'use strict';

  var I18N = global.FSI18N;
  var CHARTS = global.FSCharts;
  var doc = global.document;

  function t(key, vars) { return I18N.t(key, vars); }
  function $(sel, root) { return (root || doc).querySelector(sel); }

  /* ------------------------------------------------------------------ *
   * Constants derived from CONTRACT.md
   * ------------------------------------------------------------------ */
  var GROUP_ORDER = ['Vehicle', 'Chassis', 'Powertrain', 'Thermal'];
  var SUBGROUP_ORDER = [
    'Body', 'Aero', 'Rolling & Traction', 'Wheels', 'Dimensions & Mass',
    'Fuel Storage', 'Fuel Converter', 'Electric Machine', 'Battery (ESS)',
    'Transmission', 'Controls', 'Cabin & HVAC', 'Battery Thermal', 'Other'
  ];

  /* KPI order + label fallbacks (the response's label_en/label_zh win when present) */
  var KPI_META = [
    ['distance_mi', 'Distance', '行驶距离', 'mi'],
    ['fuel_consumed_kwh', 'Fuel Energy', '燃料能量', 'kWh'],
    ['electricity_consumed_kwh', 'Electricity Used', '耗电量', 'kWh'],
    ['fuel_economy_mpgge', 'Fuel Economy', '燃油经济性', 'mpgge'],
    ['electric_economy_wh_per_mi', 'Electricity Economy', '电耗率', 'Wh/mi'],
    ['soc_start_pct', 'SOC Start', '起始 SOC', '%'],
    ['soc_end_pct', 'SOC End', '终止 SOC', '%'],
    ['energy_to_wheels_kwh', 'Energy at Wheels', '车轮牵引能量', 'kWh'],
    ['regen_recovered_kwh', 'Regen Recovered', '再生回收能量', 'kWh'],
    ['avg_speed_mph', 'Average Speed', '平均车速', 'mph'],
    ['max_speed_mph', 'Max Speed', '最高车速', 'mph'],
    ['trace_met_pct', 'Cycle Trace Match', '工况跟随率', '%']
  ];
  var KPI_ALIAS = { fuel_economy: 'fuel_economy_mpgge' };

  /* unit suffixes recognised by the contract, longest first (used to prettify labels) */
  var UNIT_SUFFIXES = [
    '_watts_per_square_meter_kelvin', '_kilograms_per_cubic_meter',
    '_meters_per_second_squared', '_kilogram_square_meters',
    '_joules_per_kilogram', '_watts_per_kilogram', '_meters_per_second',
    '_watts_per_kelvin', '_joules_per_kelvin', '_cubic_meters', '_cubic_meter',
    '_square_meters', '_kilograms', '_seconds', '_watts', '_joules', '_meters', '_kelvin'
  ];

  var ABBREV = {
    pwr: 'Power', out: 'Out', in: 'In', max: 'Max', min: 'Min', avg: 'Avg',
    coef: 'Coef', eff: 'Eff', interp: 'Interp', aux: 'Aux', tgt: 'Target',
    temp: 'Temp', ctrl: 'Ctrl', thr: 'Threshold', kwh: 'kWh', wh: 'Wh',
    soc: 'SOC', hvac: 'HVAC', thrml: 'Thermal', cfg: 'Config', num: 'Num',
    volt: 'Volt', amp: 'Amp', res: 'Battery', em: 'Motor', fc: 'Fuel Converter',
    fs: 'Fuel Storage', pt: 'PT', sim: 'Sim', cntrl: 'Control', add: 'Add',
    w: 'W', kw: 'kW', kw_: 'kW', rgn: 'Regen', regen: 'Regen', mps: 'mps',
    kg: 'kg', m: 'm', s: 's', j: 'J', k: 'K', hp: 'HP', rpm: 'RPM'
  };

  var TABLE_COLS = [
    { key: 'time_s', label: function () { return t('axis.time'); }, dec: 2 },
    { key: 'speed_target_mps', label: function () { return t('trace.target') + ' [m/s]'; } },
    { key: 'speed_achieved_mps', label: function () { return t('trace.achieved') + ' [m/s]'; } },
    { key: 'grade_pct', label: function () { return t('trace.grade') + ' [%]'; } },
    { key: 'engine_power_kw', label: function () { return t('trace.engine') + ' [kW]'; } },
    { key: 'motor_power_kw', label: function () { return t('trace.motor') + ' [kW]'; } },
    { key: 'battery_power_kw', label: function () { return t('trace.battery') + ' [kW]'; } },
    { key: 'soc_pct', label: function () { return t('trace.soc') + ' [%]'; } },
    { key: 'fuel_power_kw', label: function () { return t('trace.fuelPower') + ' [kW]'; } },
    { key: 'cum_fuel_kwh', label: function () { return t('trace.fuel') + ' [kWh]'; } },
    { key: 'cum_energy_kwh', label: function () { return t('trace.energy') + ' [kWh]'; } },
    { key: 'cum_distance_mi', label: function () { return t('trace.distance') + ' [mi]'; } }
  ];

  var MAX_TABLE_ROWS = 1500;
  var MOCK = { schema: '/mock/schema.json', simulate: '/mock/simulate.json', export: '/mock/export.json' };

  /* ------------------------------------------------------------------ *
   * State
   * ------------------------------------------------------------------ */
  var S = {
    /* schema */
    schema: null,
    presets: [],
    cycles: [],
    presetId: null,
    cycleId: null,
    params: [],
    defaults: {},
    values: {},
    editsByPreset: {},        /* presetId -> {path: value} — survives preset switching */
    groups: [],
    rows: [],

    /* ui */
    search: '',
    modifiedOnly: false,
    collapsedGroups: {},
    collapsedSubgroups: {},
    tab: 'interactive',
    schemaBusy: false,
    loading: false,
    exporting: false,
    mock: false,
    mockAuto: false,

    /* results */
    result: null,
    resultMeta: null,
    stale: false,
    lastMs: 0,

    /* modal */
    modalOpen: false,
    exportCode: '',
    exportState: 'idle',      /* idle | loading | ready | error */
    exportError: '',
    exportToken: 0
  };

  /* ------------------------------------------------------------------ *
   * Small helpers
   * ------------------------------------------------------------------ */
  function el(tag, opts) {
    var node = doc.createElement(tag);
    opts = opts || {};
    if (opts.className) node.className = opts.className;
    if (opts.text != null) node.textContent = opts.text;
    if (opts.attrs) {
      Object.keys(opts.attrs).forEach(function (k) {
        var v = opts.attrs[k];
        if (v !== null && v !== undefined) node.setAttribute(k, v);
      });
    }
    return node;
  }

  function esc(s) {
    return String(s == null ? '' : s)
      .replace(/&/g, '&amp;').replace(/</g, '&lt;').replace(/>/g, '&gt;');
  }

  /** compact numeric formatting for KPIs, tables and read-only values */
  function fmtNum(v) {
    if (v === null || v === undefined || v === '') return t('field.emptyValue');
    if (typeof v === 'boolean') return String(v);
    var n = typeof v === 'number' ? v : Number(v);
    if (typeof v !== 'number') {
      if (!isFinite(n)) return String(v);
    }
    if (!isFinite(n)) return t('field.emptyValue');
    if (n === 0) return '0';
    var a = Math.abs(n);
    if (Number.isInteger(n) && a < 1e7) return String(n);
    if (a >= 1e7 || a < 1e-4) return n.toExponential(2);
    var d = a >= 100 ? 1 : a >= 10 ? 2 : a >= 1 ? 3 : 4;
    return n.toFixed(d).replace(/(\.\d*?)0+$/, '$1').replace(/\.$/, '');
  }

  function leafOf(path) {
    var parts = String(path || '').split('.');
    return parts[parts.length - 1];
  }

  /** pwr_out_max_watts -> "Power Out Max" */
  function humanize(path) {
    var core = leafOf(path);
    for (var i = 0; i < UNIT_SUFFIXES.length; i++) {
      var suf = UNIT_SUFFIXES[i];
      if (core.length > suf.length && core.slice(-suf.length) === suf) {
        core = core.slice(0, core.length - suf.length);
        break;
      }
    }
    var words = core.split('_').filter(function (w) { return w !== ''; }).map(function (w) {
      var hit = ABBREV[w.toLowerCase()];
      if (hit) return hit;
      if (/^[a-z]/.test(w)) return w.charAt(0).toUpperCase() + w.slice(1);
      return w;
    });
    return words.join(' ') || leafOf(path);
  }

  function paramLabel(p) {
    if (p.label_en || p.label_zh) return I18N.pick(p, 'label_en', 'label_zh', humanize(p.path));
    if (p.name && p.name !== p.path) return p.name;
    return humanize(p.path);
  }

  function paramDescription(p) {
    return I18N.pick(p, 'description', 'description_zh', '');
  }

  function groupLabel(name, sample) {
    if (I18N.isZhLike() && sample && sample.group_zh) return I18N.zhText(sample.group_zh);
    return I18N.tOr('group.' + name, name);
  }

  function subgroupLabel(name, sample) {
    if (I18N.isZhLike() && sample && sample.subgroup_zh) return I18N.zhText(sample.subgroup_zh);
    return I18N.tOr('subgroup.' + name, name);
  }

  function summarize(v) {
    if (v === null || v === undefined) return 'null';
    if (Array.isArray(v)) return t('field.items', { n: v.length });
    if (typeof v === 'object') {
      var keys = Object.keys(v);
      if (!keys.length) return '{}';
      if (keys.length <= 3) return '{ ' + keys.join(', ') + ' }';
      return t('field.keys', { n: keys.length });
    }
    var s = typeof v === 'number' ? fmtNum(v) : String(v);
    return s.length > 20 ? s.slice(0, 19) + '…' : s;
  }

  function debounce(fn, ms) {
    var h = null;
    return function () {
      var args = arguments, self = this;
      if (h) global.clearTimeout(h);
      h = global.setTimeout(function () { h = null; fn.apply(self, args); }, ms);
    };
  }

  /* ------------------------------------------------------------------ *
   * API layer (contract-only; mock path is opt-in via ?mock=1)
   * ------------------------------------------------------------------ */
  function apiError(message, extra) {
    var e = new Error(message);
    if (extra) {
      if (extra.status !== undefined) e.status = extra.status;
      if (extra.detail !== undefined) e.detail = extra.detail;
      if (extra.data !== undefined) e.data = extra.data;
      if (extra.kind !== undefined) e.kind = extra.kind;
    }
    return e;
  }

  function readJson(res) {
    return res.text().then(function (text) {
      var data = null;
      if (text) {
        try { data = JSON.parse(text); } catch (e) { data = null; }
      }
      if (!res.ok) {
        var message = '';
        var detail;
        if (data && typeof data === 'object') {
          message = data.error || data.message || '';
          detail = data.detail !== undefined && data.detail !== null ? String(data.detail) : undefined;
          if (!message && detail) { message = detail; detail = undefined; }
        }
        if (!message) {
          message = (text || '').slice(0, 300) ||
            t('err.http', { status: res.status, statusText: res.statusText || '' });
        }
        throw apiError(String(message).slice(0, 600), {
          status: res.status, detail: detail, data: data, kind: 'http'
        });
      }
      if (data === null) throw apiError(t('err.badJson'), { kind: 'json', status: res.status });
      return data;
    });
  }

  function request(url, opts) {
    var p;
    try {
      p = global.fetch(url, opts || {});
    } catch (e) {
      return Promise.reject(apiError(t('err.network'), { kind: 'network' }));
    }
    return p.then(function (res) { return readJson(res); }, function () {
      throw apiError(t('err.network'), { kind: 'network' });
    });
  }

  function fetchSchema(presetId) {
    if (S.mock) return request(MOCK.schema);
    var url = '/api/schema' + (presetId ? '?preset=' + encodeURIComponent(presetId) : '');
    return request(url);
  }

  function fetchSimulate(payload) {
    if (S.mock) return request(MOCK.simulate);
    return request('/api/simulate', {
      method: 'POST',
      headers: { 'Content-Type': 'application/json' },
      body: JSON.stringify(payload)
    });
  }

  function fetchExport(payload) {
    if (S.mock) return request(MOCK.export);
    return request('/api/export-python', {
      method: 'POST',
      headers: { 'Content-Type': 'application/json' },
      body: JSON.stringify(payload)
    });
  }

  /* ------------------------------------------------------------------ *
   * Banner
   * ------------------------------------------------------------------ */
  function hideBanner() {
    var slot = $('#banner-slot');
    if (slot) slot.innerHTML = '';
  }

  function showBanner(cfg) {
    var slot = $('#banner-slot');
    if (!slot) return;
    slot.innerHTML = '';
    var kind = cfg.kind === 'info' ? ' banner-info' : '';
    var wrap = el('div', { className: 'banner' + kind, attrs: { role: 'alert' } });
    wrap.appendChild(el('span', { className: 'banner-icon', text: cfg.kind === 'info' ? 'i' : '!' }));

    var content = el('div', { className: 'banner-content' });
    if (cfg.title) content.appendChild(el('div', { className: 'banner-title', text: cfg.title }));
    if (cfg.message) content.appendChild(el('div', { className: 'banner-detail', text: cfg.message }));
    if (cfg.detail) content.appendChild(el('div', { className: 'banner-detail', text: cfg.detail }));
    wrap.appendChild(content);

    var actions = el('div', { className: 'banner-actions' });
    (cfg.actions || []).forEach(function (a) {
      var b = el('button', { className: 'btn btn-ghost btn-sm', text: a.label, attrs: { type: 'button' } });
      b.addEventListener('click', a.onClick);
      actions.appendChild(b);
    });
    var close = el('button', {
      className: 'btn btn-ghost btn-sm', text: t('err.dismiss'),
      attrs: { type: 'button', 'aria-label': t('err.dismiss') }
    });
    close.addEventListener('click', hideBanner);
    actions.appendChild(close);
    wrap.appendChild(actions);

    slot.appendChild(wrap);
  }

  /* ------------------------------------------------------------------ *
   * Modified-value bookkeeping
   * ------------------------------------------------------------------ */
  function isModified(p, v) {
    if (!p || !p.editable) return false;
    var d = p['default'];
    if (p.type === 'bool') return (!!v) !== (!!d);
    if (p.type === 'int' || p.type === 'float') {
      if (v === null || v === undefined || v === '') return false;
      var a = Number(v), b = Number(d);
      if (!isFinite(a)) return false;
      if (d === null || d === undefined || !isFinite(b)) return true;
      return Math.abs(a - b) > Math.max(1e-9, Math.abs(b) * 1e-9);
    }
    var ds = (d === null || d === undefined) ? '' : String(d);
    return String(v == null ? '' : v) !== ds;
  }

  function collectOverrides() {
    var out = {};
    S.params.forEach(function (p) {
      if (!p.editable) return;
      if (isModified(p, S.values[p.path])) out[p.path] = S.values[p.path];
    });
    return out;
  }

  function persistEdits() {
    if (!S.presetId) return;
    var map = {};
    S.params.forEach(function (p) {
      if (p.editable && isModified(p, S.values[p.path])) map[p.path] = S.values[p.path];
    });
    S.editsByPreset[S.presetId] = map;
  }

  function modifiedCount() {
    var n = 0;
    S.params.forEach(function (p) { if (isModified(p, S.values[p.path])) n++; });
    return n;
  }

  function updateModifiedCount() {
    var n = modifiedCount();
    var chip = $('#modified-count');
    if (chip) {
      chip.textContent = t('panel.modifiedCount', { n: n });
      chip.classList.toggle('is-zero', n === 0);
    }
    /* per-group warning dots */
    S.groups.forEach(function (g) {
      var any = g.subgroups.some(function (sg) {
        return sg.params.some(function (p) { return isModified(p, S.values[p.path]); });
      });
      var dot = g.node && g.node.querySelector('.group-dot');
      if (dot) dot.hidden = !any;
    });
  }

  /* ------------------------------------------------------------------ *
   * Parameter panel
   * ------------------------------------------------------------------ */
  function buildTree() {
    var byGroup = {};
    S.params.forEach(function (p) {
      var gName = p.group || 'Other';
      var sgName = p.subgroup || 'Other';
      if (!byGroup[gName]) byGroup[gName] = { name: gName, sample: p, subgroups: {} };
      var g = byGroup[gName];
      if (!g.subgroups[sgName]) g.subgroups[sgName] = { name: sgName, sample: p, params: [] };
      g.subgroups[sgName].params.push(p);
    });

    var groups = Object.keys(byGroup).map(function (gName) {
      var g = byGroup[gName];
      var subs = Object.keys(g.subgroups).map(function (sgName) {
        var sg = g.subgroups[sgName];
        sg.params.sort(function (a, b) { return a.path < b.path ? -1 : a.path > b.path ? 1 : 0; });
        return sg;
      });
      subs.sort(function (a, b) {
        var ia = SUBGROUP_ORDER.indexOf(a.name), ib = SUBGROUP_ORDER.indexOf(b.name);
        if (ia === -1) ia = 50;
        if (ib === -1) ib = 50;
        return ia === ib ? (a.name < b.name ? -1 : 1) : ia - ib;
      });
      return { name: g.name, sample: g.sample, subgroups: subs };
    });

    groups.sort(function (a, b) {
      var ia = GROUP_ORDER.indexOf(a.name), ib = GROUP_ORDER.indexOf(b.name);
      if (ia === -1) ia = 50;
      if (ib === -1) ib = 50;
      return ia === ib ? (a.name < b.name ? -1 : 1) : ia - ib;
    });
    return groups;
  }

  function fieldTooltip(p) {
    var parts = [p.path];
    if (p['default'] !== undefined) parts.push(t('field.default', { v: summarize(p['default']) }));
    return parts.join('\n');
  }

  function buildControl(p) {
    var value = S.values[p.path];
    var editable = p.editable !== false && ['float', 'int', 'bool', 'enum'].indexOf(p.type) !== -1;

    if (!editable) {
      var ro = el('div', {
        className: 'ro-value',
        text: summarize(value !== undefined ? value : p['default']),
        attrs: { title: t('field.readonly') + '\n' + p.path }
      });
      return ro;
    }

    var node;
    if (p.type === 'bool') {
      node = el('input', { className: 'input-check', attrs: { type: 'checkbox' } });
      node.checked = !!value;
      node.addEventListener('change', function () {
        setValue(p, node.checked);
      });
    } else if (p.type === 'enum') {
      node = el('select', { className: 'select-field', attrs: { 'aria-label': paramLabel(p) } });
      var opts = Array.isArray(p.enum_options) ? p.enum_options.slice() : [];
      [value, p['default']].forEach(function (v) {
        if (v !== null && v !== undefined && opts.indexOf(v) === -1) opts.push(v);
      });
      opts.forEach(function (o) {
        var opt = el('option', { text: String(o), attrs: { value: String(o) } });
        if (String(o) === String(value)) opt.selected = true;
        node.appendChild(opt);
      });
      node.addEventListener('change', function () { setValue(p, node.value); });
    } else if (p.type === 'str') {
      node = el('input', { className: 'input-text', attrs: { type: 'text', 'aria-label': paramLabel(p), spellcheck: 'false' } });
      node.value = value === null || value === undefined ? '' : String(value);
      node.addEventListener('input', function () { setValue(p, node.value); });
    } else {
      node = el('input', {
        className: 'input-num',
        attrs: {
          type: 'number',
          step: p.type === 'int' ? '1' : 'any',
          inputmode: 'decimal',
          'aria-label': paramLabel(p)
        }
      });
      node.value = value === null || value === undefined ? '' : String(value);
      node.addEventListener('input', function () {
        if (node.value.trim() === '') return;              /* let the user finish typing */
        var n = p.type === 'int' ? global.parseInt(node.value, 10) : global.parseFloat(node.value);
        if (!isFinite(n)) return;
        setValue(p, n);
      });
      node.addEventListener('blur', function () {
        if (node.value.trim() === '') {
          node.value = S.values[p.path] === null || S.values[p.path] === undefined
            ? '' : String(S.values[p.path]);
        }
      });
    }

    var reset = el('button', {
      className: 'reset-btn',
      text: '↺',
      attrs: { type: 'button', title: t('field.reset'), 'aria-label': t('field.reset') }
    });
    reset.addEventListener('click', function () { resetField(p); });

    /* deterministic id/name so every control is a well-formed form field
       (screen readers + Chrome's "should have an id or name" a11y audit) */
    node.id = 'param-' + String(p.path).replace(/[^A-Za-z0-9_-]+/g, '-');
    node.setAttribute('name', p.path);

    var wrap = doc.createDocumentFragment();
    wrap.appendChild(node);
    wrap.appendChild(reset);
    wrap._input = node;
    wrap._reset = reset;
    return wrap;
  }

  function renderParams() {
    var host = $('#params-root');
    if (!host) return;
    var scrollTop = host.scrollTop;
    host.innerHTML = '';
    S.rows = [];
    S.groups = [];

    if (!S.params.length) {
      var msg = el('div', { className: 'wb-empty', text: S.schemaBusy ? t('panel.loading') : t('panel.loadFailed') });
      msg.setAttribute('data-role', 'panel-message');
      host.appendChild(msg);
      updateModifiedCount();
      applyFilters();
      return;
    }

    var tree = buildTree();
    var frag = doc.createDocumentFragment();

    tree.forEach(function (g) {
      var gKey = 'g:' + g.name;
      var gOpen = !S.collapsedGroups[gKey];

      var sec = el('section', { className: 'group' + (gOpen ? ' is-open' : ''), attrs: { 'data-group': g.name } });
      var head = el('button', { className: 'group-head', attrs: { type: 'button', 'aria-expanded': gOpen ? 'true' : 'false' } });
      head.appendChild(el('span', { className: 'chevron' }));
      head.appendChild(el('span', { className: 'group-name', text: groupLabel(g.name, g.sample) }));
      head.appendChild(el('span', { className: 'group-dot', attrs: { hidden: 'hidden' } }));
      var total = g.subgroups.reduce(function (n, sg) { return n + sg.params.length; }, 0);
      head.appendChild(el('span', { className: 'group-count', text: String(total) }));
      head.addEventListener('click', function () {
        var open = sec.classList.toggle('is-open');
        S.collapsedGroups[gKey] = !open;
        head.setAttribute('aria-expanded', open ? 'true' : 'false');
      });
      sec.appendChild(head);

      var body = el('div', { className: 'group-body' });
      g.subgroups.forEach(function (sg) {
        var sgEl = el('div', { className: 'subgroup is-open' });
        var sgHead = el('button', { className: 'subgroup-head', attrs: { type: 'button', 'aria-expanded': 'true' } });
        sgHead.appendChild(el('span', { className: 'chevron' }));
        sgHead.appendChild(el('span', { className: 'subgroup-name', text: subgroupLabel(sg.name, sg.sample) }));
        sgHead.appendChild(el('span', { className: 'subgroup-line' }));
        sgHead.appendChild(el('span', { className: 'group-count', text: String(sg.params.length) }));
        sgHead.addEventListener('click', function () {
          var open = sgEl.classList.toggle('is-open');
          sgHead.setAttribute('aria-expanded', open ? 'true' : 'false');
        });
        sgEl.appendChild(sgHead);

        var sgBody = el('div', { className: 'subgroup-body' });
        sg.params.forEach(function (p) {
          sgBody.appendChild(buildRow(p));
        });
        sgEl.appendChild(sgBody);
        body.appendChild(sgEl);
      });
      sec.appendChild(body);
      g.node = sec;
      S.groups.push(g);
      frag.appendChild(sec);
    });

    host.appendChild(frag);
    host.scrollTop = scrollTop;
    updateModifiedCount();
    applyFilters();
  }

  function buildRow(p) {
    var editable = p.editable !== false && ['float', 'int', 'bool', 'enum'].indexOf(p.type) !== -1;
    var row = el('div', {
      className: 'param-row' + (editable ? '' : ' is-readonly'),
      attrs: { 'data-path': p.path }
    });

    var main = el('div', { className: 'param-main' });
    var head = el('div', { className: 'param-head' });
    head.appendChild(el('span', { className: 'param-label', text: paramLabel(p) }));
    var unit = p.unit && p.unit !== '-' ? p.unit : '';
    head.appendChild(el('span', { className: 'unit-chip' + (unit ? '' : ' is-empty'), text: unit }));
    var badge = el('span', { className: 'badge-modified', text: t('field.modified'), attrs: { hidden: 'hidden' } });
    head.appendChild(badge);
    main.appendChild(head);

    var desc = paramDescription(p);
    main.appendChild(el('div', { className: 'param-desc', text: desc }));
    main.appendChild(el('div', { className: 'param-path', text: p.path, attrs: { title: fieldTooltip(p) } }));

    var control = el('div', { className: 'param-control' });
    var built = buildControl(p);
    control.appendChild(built);
    if (!editable) {
      row.setAttribute('title', t('field.readonly'));
    }

    row.appendChild(main);
    row.appendChild(control);

    var rec = {
      p: p,
      el: row,
      input: built._input || null,
      reset: built._reset || null,
      badge: badge,
      groupName: p.group || 'Other',
      hay: [
        p.path, p.name || '', paramLabel(p), desc,
        I18N.zhText(p.description_zh || ''), I18N.zhText(p.label_zh || ''),
        p.group || '', I18N.zhText(p.group_zh || ''), p.subgroup || '', I18N.zhText(p.subgroup_zh || '')
      ].join(' ').toLowerCase()
    };
    S.rows.push(rec);
    refreshRow(rec);
    return row;
  }

  function refreshRow(rec) {
    var modified = isModified(rec.p, S.values[rec.p.path]);
    rec.el.classList.toggle('is-modified', modified);
    if (rec.badge) rec.badge.hidden = !modified;
    if (rec.reset) rec.reset.disabled = !modified;
  }

  function setValue(p, value) {
    S.values[p.path] = value;
    var rec = S.rows.filter(function (r) { return r.p.path === p.path; })[0];
    if (rec) refreshRow(rec);
    persistEdits();
    updateModifiedCount();
    applyFilters();
    markStale();
  }

  function resetField(p) {
    S.values[p.path] = p['default'];
    var rec = S.rows.filter(function (r) { return r.p.path === p.path; })[0];
    if (rec) {
      refreshRow(rec);
      if (rec.input) {
        if (rec.p.type === 'bool') rec.input.checked = !!p['default'];
        else if (rec.p.type === 'enum') rec.input.value = String(p['default']);
        else rec.input.value = p['default'] === null || p['default'] === undefined ? '' : String(p['default']);
      }
    }
    persistEdits();
    updateModifiedCount();
    applyFilters();
    markStale();
  }

  function resetAll() {
    if (!global.confirm(t('panel.resetAllConfirm'))) return;
    S.params.forEach(function (p) { S.values[p.path] = p['default']; });
    S.editsByPreset[S.presetId] = {};
    renderParams();
    markStale();
  }

  /* ------------------------------------------------------------------ *
   * Filtering
   * ------------------------------------------------------------------ */
  function applyFilters() {
    var q = (S.search || '').trim().toLowerCase();
    var tokens = q ? q.split(/\s+/) : [];
    var host = $('#params-root');
    var active = doc.activeElement;
    var visible = 0;

    S.rows.forEach(function (rec) {
      var match = true;
      for (var i = 0; i < tokens.length; i++) {
        if (rec.hay.indexOf(tokens[i]) === -1) { match = false; break; }
      }
      if (match && S.modifiedOnly && !rec.el.classList.contains('is-modified')) match = false;
      if (!match && active && rec.el.contains(active)) match = true;  /* never yank the focused field */
      rec.el.classList.toggle('is-hidden', !match);
      if (match) visible++;
    });

    /* hide empty subgroups / groups */
    var groups = host ? host.querySelectorAll('.group') : [];
    for (var gi = 0; gi < groups.length; gi++) {
      var gNode = groups[gi];
      var subEls = gNode.querySelectorAll('.subgroup');
      var groupVisible = 0;
      for (var si = 0; si < subEls.length; si++) {
        var anyRow = 0;
        var rows = subEls[si].querySelectorAll('.param-row');
        for (var ri = 0; ri < rows.length; ri++) {
          if (!rows[ri].classList.contains('is-hidden')) anyRow++;
        }
        subEls[si].classList.toggle('is-hidden', anyRow === 0);
        groupVisible += anyRow;
      }
      gNode.classList.toggle('is-hidden', groupVisible === 0);
    }

    var empty = $('#params-empty');
    var noMatch = S.rows.length > 0 && visible === 0;
    if (empty) empty.hidden = !noMatch;
    if (host) host.hidden = noMatch;   /* let the hint occupy the panel instead */
  }

  /* ------------------------------------------------------------------ *
   * Selects
   * ------------------------------------------------------------------ */
  function renderPresetSelect() {
    var sel = $('#preset-select');
    if (!sel) return;
    var current = S.presetId;
    sel.innerHTML = '';
    S.presets.forEach(function (p) {
      var label = I18N.pick(p, 'name', 'name_zh', p.id);
      if (p.pt_type) label += ' · ' + p.pt_type;
      var opt = el('option', { text: label, attrs: { value: p.id } });
      var bits = [];
      if (p.pt_type) bits.push(p.pt_type);
      if (p.mass_kg != null) bits.push(Math.round(p.mass_kg) + ' kg');
      if (bits.length) opt.title = bits.join(' · ');
      if (p.id === current) opt.selected = true;
      sel.appendChild(opt);
    });
    if (current) sel.value = current;
  }

  function renderCycleSelect() {
    var sel = $('#cycle-select');
    if (!sel) return;
    var current = S.cycleId;
    sel.innerHTML = '';
    S.cycles.forEach(function (c) {
      var opt = el('option', { text: I18N.pick(c, 'name', 'name_zh', c.id), attrs: { value: c.id } });
      var bits = [];
      if (c.duration_s != null) bits.push(Math.round(c.duration_s) + ' s');
      if (bits.length) opt.title = bits.join(' · ');
      if (c.id === current) opt.selected = true;
      sel.appendChild(opt);
    });
    if (!current && S.cycles.length) {
      S.cycleId = S.cycles[0].id;
      sel.value = S.cycleId;
    } else if (current) {
      sel.value = current;
    }
  }

  function presetName(id) {
    var hit = S.presets.filter(function (p) { return p.id === id; })[0];
    return hit ? I18N.pick(hit, 'name', 'name_zh', id) : (id || '');
  }

  function cycleName(id) {
    var hit = S.cycles.filter(function (c) { return c.id === id; })[0];
    return hit ? I18N.pick(hit, 'name', 'name_zh', id) : (id || '');
  }

  /* ------------------------------------------------------------------ *
   * Schema loading
   * ------------------------------------------------------------------ */
  function applySchema(data, requestedPreset) {
    if (!data || typeof data !== 'object') throw apiError(t('err.badJson'), { kind: 'json' });

    if (!S.mock && requestedPreset && data.preset && String(data.preset) !== String(requestedPreset)) {
      throw apiError(t('err.presetMismatch', { got: data.preset, want: requestedPreset }), { kind: 'mismatch' });
    }

    if (Array.isArray(data.presets) && data.presets.length) S.presets = data.presets;
    if (Array.isArray(data.cycles) && data.cycles.length) S.cycles = data.cycles;

    var params = Array.isArray(data.parameters) ? data.parameters.filter(function (p) {
      return p && typeof p.path === 'string' && p.path;
    }) : [];
    if (!params.length) throw apiError(t('panel.loadFailed'), { kind: 'empty' });

    S.schema = data;
    S.params = params;
    S.presetId = requestedPreset || data.preset || (S.presets[0] && S.presets[0].id) || S.presetId;

    /* defaults + remembered edits (paths that vanished are dropped silently) */
    var saved = S.editsByPreset[S.presetId] || {};
    var values = {};
    var defaults = {};
    params.forEach(function (p) {
      defaults[p.path] = p['default'];
      var v = p['default'];
      if (p.editable && Object.prototype.hasOwnProperty.call(saved, p.path)) v = saved[p.path];
      values[p.path] = v;
    });
    S.defaults = defaults;
    S.values = values;
    persistEdits();

    if (S.presets.length) renderPresetSelect();
    if (S.cycles.length) renderCycleSelect();

    renderParams();
    updateStatus();
  }

  function loadSchema(presetId) {
    S.schemaBusy = true;
    var host = $('#params-root');
    if (host && !S.params.length) {
      host.innerHTML = '';
      host.appendChild(el('div', { className: 'wb-empty', text: t('panel.loading') }));
    }
    var started = Date.now();
    return fetchSchema(presetId).then(function (data) {
      applySchema(data, presetId);
      S.schemaBusy = false;
      return data;
    }, function (err) {
      S.schemaBusy = false;
      if (S.mockAuto && !S.mock) {
        /* ?mock=auto — the real API is unreachable, fall back to the fixtures */
        S.mock = true;
        S.mockAuto = false;
        setMockUi();
        showMockBanner();
        return loadSchema(presetId);
      }
      throw err;
    });
  }

  function setMockUi() {
    var chip = $('#mock-chip');
    if (chip) chip.hidden = !S.mock;
  }

  /* ------------------------------------------------------------------ *
   * Simulation
   * ------------------------------------------------------------------ */
  function setBusy(busy) {
    S.loading = busy;
    var runBtn = $('#run-btn');
    var label = $('#run-label');
    var spinner = $('#run-spinner');
    var exportBtn = $('#export-btn');
    if (runBtn) runBtn.disabled = busy;
    if (exportBtn) exportBtn.disabled = busy;
    if (label) label.textContent = busy ? t('toolbar.running') : t('toolbar.run');
    if (runBtn) runBtn.setAttribute('aria-busy', busy ? 'true' : 'false');
    if (spinner) spinner.hidden = !busy;

    var loadingEl = $('#loading-state');
    var emptyEl = $('#empty-state');
    var tabs = $('#tabs');
    if (busy) {
      if (loadingEl) loadingEl.hidden = false;
      if (emptyEl) emptyEl.hidden = true;
      ['#pane-interactive', '#pane-figure', '#pane-table'].forEach(function (s) {
        var n = $(s); if (n) n.hidden = true;
      });
      if (tabs) tabs.hidden = true;
      var status = $('#run-status');
      if (status) {
        status.textContent = t('status.busy');
        status.classList.add('is-busy');
      }
    } else {
      if (loadingEl) loadingEl.hidden = true;
      if (tabs) tabs.hidden = !S.result;
    }
  }

  function runSimulation() {
    if (S.loading) return;
    if (!S.presetId || !S.cycleId) {
      showBanner({ title: t('err.sim'), message: t('err.missing'), kind: 'error' });
      return;
    }
    hideBanner();
    setBusy(true);
    var payload = {
      preset: S.presetId,
      cycle: S.cycleId,
      overrides: collectOverrides(),
      render_matplotlib: true,
      lang: I18N.lang
    };
    var t0 = (global.performance && global.performance.now) ? global.performance.now() : Date.now();

    fetchSimulate(payload).then(function (data) {
      var t1 = (global.performance && global.performance.now) ? global.performance.now() : Date.now();
      S.lastMs = Math.round(t1 - t0);
      S.result = data && typeof data === 'object' ? data : null;
      S.resultMeta = { preset: payload.preset, cycle: payload.cycle, overrides: payload.overrides };
      S.stale = false;
      setBusy(false);
      renderResult();
    }).catch(function (err) {
      setBusy(false);
      handleSimError(err);
      updateStatus();
    });
  }

  function handleSimError(err) {
    var unknownPath = null;
    var raw = (err && (err.detail || err.message)) || '';
    if (err && err.status === 400 && /unknown parameter path/i.test(String(err.message || ''))) {
      unknownPath = err.detail || null;
    }
    var pruned = false;
    if (unknownPath) pruned = pruneOverride(unknownPath);
    if (!pruned && err && err.data && String(err.data.error || '') === 'Unknown parameter path' && err.data.detail) {
      pruned = pruneOverride(String(err.data.detail));
      unknownPath = String(err.data.detail);
    }

    showBanner({
      title: t('err.sim'),
      message: pruned
        ? (unknownPath ? t('err.unknownPath', { path: unknownPath }) : t('err.unknownPathGeneric'))
        : (err && err.message ? err.message : String(err)),
      detail: pruned ? '' : (err && err.detail ? err.detail : ''),
      kind: 'error'
    });
  }

  function pruneOverride(path) {
    if (!path || !Object.prototype.hasOwnProperty.call(S.defaults, path)) return false;
    S.values[path] = S.defaults[path];
    var p = S.params.filter(function (x) { return x.path === path; })[0];
    if (p) {
      var rec = S.rows.filter(function (r) { return r.p.path === path; })[0];
      if (rec) {
        refreshRow(rec);
        if (rec.input) {
          if (p.type === 'bool') rec.input.checked = !!S.defaults[path];
          else if (p.type === 'enum') rec.input.value = String(S.defaults[path]);
          else rec.input.value = S.defaults[path] == null ? '' : String(S.defaults[path]);
        }
      }
    }
    persistEdits();
    updateModifiedCount();
    applyFilters();
    return true;
  }

  function markStale() {
    if (!S.result) return;
    if (!S.stale) {
      S.stale = true;
      renderStaleChip();
    }
  }

  function renderStaleChip() {
    var chip = $('#stale-chip');
    if (!chip) return;
    chip.hidden = !(S.result && S.stale);
    chip.textContent = t('stale.badge');
    chip.onclick = function () { runSimulation(); };
  }

  /* ------------------------------------------------------------------ *
   * Results: KPIs, status, panes
   * ------------------------------------------------------------------ */
  function normalizeKpis(kpis) {
    var out = [];
    if (!kpis || typeof kpis !== 'object') return out;
    var seen = {};
    var order = {};
    KPI_META.forEach(function (row, i) { order[row[0]] = i; });

    Object.keys(kpis).forEach(function (key) {
      var item = kpis[key];
      if (item === null || item === undefined) return;
      var canonical = KPI_ALIAS[key] ? KPI_ALIAS[key] : key;
      if (seen[canonical]) return;
      seen[canonical] = true;
      var meta = KPI_META.filter(function (r) { return r[0] === canonical; })[0];
      var obj = (typeof item === 'object') ? item : { value: item };
      out.push({
        key: canonical,
        order: order[canonical] === undefined ? 100 : order[canonical],
        value: obj.value,
        unit: obj.unit || (meta ? meta[3] : ''),
        label: I18N.pick(obj, 'label_en', 'label_zh',
          meta ? (I18N.isZhLike() ? I18N.zhText(meta[2]) : meta[1]) : canonical)
      });
    });

    out.sort(function (a, b) {
      return a.order === b.order ? (a.key < b.key ? -1 : 1) : a.order - b.order;
    });
    return out;
  }

  function renderKpis() {
    var row = $('#kpi-row');
    if (!row) return;
    var items = normalizeKpis(S.result && S.result.kpis);
    row.innerHTML = '';
    if (!items.length) {
      row.hidden = true;
      return;
    }
    row.hidden = false;
    items.forEach(function (item) {
      var card = el('div', { className: 'kpi-card', attrs: { 'data-kpi': item.key } });
      card.appendChild(el('div', { className: 'kpi-label', text: item.label }));
      var vrow = el('div', { className: 'kpi-value-row' });
      vrow.appendChild(el('span', { className: 'kpi-value', text: fmtNum(item.value) }));
      if (item.unit) vrow.appendChild(el('span', { className: 'kpi-unit', text: item.unit }));
      card.appendChild(vrow);
      row.appendChild(card);
    });
  }

  function updateStatus() {
    var status = $('#run-status');
    if (!status) return;
    status.classList.toggle('is-busy', S.loading);
    if (S.loading) {
      status.textContent = t('status.busy');
      return;
    }
    if (!S.result) {
      status.textContent = S.presetId
        ? (presetName(S.presetId) + ' · ' + cycleName(S.cycleId))
        : t('status.ready');
      return;
    }
    var steps = CHARTS ? CHARTS.describeLength(S.result) : 0;
    var overrides = S.resultMeta ? Object.keys(S.resultMeta.overrides || {}).length : 0;
    var parts = [
      t('status.done'),
      t('status.doneDetail', {
        preset: presetName(S.resultMeta ? S.resultMeta.preset : S.presetId),
        cycle: cycleName(S.resultMeta ? S.resultMeta.cycle : S.cycleId),
        n: steps,
        ms: S.lastMs
      }),
      overrides ? t(overrides === 1 ? 'status.overrideOne' : 'status.overrides', { n: overrides }) : t('status.noOverrides')
    ];
    status.textContent = parts.join(' · ');
  }

  function setTab(tab) {
    S.tab = tab;
    var tabs = doc.querySelectorAll('#tabs .tab');
    for (var i = 0; i < tabs.length; i++) {
      var on = tabs[i].getAttribute('data-tab') === tab;
      tabs[i].classList.toggle('is-active', on);
      tabs[i].setAttribute('aria-selected', on ? 'true' : 'false');
    }
    var panes = { interactive: $('#pane-interactive'), figure: $('#pane-figure'), table: $('#pane-table') };
    Object.keys(panes).forEach(function (k) {
      if (panes[k]) panes[k].hidden = !(S.result && k === tab);
    });
    if (!S.result) return;
    if (tab === 'interactive') renderChart();
    else if (tab === 'figure') renderFigure();
    else if (tab === 'table') renderTable();
  }

  function renderResult() {
    var emptyEl = $('#empty-state');
    if (emptyEl) emptyEl.hidden = true;
    var tabs = $('#tabs');
    if (tabs) tabs.hidden = false;
    renderKpis();
    renderStaleChip();
    updateStatus();
    setTab(S.tab);
  }

  function renderChart() {
    var host = $('#plot');
    var note = $('#plot-note');
    if (!host) return;
    if (!S.result) return;
    if (note) { note.hidden = true; note.textContent = ''; }

    if (!global.Plotly) {
      if (note) {
        note.hidden = false;
        note.textContent = t('chart.plotlyMissing');
      }
      return;
    }
    var res = CHARTS.render(host, S.result);
    if (!res.ok) {
      if (res.message === 'no-timeseries') {
        if (note) { note.hidden = false; note.textContent = t('chart.noSeries'); }
      } else if (note) {
        note.hidden = false;
        note.textContent = t('chart.renderFailed', { msg: res.message || 'unknown error' });
      }
      return;
    }
    if (note) { note.hidden = true; note.textContent = ''; }
  }

  /**
   * Matplotlib pane.
   * The API returns matplotlib_png_base64 and — beyond the contract — a raw
   * (non-base64) matplotlib_svg. Both are surfaced when present; when the SVG
   * is missing we say so instead of inventing one from the raster.
   */
  function renderFigure() {
    var host = $('#figure-host');
    var meta = $('#figure-meta');
    var svgBtn = $('#dl-svg');
    var svgNote = $('#svg-note');
    var pngBtn = $('#dl-png');
    if (!host) return;
    host.innerHTML = '';

    var b64 = S.result && S.result.matplotlib_png_base64;
    var svg = S.result && S.result.matplotlib_svg;
    var hasPng = typeof b64 === 'string' && b64.length > 0;
    var hasSvg = typeof svg === 'string' && svg.length > 0 && /^\s*(<\?xml|<svg)/i.test(svg);

    if (svgBtn) {
      svgBtn.hidden = !hasSvg;
      svgBtn.disabled = !hasSvg;
      svgBtn.setAttribute('title', hasSvg ? t('figure.svgAria') : t('figure.svgUnavailable'));
    }
    if (svgNote) svgNote.hidden = hasSvg;
    if (pngBtn) {
      pngBtn.disabled = !hasPng;
      pngBtn.setAttribute('title', hasPng ? t('figure.downloadPngAria') : t('figure.missing'));
    }

    if (!hasPng && !hasSvg) {
      if (meta) meta.textContent = '';
      host.appendChild(el('div', { className: 'wb-empty', text: t('figure.missing') }));
      return;
    }

    /* prefer the raster for display; fall back to the vector if that is all we got */
    var src = hasPng
      ? 'data:image/png;base64,' + b64
      : 'data:image/svg+xml;charset=utf-8,' + encodeURIComponent(svg);
    var img = el('img', { attrs: { alt: t('figure.alt'), src: src } });
    img.addEventListener('load', function () {
      if (!meta) return;
      var w = img.naturalWidth, h = img.naturalHeight;
      if (hasPng && hasSvg) meta.textContent = t('figure.metaBoth', { w: w, h: h });
      else if (hasPng) meta.textContent = t('figure.meta', { w: w, h: h });
      else meta.textContent = t('figure.metaSvgOnly');
    });
    img.addEventListener('error', function () {
      if (meta) meta.textContent = t('figure.missing');
    });
    host.appendChild(img);
  }

  function renderTable() {
    var wrap = $('#table-wrap');
    var note = $('#table-note');
    if (!wrap) return;
    wrap.innerHTML = '';
    var ts = S.result && S.result.timeseries;
    if (!ts || typeof ts !== 'object') {
      if (note) note.textContent = t('table.empty');
      return;
    }
    var cols = TABLE_COLS.filter(function (c) {
      var a = ts[c.key];
      return a && a.length;
    });
    if (!cols.length) {
      if (note) note.textContent = t('table.empty');
      return;
    }
    var len = ts.time_s ? ts.time_s.length : Math.max.apply(null, cols.map(function (c) { return ts[c.key].length; }));
    var stride = len > MAX_TABLE_ROWS ? Math.ceil(len / MAX_TABLE_ROWS) : 1;
    var shown = 0;

    var html = ['<table class="data-table"><thead><tr>'];
    cols.forEach(function (c) { html.push('<th>' + esc(c.label()) + '</th>'); });
    html.push('</tr></thead><tbody>');
    for (var i = 0; i < len; i += stride) {
      html.push('<tr>');
      for (var j = 0; j < cols.length; j++) {
        var arr = ts[cols[j].key];
        html.push('<td>' + esc(fmtNum(arr ? arr[i] : null)) + '</td>');
      }
      html.push('</tr>');
      shown++;
    }
    html.push('</tbody></table>');
    wrap.innerHTML = html.join('');

    if (note) {
      note.textContent = stride > 1
        ? t('table.noteDown', { k: stride, n: shown, m: len })
        : t('table.note', { n: len });
    }
  }

  /* ------------------------------------------------------------------ *
   * Export modal
   * ------------------------------------------------------------------ */
  function escapeHtml(s) {
    return String(s == null ? '' : s).replace(/&/g, '&amp;').replace(/</g, '&lt;').replace(/>/g, '&gt;');
  }

  var PY_RE = new RegExp(
    '(#[^\\n]*)' +                                              /* 1 comment */
    '|("""[\\s\\S]*?"""|\'\'\'[\\s\\S]*?\'\'\'|"(?:[^"\\\\\\n]|\\\\.)*"|\'(?:[^\'\\\\\\n]|\\\\.)*\')' + /* 2 string */
    '|(\\b\\d+(?:\\.\\d+)?(?:[eE][-+]?\\d+)?\\b)' +              /* 3 number */
    '|\\b(import|from|as|if|else|elif|for|in|not|and|or|is|None|True|False|def|return|with|lambda|while|class|pass|continue|break|try|except|raise|assert|yield|global|del|print)\\b' + /* 4 keyword */
    '|\\b([A-Za-z_]\\w*)(?=\\s*\\()',                            /* 5 call */
    'g'
  );

  function highlightLine(line) {
    var out = escapeHtml(line);
    return out.replace(PY_RE, function (m, com, str, num, kw, fn) {
      if (com) return '<span class="tok-com">' + com + '</span>';
      if (str) return '<span class="tok-str">' + str + '</span>';
      if (num) return '<span class="tok-num">' + num + '</span>';
      if (kw) return '<span class="tok-kw">' + kw + '</span>';
      if (fn) return '<span class="tok-fn">' + fn + '</span>';
      return m;
    });
  }

  function renderExportModal() {
    var body = $('#modal-body');
    var meta = $('#modal-meta');
    var copyBtn = $('#copy-btn');
    var dlBtn = $('#download-btn');
    var copyLabel = $('#copy-label');
    if (!body) return;
    if (copyLabel) copyLabel.textContent = t('export.copy');
    if (copyBtn) copyBtn.disabled = S.exportState !== 'ready';
    if (dlBtn) dlBtn.disabled = S.exportState !== 'ready';

    if (S.exportState === 'loading') {
      body.className = 'modal-body is-message';
      body.innerHTML = '';
      var line = el('div', { className: 'loading-line' });
      line.appendChild(el('span', { className: 'spinner spinner-lg' }));
      line.appendChild(el('div', { text: t('export.loading') }));
      body.appendChild(line);
      if (meta) meta.textContent = '';
      return;
    }

    if (S.exportState === 'error') {
      body.className = 'modal-body is-message';
      body.innerHTML = '';
      body.appendChild(el('div', { text: t('export.error', { msg: S.exportError }) }));
      if (meta) meta.textContent = '';
      return;
    }

    if (S.exportState !== 'ready' || !S.exportCode) {
      body.className = 'modal-body is-message';
      body.innerHTML = '';
      body.appendChild(el('div', { text: t('export.empty') }));
      if (meta) meta.textContent = '';
      return;
    }

    body.className = 'modal-body';
    var lines = S.exportCode.split('\n');
    if (lines.length && lines[lines.length - 1] === '') lines.pop();
    var html = ['<pre class="code-block">'];
    lines.forEach(function (ln, i) {
      html.push('<div class="code-line"><span class="code-ln">' + (i + 1) + '</span>' +
        '<span class="code-src">' + (ln ? highlightLine(ln) : '&nbsp;') + '</span></div>');
    });
    html.push('</pre>');
    body.innerHTML = html.join('');

    if (meta) {
      meta.textContent = t('export.meta', {
        lines: lines.length,
        kb: (S.exportCode.length / 1024).toFixed(1)
      });
    }
  }

  function openModal() {
    var backdrop = $('#modal-backdrop');
    if (!backdrop) return;
    S.modalOpen = true;
    backdrop.hidden = false;
    doc.body.classList.add('modal-open');
    var closeBtn = $('#modal-close');
    if (closeBtn) closeBtn.focus();
  }

  function closeModal() {
    var backdrop = $('#modal-backdrop');
    if (!backdrop || !S.modalOpen) return;
    S.modalOpen = false;
    backdrop.hidden = true;
    S.exportToken++;                       /* invalidate in-flight responses */
    doc.body.classList.remove('modal-open');
    var btn = $('#export-btn');
    if (btn) btn.focus();
  }

  function exportPython() {
    if (!S.presetId || !S.cycleId) {
      showBanner({ title: t('err.export'), message: t('err.missing'), kind: 'error' });
      return;
    }
    S.exportState = 'loading';
    S.exportCode = '';
    S.exportError = '';
    S.exporting = true;
    renderExportModal();
    openModal();
    var token = ++S.exportToken;
    var payload = {
      preset: S.presetId,
      cycle: S.cycleId,
      overrides: collectOverrides()
    };
    fetchExport(payload).then(function (data) {
      if (token !== S.exportToken) return;
      S.exporting = false;
      var code = data && typeof data.code === 'string' ? data.code : '';
      S.exportCode = code;
      S.exportState = code ? 'ready' : 'error';
      if (!code) S.exportError = t('export.empty');
      renderExportModal();
    }).catch(function (err) {
      if (token !== S.exportToken) return;
      S.exporting = false;
      S.exportState = 'error';
      S.exportError = (err && err.message) || String(err);
      renderExportModal();
    });
  }

  function copyText(text) {
    return new Promise(function (resolve) {
      var fallback = function () { resolve(fallbackCopy(text)); };
      if (global.navigator && global.navigator.clipboard && global.navigator.clipboard.writeText) {
        global.navigator.clipboard.writeText(text).then(function () { resolve(true); }, fallback);
      } else {
        fallback();
      }
    });
  }

  function fallbackCopy(text) {
    var ta = el('textarea', { attrs: { readonly: 'readonly' } });
    ta.value = text;
    ta.style.position = 'fixed';
    ta.style.top = '-2000px';
    ta.style.opacity = '0';
    doc.body.appendChild(ta);
    ta.select();
    var ok = false;
    try { ok = doc.execCommand('copy'); } catch (e) { ok = false; }
    doc.body.removeChild(ta);
    return ok;
  }

  function downloadBlob(content, mime, filename) {
    try {
      var blob = new global.Blob([content], { type: mime });
      var url = global.URL.createObjectURL(blob);
      var a = el('a', { attrs: { href: url, download: filename } });
      a.style.display = 'none';
      doc.body.appendChild(a);
      a.click();
      global.setTimeout(function () {
        doc.body.removeChild(a);
        global.URL.revokeObjectURL(url);
      }, 500);
      return url;
    } catch (e) {
      showBanner({ title: t('err.export'), message: (e && e.message) || String(e), kind: 'error' });
      return null;
    }
  }

  function base64ToBytes(b64) {
    var bin = global.atob(b64);
    var bytes = new Uint8Array(bin.length);
    for (var i = 0; i < bin.length; i++) bytes[i] = bin.charCodeAt(i);
    return bytes;
  }

  /** filesystem-safe slug: "2012 Nissan Leaf 30 kWh thrml.yaml" -> "2012-Nissan-Leaf-30-kWh-thrml" */
  function slug(value, fallback) {
    var out = String(value == null ? '' : value)
      .replace(/\.[A-Za-z0-9]+$/, '')
      .replace(/[^A-Za-z0-9._-]+/g, '-')
      .replace(/-+/g, '-')
      .replace(/^[-._]+|[-._]+$/g, '');
    return out || fallback;
  }

  /** names the download after the run that produced the figure, not the current form state */
  function figureFileName(ext) {
    var meta = S.resultMeta || {};
    var preset = slug(meta.preset || S.presetId, 'vehicle');
    var cycle = slug(meta.cycle || S.cycleId, 'cycle');
    return 'fastsim-' + preset + '-' + cycle + '.' + ext;
  }

  function downloadPng() {
    var b64 = S.result && S.result.matplotlib_png_base64;
    if (!b64) return;
    try {
      downloadBlob(base64ToBytes(b64), 'image/png', figureFileName('png'));
    } catch (e) {
      showBanner({ title: t('err.export'), message: (e && e.message) || String(e), kind: 'error' });
    }
  }

  function downloadSvg() {
    var svg = S.result && S.result.matplotlib_svg;
    if (!svg) return;
    downloadBlob(svg, 'image/svg+xml;charset=utf-8', figureFileName('svg'));
  }

  /* ------------------------------------------------------------------ *
   * Language
   * ------------------------------------------------------------------ */
  function syncLangButtons() {
    var btns = doc.querySelectorAll('.lang-btn');
    for (var i = 0; i < btns.length; i++) {
      var on = btns[i].getAttribute('data-lang') === I18N.lang;
      btns[i].classList.toggle('is-active', on);
      btns[i].setAttribute('aria-pressed', on ? 'true' : 'false');
    }
  }

  function setLanguage(lang) {
    var scroll = $('#params-root') ? $('#params-root').scrollTop : 0;
    I18N.setLang(lang);
    syncLangButtons();
    I18N.applyStatic();

    /* every dynamic region re-renders from state — no refetch, no re-run */
    if (S.presets.length) renderPresetSelect();
    if (S.cycles.length) renderCycleSelect();
    renderParams();
    var host = $('#params-root');
    if (host) host.scrollTop = scroll;

    var runLabel = $('#run-label');
    if (runLabel) runLabel.textContent = S.loading ? t('toolbar.running') : t('toolbar.run');
    var wbToggle = $('#wb-toggle');
    var collapsed = $('#app').classList.contains('wb-collapsed') || !$('#app').classList.contains('wb-open');
    if (wbToggle) wbToggle.setAttribute('title', t(collapsed ? 'toolbar.expand' : 'toolbar.collapse'));

    renderStaleChip();
    updateStatus();
    if (S.result) {
      renderKpis();
      setTab(S.tab);
      refreshFigureLang();
    }
    if (S.modalOpen) renderExportModal();
  }

  /* Re-render the matplotlib figure in the newly selected language.
     The backend renders its titles/axis labels server-side, so a language
     switch needs one silent round trip. Numbers are identical across
     languages, so only the figure payload is swapped; on any failure the
     previous figure is kept and nothing is shown to the user. */
  var _figLangToken = 0;
  function refreshFigureLang() {
    if (S.mock || !S.result || !S.resultMeta) return;
    if (!S.result.matplotlib_png_base64 && !S.result.matplotlib_svg) return;
    var token = ++_figLangToken;
    var payload = {
      preset: S.resultMeta.preset,
      cycle: S.resultMeta.cycle,
      overrides: S.resultMeta.overrides || {},
      render_matplotlib: true,
      lang: I18N.lang
    };
    fetchSimulate(payload).then(function (data) {
      if (token !== _figLangToken) return;            /* superseded by a newer switch */
      if (!data || typeof data !== 'object') return;
      if (!data.matplotlib_png_base64 && !data.matplotlib_svg) return;
      S.result.matplotlib_png_base64 = data.matplotlib_png_base64;
      S.result.matplotlib_svg = data.matplotlib_svg;
      S.result.lang = data.lang;
      setTab(S.tab);
    }).catch(function () { /* best effort: keep the current figure */ });
  }

  /* ------------------------------------------------------------------ *
   * Layout toggle
   * ------------------------------------------------------------------ */
  function toggleWorkbench(force) {
    var app = $('#app');
    var narrow = global.innerWidth <= 1040;
    if (narrow) {
      var open = force !== undefined ? force : !app.classList.contains('wb-open');
      app.classList.remove('wb-collapsed');
      app.classList.toggle('wb-open', open);
    } else {
      var collapsed = force !== undefined ? force : !app.classList.contains('wb-collapsed');
      app.classList.remove('wb-open');
      app.classList.toggle('wb-collapsed', collapsed);
      var btn = $('#wb-collapse');
      if (btn) {
        btn.setAttribute('title', t(collapsed ? 'toolbar.expand' : 'toolbar.collapse'));
        btn.textContent = collapsed ? '›' : '‹';
      }
    }
    /* plot must re-measure after the grid column moves */
    global.setTimeout(function () {
      if (S.result && S.tab === 'interactive') CHARTS.resize($('#plot'));
    }, 80);
  }

  /* ------------------------------------------------------------------ *
   * Events
   * ------------------------------------------------------------------ */
  function onPresetChange() {
    var sel = $('#preset-select');
    var id = sel && sel.value;
    if (!id || id === S.presetId) return;
    var prev = S.presetId;
    hideBanner();
    loadSchema(id).catch(function (err) {
      /* keep the previous tree on screen; roll the select back */
      S.presetId = prev;
      if (sel && prev) sel.value = prev;
      showBanner({
        title: t('err.schema'),
        message: err && err.message ? err.message : String(err),
        detail: err && err.detail ? err.detail : '',
        kind: 'error',
        actions: [{ label: t('err.retry'), onClick: function () { if (sel) sel.value = id; onPresetChange(); } }]
      });
    });
  }

  function onCycleChange() {
    var sel = $('#cycle-select');
    if (!sel || !sel.value) return;
    S.cycleId = sel.value;
    markStale();
    updateStatus();
  }

  function bindEvents() {
    var presetSel = $('#preset-select');
    if (presetSel) presetSel.addEventListener('change', onPresetChange);
    var cycleSel = $('#cycle-select');
    if (cycleSel) cycleSel.addEventListener('change', onCycleChange);

    var runBtn = $('#run-btn');
    if (runBtn) runBtn.addEventListener('click', runSimulation);
    var exportBtn = $('#export-btn');
    if (exportBtn) exportBtn.addEventListener('click', exportPython);

    var langSwitch = $('#lang-switch');
    if (langSwitch) {
      langSwitch.addEventListener('click', function (ev) {
        var btn = ev.target && ev.target.closest ? ev.target.closest('.lang-btn') : null;
        if (!btn) return;
        var lang = btn.getAttribute('data-lang');
        if (lang && lang !== I18N.lang) setLanguage(lang);
      });
    }

    var search = $('#param-search');
    if (search) {
      search.addEventListener('input', function () {
        S.search = search.value || '';
        applyFilters();
      });
      search.addEventListener('keydown', function (ev) {
        if (ev.key === 'Escape' && search.value) {
          search.value = '';
          S.search = '';
          applyFilters();
          ev.stopPropagation();
        }
      });
    }

    var modOnly = $('#modified-only');
    if (modOnly) {
      modOnly.addEventListener('change', function () {
        S.modifiedOnly = !!modOnly.checked;
        applyFilters();
      });
    }

    var resetAllBtn = $('#reset-all');
    if (resetAllBtn) resetAllBtn.addEventListener('click', resetAll);

    var wbCollapse = $('#wb-collapse');
    if (wbCollapse) wbCollapse.addEventListener('click', function () { toggleWorkbench(true); });
    var wbToggle = $('#wb-toggle');
    if (wbToggle) wbToggle.addEventListener('click', function () { toggleWorkbench(); });

    var tabs = $('#tabs');
    if (tabs) {
      tabs.addEventListener('click', function (ev) {
        var btn = ev.target && ev.target.closest ? ev.target.closest('.tab') : null;
        if (!btn) return;
        setTab(btn.getAttribute('data-tab'));
      });
    }

    var modalClose = $('#modal-close');
    if (modalClose) modalClose.addEventListener('click', closeModal);
    var backdrop = $('#modal-backdrop');
    if (backdrop) {
      backdrop.addEventListener('mousedown', function (ev) {
        if (ev.target === backdrop) closeModal();
      });
    }
    var copyBtn = $('#copy-btn');
    if (copyBtn) {
      copyBtn.addEventListener('click', function () {
        if (!S.exportCode) return;
        copyText(S.exportCode).then(function (ok) {
          var label = $('#copy-label');
          if (!label) return;
          label.textContent = ok ? t('export.copied') : t('export.copy');
          global.setTimeout(function () {
            if ($('#copy-label')) $('#copy-label').textContent = t('export.copy');
          }, 1600);
        });
      });
    }
    var dlBtn = $('#download-btn');
    if (dlBtn) {
      dlBtn.addEventListener('click', function () {
        if (S.exportCode) downloadBlob(S.exportCode, 'text/x-python', t('export.fileName'));
      });
    }
    var dlPng = $('#dl-png');
    if (dlPng) dlPng.addEventListener('click', downloadPng);
    var dlSvg = $('#dl-svg');
    if (dlSvg) dlSvg.addEventListener('click', downloadSvg);

    doc.addEventListener('keydown', function (ev) {
      if (ev.key === 'Escape' && S.modalOpen) {
        ev.preventDefault();
        closeModal();
        return;
      }
      if ((ev.ctrlKey || ev.metaKey) && ev.key === 'Enter') {
        ev.preventDefault();
        runSimulation();
      }
    });

    var onResize = debounce(function () {
      if (S.result && S.tab === 'interactive') CHARTS.resize($('#plot'));
    }, 160);
    global.addEventListener('resize', onResize);

    if (global.ResizeObserver) {
      var plotHost = $('#plot');
      if (plotHost) {
        try {
          var ro = new global.ResizeObserver(onResize);
          ro.observe(plotHost);
        } catch (e) { /* observer unsupported — window resize still works */ }
      }
    }
  }

  /* ------------------------------------------------------------------ *
   * Boot
   * ------------------------------------------------------------------ */
  function detectMock() {
    var params;
    try {
      params = new global.URLSearchParams(global.location.search);
    } catch (e) {
      return;
    }
    var raw = params.get('mock');
    if (raw === null) return;
    var v = String(raw).toLowerCase();
    if (v === '' || v === '1' || v === 'true' || v === 'yes') S.mock = true;
    else if (v === 'auto') S.mockAuto = true;   /* try the real API, fall back on failure */
  }

  function showMockBanner() {
    showBanner({ title: t('mock.bannerTitle'), message: t('mock.note'), kind: 'info' });
  }

  function boot() {
    I18N.setLang(I18N.readStored());
    syncLangButtons();
    I18N.applyStatic();
    detectMock();
    setMockUi();
    bindEvents();
    if (S.mock) showMockBanner();
    updateStatus();

    loadSchema(null).catch(function (err) {
      var host = $('#params-root');
      if (host) {
        host.innerHTML = '';
        host.appendChild(el('div', { className: 'wb-empty', text: t('panel.loadFailed') }));
      }
      showBanner({
        title: t('err.schema'),
        message: err && err.message ? err.message : String(err),
        detail: err && err.detail ? err.detail : '',
        kind: 'error',
        actions: [
          { label: t('err.retry'), onClick: function () { hideBanner(); loadSchema(S.presetId).catch(function (e2) {
            showBanner({ title: t('err.schema'), message: e2 && e2.message ? e2.message : String(e2), kind: 'error' });
          }); } },
          { label: t('err.useMock'), onClick: function () { S.mock = true; setMockUi(); hideBanner(); loadSchema(S.presetId).catch(function () {}); } }
        ]
      });
    });
  }

  /* dev hook: lets a headless harness (and the console) inspect state */
  global.__fastsim = {
    state: S,
    i18n: I18N,
    loadSchema: loadSchema,
    applySchema: applySchema,
    runSimulation: runSimulation,
    exportPython: exportPython,
    setLanguage: setLanguage,
    setTab: setTab,
    applyFilters: applyFilters,
    collectOverrides: collectOverrides,
    isModified: isModified,
    pruneOverride: pruneOverride,
    renderParams: renderParams,
    renderKpis: renderKpis,
    renderResult: renderResult,
    renderChart: renderChart,
    renderFigure: renderFigure,
    renderTable: renderTable,
    highlightLine: highlightLine,
    closeModal: closeModal
  };

  if (doc.readyState === 'loading') {
    doc.addEventListener('DOMContentLoaded', boot);
  } else {
    boot();
  }
})(window);
