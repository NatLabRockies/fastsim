# FASTSim Studio — a localhost Web GUI for FASTSim

A single-command web workbench for [FASTSim](https://github.com/NatLabRockies/fastsim),
the open-source vehicle powertrain simulator from the National Laboratory of the Rockies.

Pick a vehicle, pick a drive cycle, tweak any parameter, press **Run**, read the KPIs and
the plots, and export the equivalent standalone Python script.

---

## Quick start

```bash
# from the repository root, with the project's Python environment active
python gui/backend/app.py
```

Then open **http://127.0.0.1:8000**.

There is **no build step and no extra dependency**: the backend is pure Python
standard library (`http.server`) and the frontend is plain HTML/CSS/JS with
Plotly.js vendored locally. The only requirements are FASTSim itself plus
`matplotlib` (for the static figure).

```
pip install fastsim matplotlib
python gui/backend/app.py --port 8000
```

---

## What it does

**Every parameter, not a curated subset.** The parameter panel is introspected at
runtime from the selected vehicle's `Vehicle.to_dict()` tree, so it covers the whole
configurable surface — chassis and aero, rolling resistance and wheels, fuel storage,
fuel converter, electric machine, battery/ESS, transmission, controls, thermal and cabin
sub-models, plus solver and cycle settings. The tree changes with the powertrain:
a conventional vehicle exposes `pt_type.Conv.fs`/`.fc`, a battery-electric one exposes
`pt_type.BEV.res`/`.em` and has no fuel converter at all.

| Feature | Notes |
|---|---|
| Instant parameter search | matches path, human label and description, in the active language |
| Subsystem grouping | Vehicle / Chassis / Powertrain / Thermal, with per-group counts |
| Modified-only filter | with badges, per-field reset and reset-all |
| KPIs | distance, fuel energy, electricity, fuel economy, Wh/mi, SOC start/end, energy at wheels, regen, speeds, cycle-trace match — only the ones that apply to the powertrain |
| Interactive charts | Plotly: target vs achieved speed, powertrain power split, SOC, cumulative energy; zoom, pan, hover, legend toggles |
| Static figure | publication-style 4-panel matplotlib rendering, downloadable as PNG **and** SVG |
| Export Python | generates a flat, runnable ~15-line script that reproduces the run |
| Languages | English · 繁體中文 · 简体中文 (including the matplotlib figure) |

## Layout

```
gui/
├── backend/     API + static server (stdlib only)
│   ├── app.py            entry point
│   ├── introspection.py  live parameter-tree introspection
│   ├── metadata.py       units, enums, groups, EN/ZH descriptions
│   ├── simulate.py       override application, run, KPIs, timeseries
│   ├── plots.py          matplotlib panels → PNG / SVG
│   └── export_code.py    standalone script generator
├── frontend/    HTML/CSS/JS, Plotly.js vendored, no CDN
├── verify/      independent end-to-end verification suites
└── CONTRACT.md  the HTTP/JSON contract both halves are built against
```

## HTTP API

The backend and frontend are developed independently against a frozen contract
(`gui/CONTRACT.md`):

- `GET /api/schema[?preset=<id>]` → `{ preset, presets, cycles, parameters[] }`
- `POST /api/simulate` `{ preset, cycle, overrides, render_matplotlib, lang }`
  → `{ kpis, timeseries, matplotlib_png_base64, matplotlib_svg, lang }`
- `POST /api/export-python` `{ preset, cycle, overrides }` → `{ code }`

Overrides are dotted paths into the vehicle tree, e.g.
`{"chassis.drag_coef": 0.30, "pt_type.Conv.fc.pwr_out_max_watts": 140000}`.
An unknown path is rejected with HTTP 400 rather than silently ignored.

## Verification

```bash
python gui/verify/run_all.py          # exits non-zero on failure
```

Five suites (223 checks) cover the endpoint contract, the exported-script
reproduction (executed in a subprocess and via `python -c`), static serving and
path-traversal protection, frontend static sanity, and a 9-vehicle × 2-cycle sweep.
See `gui/verify/REPORT.md`.

## Notes

- Vehicle presets come from the resources bundled with the FASTSim package, so the
  GUI works offline. It deliberately does **not** call `Vehicle.from_db()`, which
  requires network access to the vehicle database repository.
- The mileage constant is `1609.344 m/mi` and is single-sourced, so the API and the
  exported script agree to printed precision.
- Simulations are re-run per request; a run costs a few milliseconds plus ~250 ms for
  the matplotlib figure.
