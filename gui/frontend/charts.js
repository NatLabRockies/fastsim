/* ==========================================================================
   charts.js — Plotly multi-panel interactive view.
   make_subplots-style: one 2x2 (or 4x1) grid of axis domains, x axes matched
   so zoom/pan stays in sync across panels.
   Depends on: /vendor/plotly.min.js (local, never a CDN) + FSI18N.
   ========================================================================== */
(function (global) {
  'use strict';

  var I18N = global.FSI18N;
  var MAX_POINTS = 4000;      /* guard: downsample absurdly long runs */
  var NARROW_PX = 780;        /* below this width -> single column */

  var lastRender = null;      /* {host, result} so a resize can redraw */

  function cssVar(name, fallback) {
    try {
      var v = global.getComputedStyle(global.document.documentElement).getPropertyValue(name);
      v = (v || '').trim();
      return v || fallback;
    } catch (e) { return fallback; }
  }

  function palette() {
    return {
      target: cssVar('--c-target', '#8c99a9'),
      achieved: cssVar('--c-achieved', '#1a5fd0'),
      engine: cssVar('--c-engine', '#c2410c'),
      motor: cssVar('--c-motor', '#0f766e'),
      battery: cssVar('--c-battery', '#6d28d9'),
      fuel: cssVar('--c-fuel', '#b45309'),
      energy: cssVar('--c-energy', '#1a5fd0'),
      grid: cssVar('--grid', '#e7ecf2'),
      text: cssVar('--text-2', '#45536a'),
      title: cssVar('--text', '#131b26'),
      muted: cssVar('--text-dim', '#93a0b1'),
      font: cssVar('--font-sans', 'Segoe UI, Arial, sans-serif')
    };
  }

  function num(v) {
    return typeof v === 'number' && isFinite(v);
  }

  function hasSeries(ts, key) {
    var a = ts ? ts[key] : null;
    return !!(a && a.length);
  }

  /** True when `timeseries` carries anything worth plotting. */
  function hasData(result) {
    var ts = result && result.timeseries;
    if (!ts || typeof ts !== 'object') return false;
    if (hasSeries(ts, 'time_s')) return true;
    var keys = Object.keys(ts);
    for (var i = 0; i < keys.length; i++) {
      if (hasSeries(ts, keys[i])) return true;
    }
    return false;
  }

  function strideFor(len) {
    return len > MAX_POINTS ? Math.ceil(len / MAX_POINTS) : 1;
  }

  function slice(arr, stride) {
    if (!arr || stride <= 1) return arr || [];
    var out = [];
    for (var i = 0; i < arr.length; i += stride) out.push(arr[i]);
    /* keep the final sample so the curve ends where it should */
    if (arr.length && out[out.length - 1] !== arr[arr.length - 1]) out.push(arr[arr.length - 1]);
    return out;
  }

  /** grid geometry: 2 columns on wide hosts, 1 column when narrow */
  function geometry(width) {
    var twoCol = width >= NARROW_PX;
    if (twoCol) {
      return {
        twoCol: true,
        cols: [[0.0, 0.455], [0.545, 1.0]],
        rows: [[0.565, 0.955], [0.055, 0.445]],
        height: Math.max(560, Math.min(660, Math.round(width * 0.52))),
        titleOffsets: [0.038, 0.038]
      };
    }
    return {
      twoCol: false,
      cols: [[0.0, 1.0]],
      rows: [[0.775, 0.955], [0.545, 0.725], [0.315, 0.495], [0.085, 0.265]],
      height: 860,
      titleOffsets: [0.022, 0.022, 0.022, 0.022]
    };
  }

  function panel(i, geo) {
    return { col: geo.twoCol ? i % 2 : 0, row: geo.twoCol ? (i < 2 ? 0 : 1) : i };
  }

  function axisIds(index) {
    return index === 0
      ? { x: 'x', y: 'y' }
      : { x: 'x' + (index + 1), y: 'y' + (index + 1) };
  }

  function buildAxes(geo, labels) {
    var layout = {};
    for (var i = 0; i < 4; i++) {
      var p = panel(i, geo);
      var ids = axisIds(i);
      var xKey = ids.x === 'x' ? 'xaxis' : 'xaxis' + ids.x.slice(1);
      var yKey = ids.y === 'y' ? 'yaxis' : 'yaxis' + ids.y.slice(1);

      var xCfg = {
        domain: geo.cols[p.col],
        anchor: ids.y,
        showgrid: true,
        gridcolor: palette().grid,
        gridwidth: 1,
        zeroline: false,
        linecolor: palette().grid,
        tickfont: { size: 10.5, color: palette().muted },
        hoverformat: '.1f',
        ticks: 'outside',
        ticklen: 3,
        tickcolor: palette().grid,
        showspikes: false
      };
      if (i === 0) {
        xCfg.title = { text: labels.time, font: { size: 11, color: palette().text } };
      }
      if (i > 0) {
        xCfg.matches = 'x';          /* linked zoom across panels */
        xCfg.showticklabels = true;
      }

      var yCfg = {
        domain: geo.rows[p.row],
        anchor: ids.x,
        showgrid: true,
        gridcolor: palette().grid,
        gridwidth: 1,
        zeroline: false,
        zerolinewidth: 1,
        zerolinecolor: '#c9d2dd',
        linecolor: palette().grid,
        tickfont: { size: 10.5, color: palette().muted },
        title: { text: labels.axes[i], font: { size: 11.5, color: palette().text }, standoff: 6 },
        ticks: 'outside',
        ticklen: 3,
        tickcolor: palette().grid
      };
      if (labels.zeroLine[i]) yCfg.zeroline = true;
      if (labels.tickSuffix[i]) yCfg.ticksuffix = labels.tickSuffix[i];

      layout[xKey] = xCfg;
      layout[yKey] = yCfg;
    }
    return layout;
  }

  function annotations(geo, titles, naFlags, t) {
    var out = [];
    for (var i = 0; i < 4; i++) {
      var p = panel(i, geo);
      var x0 = geo.cols[p.col][0];
      var yTop = geo.rows[p.row][1] + geo.titleOffsets[p.row];
      out.push({
        text: '<b>' + titles[i] + '</b>',
        xref: 'paper', yref: 'paper',
        x: x0, y: yTop,
        xanchor: 'left', yanchor: 'bottom',
        showarrow: false,
        font: { size: 12.5, color: palette().title, family: palette().font }
      });
      if (naFlags[i]) {
        out.push({
          text: t('chart.na'),
          xref: 'paper', yref: 'paper',
          x: (geo.cols[p.col][0] + geo.cols[p.col][1]) / 2,
          y: (geo.rows[p.row][0] + geo.rows[p.row][1]) / 2,
          xanchor: 'center', yanchor: 'middle',
          showarrow: false,
          font: { size: 11.5, color: palette().muted, family: palette().font }
        });
      }
    }
    return out;
  }

  function trace(name, x, y, opts) {
    var o = {
      type: 'scatter',
      mode: 'lines',
      name: name,
      x: x,
      y: y,
      line: { width: opts.width || 2, color: opts.color, shape: opts.shape || 'linear' },
      hovertemplate: opts.hover,
      connectgaps: false
    };
    if (opts.axis) { o.xaxis = opts.axis.x; o.yaxis = opts.axis.y; }
    if (opts.dash) o.line.dash = opts.dash;
    if (opts.fill) { o.fill = 'tozeroy'; o.fillcolor = opts.fill; }
    if (opts.legendGroup) o.legendgroup = opts.legendGroup;
    if (opts.hoverlabelName) o.hoverlabel = { namelength: -1 };
    return o;
  }

  /**
   * Draw the interactive figure.
   * @param {HTMLElement} host
   * @param {{kpis:object,timeseries:object}} result
   * @returns {{ok:boolean, message?:string, panels?:number}}
   */
  function render(host, result) {
    var t = function (k) { return I18N.t(k); };
    lastRender = { host: host, result: result };

    if (!host || !global.Plotly || typeof global.Plotly.newPlot !== 'function') {
      return { ok: false, message: 'plotly-unavailable' };
    }
    if (!hasData(result)) {
      return { ok: false, message: 'no-timeseries' };
    }

    var ts = result.timeseries || {};
    var C = palette();
    var width = host.clientWidth || 900;
    var geo = geometry(width);

    var time = ts.time_s || [];
    var stride = strideFor(time.length);
    var x = slice(time, stride);

    var labels = {
      time: t('axis.time'),
      axes: [t('axis.speed'), t('axis.power'), null, t('axis.energy')],
      zeroLine: [false, true, false, false],
      tickSuffix: ['', '', '', '']
    };

    var panel3IsSoc = hasSeries(ts, 'soc_pct');
    var panel3IsDist = !panel3IsSoc && hasSeries(ts, 'cum_distance_mi');
    labels.axes[2] = panel3IsSoc ? t('axis.soc') : (panel3IsDist ? t('axis.distance') : t('axis.soc'));

    var titles = [t('chart.speed'), t('chart.power'),
      panel3IsSoc ? t('chart.soc') : (panel3IsDist ? t('chart.distance') : t('chart.soc')),
      t('chart.energy')];
    var naFlags = [false, false, !(panel3IsSoc || panel3IsDist), false];

    var traces = [];
    var axis0 = { x: 'x', y: 'y' };
    var axis1 = { x: 'x2', y: 'y2' };
    var axis2 = { x: 'x3', y: 'y3' };
    var axis3 = { x: 'x4', y: 'y4' };

    /* ---- panel 1: speed target vs achieved ---- */
    var speedCount = 0;
    if (hasSeries(ts, 'speed_target_mps')) {
      traces.push(trace(t('trace.target'), x, slice(ts.speed_target_mps, stride), {
        color: C.target, width: 1.8, dash: 'dash', axis: axis0,
        hover: '%{y:.2f} m/s'
      }));
      speedCount++;
    }
    if (hasSeries(ts, 'speed_achieved_mps')) {
      traces.push(trace(t('trace.achieved'), x, slice(ts.speed_achieved_mps, stride), {
        color: C.achieved, width: 2.2, axis: axis0,
        hover: '%{y:.2f} m/s'
      }));
      speedCount++;
    }
    if (!speedCount) naFlags[0] = true;

    /* ---- panel 2: powertrain power split (whichever exist) ---- */
    var powerCount = 0;
    if (hasSeries(ts, 'engine_power_kw')) {
      traces.push(trace(t('trace.engine'), x, slice(ts.engine_power_kw, stride), {
        color: C.engine, width: 1.9, axis: axis1, hover: '%{y:.2f} kW'
      }));
      powerCount++;
    }
    if (hasSeries(ts, 'motor_power_kw')) {
      traces.push(trace(t('trace.motor'), x, slice(ts.motor_power_kw, stride), {
        color: C.motor, width: 1.9, axis: axis1, hover: '%{y:.2f} kW'
      }));
      powerCount++;
    }
    if (hasSeries(ts, 'battery_power_kw')) {
      traces.push(trace(t('trace.battery'), x, slice(ts.battery_power_kw, stride), {
        color: C.battery, width: 1.9, axis: axis1, hover: '%{y:.2f} kW'
      }));
      powerCount++;
    }
    if (!powerCount) naFlags[1] = true;

    /* ---- panel 3: SOC, else cumulative distance ---- */
    if (panel3IsSoc) {
      traces.push(trace(t('trace.soc'), x, slice(ts.soc_pct, stride), {
        color: C.battery, width: 2.1, axis: axis2, hover: '%{y:.2f} %'
      }));
    } else if (panel3IsDist) {
      traces.push(trace(t('trace.distance'), x, slice(ts.cum_distance_mi, stride), {
        color: C.achieved, width: 2.1, axis: axis2, hover: '%{y:.3f} mi'
      }));
    }

    /* ---- panel 4: cumulative fuel + energy ---- */
    var energyCount = 0;
    if (hasSeries(ts, 'cum_fuel_kwh')) {
      traces.push(trace(t('trace.fuel'), x, slice(ts.cum_fuel_kwh, stride), {
        color: C.fuel, width: 2, axis: axis3, hover: '%{y:.3f} kWh'
      }));
      energyCount++;
    }
    if (hasSeries(ts, 'cum_energy_kwh')) {
      traces.push(trace(t('trace.energy'), x, slice(ts.cum_energy_kwh, stride), {
        color: C.energy, width: 2, dash: 'dot', axis: axis3, hover: '%{y:.3f} kWh'
      }));
      energyCount++;
    }
    if (!energyCount) naFlags[3] = true;

    var layout = {
      height: geo.height,
      autosize: true,
      paper_bgcolor: '#ffffff',
      plot_bgcolor: '#ffffff',
      margin: geo.twoCol ? { l: 58, r: 26, t: 74, b: 46 } : { l: 58, r: 26, t: 60, b: 40 },
      font: { family: C.font, size: 11.5, color: C.text },
      hovermode: 'x unified',
      hoverdistance: 24,
      dragmode: 'zoom',
      showlegend: true,
      legend: {
        orientation: 'h',
        x: 0,
        y: 1.02,
        xanchor: 'left',
        yanchor: 'bottom',
        font: { size: 11.5, color: C.text },
        bgcolor: 'rgba(255,255,255,0.7)',
        bordercolor: 'rgba(0,0,0,0)',
        itemclick: 'toggle',
        itemdoubleclick: 'toggleothers'
      },
      hoverlabel: {
        bgcolor: '#ffffff',
        bordercolor: C.grid,
        font: { size: 11.5, family: C.font, color: C.title },
        align: 'left'
      },
      annotations: []
    };

    var axes = buildAxes(geo, labels);
    Object.keys(axes).forEach(function (k) { layout[k] = axes[k]; });
    layout.annotations = annotations(geo, titles, naFlags, t);

    var config = {
      responsive: true,
      displaylogo: false,
      scrollZoom: true,
      doubleClick: 'reset',
      displayModeBar: 'hover',
      modeBarButtonsToRemove: ['select2d', 'lasso2d', 'toggleSpikelines', 'hoverClosestCartesian', 'hoverCompareCartesian'],
      toImageButtonOptions: { format: 'png', filename: 'fastsim-interactive', scale: 2 }
    };

    try {
      global.Plotly.react(host, traces, layout, config);
      host._fastsimTwoCol = geo.twoCol;
      return { ok: true, panels: 4 };
    } catch (e) {
      return { ok: false, message: (e && e.message) || String(e) };
    }
  }

  function purge(host) {
    if (!host || !global.Plotly || typeof global.Plotly.purge !== 'function') return;
    try { global.Plotly.purge(host); } catch (e) { /* nothing to purge */ }
    host.innerHTML = '';
    lastRender = null;
  }

  /** Resize hook: keeps the plot fluid and re-lays-out when the column count flips. */
  function resize(host) {
    if (!lastRender || !host || !global.Plotly) return;
    try {
      var width = host.clientWidth || 0;
      var geo = geometry(width);
      /* Plotly promotes the host element itself to .js-plotly-plot */
      var plotDiv = (host.classList && host.classList.contains('js-plotly-plot'))
        ? host
        : host.querySelector('.js-plotly-plot');
      if (!plotDiv) return;
      var wasTwoCol = host._fastsimTwoCol;
      if (wasTwoCol !== undefined && wasTwoCol !== geo.twoCol) {
        render(host, lastRender.result);
        return;
      }
      global.Plotly.Plots.resize(host);
    } catch (e) { /* resize is best-effort */ }
  }

  /** Small helper used by app.js to label the plot note. */
  function describeLength(result) {
    var ts = result && result.timeseries;
    return ts && ts.time_s ? ts.time_s.length : 0;
  }

  global.FSCharts = {
    render: render,
    purge: purge,
    resize: resize,
    hasData: hasData,
    describeLength: describeLength
  };
})(window);
