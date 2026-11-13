# FASTSim Web GUI — Independent Verification Report

Generated: 2026-11-14 02:21:40 China Standard Time  
Verifier: Subagent 3 (verification only — nothing outside `gui/verify/` was created, edited or deleted)  
Contract of record: `gui/CONTRACT.md`

## Verdict: **GREEN**

`223 checks passed, 0 failed, 0 warnings` across 5 suites.

---

## 1. How to start and reach the app

```powershell
# start (single command, one origin serves API + frontend)
& "C:\Users\Pok fun\FASTSim\venv\Scripts\python.exe" gui\backend\app.py --port 8000

# run this verification suite (rewrites this report)
& "C:\Users\Pok fun\FASTSim\venv\Scripts\python.exe" gui\verify\run_all.py
```

- URL verified: **http://127.0.0.1:8000**  
- Server provenance during this run: **pre-existing server**

| Runtime fact | Value |
|---|---|
| python | `3.13.5` |
| python_path | `C:\Users\Pok fun\FASTSim\venv\Scripts\python.exe` |
| fastsim | `3.1.0` |
| node | `v22.18.0` |

### Suites in this run

| Suite | Script | Checks | Passed | Failed | Warnings | Exit |
|---|---|---|---|---|---|---|
| Task 1 — Endpoint contract | `e2e_api.py` | 79 | 79 | 0 | 0 | 0 |
| Task 2 — Exported Python must actually run | `e2e_export.py` | 46 | 46 | 0 | 0 | 0 |
| Task 3 — Static serving + security | `e2e_static.py` | 36 | 36 | 0 | 0 | 0 |
| Task 4 — Frontend static sanity | `e2e_frontend.py` | 34 | 34 | 0 | 0 | 0 |
| Task 5 — Full-sweep robustness | `e2e_sweep.py` | 28 | 28 | 0 | 0 | 0 |
| **Total** | | **223** | **223** | **0** | **0** | 0 |

## 2. Failures

**None.** No check in any of the five suites failed.

## 3. Every check, with the value actually observed

### Task 1 — Endpoint contract  (`e2e_api.py`)

| # | Check | What was checked | Result | Observed | Expected |
|---|---|---|---|---|---|
| 1 | `S01` | GET /api/schema -> HTTP 200 | PASS | `200` | `200` |
| 2 | `S02` | GET /api/schema content-type is JSON | PASS | `application/json; charset=utf-8` | `application/json*` |
| 3 | `S03` | schema.presets count == 9 | PASS | `9` | `9` |
| 4 | `S04` | schema.presets ids match CONTRACT section 2 | PASS | `2012_Ford_Fusion.yaml, 2016_TOYOTA_Prius_Two.yaml, 2016 Nissan Leaf 30 kWh thrml.yaml, 2022 Tesla Model 3 RWD thrml.yaml, 2022_Renault_Zoe_ZE50_R135.yaml, 2020 Chevrolet Bolt EV thrml.yaml, 2021_Hyundai_Sonata_Hybrid_Blue_thrml.yaml, 2026_Chrysler_Pacifica_Select.yaml, 2026_Chrysler_Pacifica_Sele...` | `2012_Ford_Fusion.yaml, 2016_TOYOTA_Prius_Two.yaml, 2016 Nissan Leaf 30 kWh thrml.yaml, 2022 Tesla Model 3 RWD thrml.yaml, 2022_Renault_Zoe_ZE50_R135.yaml, 2020 Chevrolet Bolt EV thrml.yaml, 2021_Hyundai_Sonata_Hybrid_Blue_thrml.yaml, 2026_Chrysler_Pacifica_Select.yaml, 2026_Chrysler_Pacifica_Sele...` |
| 5 | `S05` | every preset carries id/name/pt_type/mass_kg | PASS | `('2012_Ford_Fusion.yaml', 'Conv', 1644.3), ('2016_TOYOTA_Prius_Two.yaml', 'HEV', 1635.0), ('2016 Nissan Leaf 30 kWh thrml.yaml', 'BEV', 1636.0)` | `all four keys present` |
| 6 | `S06` | schema.cycles count == 2 | PASS | `2` | `2` |
| 7 | `S07` | schema.cycles ids are udds.csv + hwfet.csv | PASS | `udds.csv, hwfet.csv` | `udds.csv, hwfet.csv` |
| 8 | `S08` | cycles carry name/name_zh/duration_s | PASS | `('UDDS (City)', 'UDDS（城市）', 1369), ('HWFET (Highway)', 'HWFET（高速）', 766)` | `name, name_zh, int duration_s` |
| 9 | `S09` | schema.parameters count >= 40 | PASS | `46` | `>= 40` |
| 10 | `S10` | every parameter has path/name/group/subgroup/type/default/unit/description/editable | PASS | `46 parameters complete` | `no missing keys` |
| 11 | `S11` | parameter types within the contract set | PASS | `bool, curve, enum, float, int, str` | `bool, curve, enum, float, int, object, str` |
| 12 | `S12` | editable=True only for float/int/bool/enum | PASS | `all editable params are scalar` | `[]` |
| 13 | `S13` | parameter paths are unique | PASS | `unique` | `[]` |
| 14 | `S14` | every parameter path is enabled (no 'None' variants) | PASS | `clean` | `[]` |
| 15 | `S15` | units are drawn from the CONTRACT suffix table | PASS | `-, J, J/kg, W, W/kg, kg, kg·m², m, m/s, m/s², m², s` | `-, J, J/K, J/kg, K, W, W/(m²·K), W/K, W/kg, kg, kg/m³, kg·m², m, m/s, m/s², m², m³, s` |
| 16 | `S15u-drag_coef` | unit parsed from suffix for 'drag_coef' | PASS | `-` | `-` |
| 17 | `S15u-frontal_area_square_meters` | unit parsed from suffix for 'frontal_area_square_meters' | PASS | `m²` | `m²` |
| 18 | `S15u-pwr_out_max_watts` | unit parsed from suffix for 'pwr_out_max_watts' | PASS | `W` | `W` |
| 19 | `S15u-mass_kilograms` | unit parsed from suffix for 'mass_kilograms' | PASS | `kg` | `kg` |
| 20 | `S15u-wheel_radius_meters` | unit parsed from suffix for 'wheel_radius_meters' | PASS | `m` | `m` |
| 21 | `S16` | every parameter has group/subgroup + _zh variants | PASS | `all grouped` | `[]` |
| 22 | `S17` | every parameter has a non-empty English description | PASS | `46` | `46` |
| 23 | `S18` | schema without ?preset echoes the first preset | PASS | `2012_Ford_Fusion.yaml` | `2012_Ford_Fusion.yaml` |
| 24 | `P01` | GET /api/schema?preset=<Conv> -> HTTP 200 | PASS | `200` | `200` |
| 25 | `P02` | Conv schema echoes preset | PASS | `2012_Ford_Fusion.yaml` | `2012_Ford_Fusion.yaml` |
| 26 | `P03` | Conv parameters all use the concrete pt_type.Conv. prefix | PASS | `24` | `no non-Conv pt_type paths` |
| 27 | `P04` | Conv schema exposes fc (fuel converter) paths | PASS | `pt_type.Conv.fc.mass_kilograms, pt_type.Conv.fc.specific_pwr_watts_per_kilogram, pt_type.Conv.fc.pwr_out_max_watts` | `fc paths present` |
| 28 | `P05` | Conv schema has NO res (battery) paths | PASS | `none` | `[]` |
| 29 | `P06` | GET /api/schema?preset=<BEV> -> HTTP 200 | PASS | `200` | `200` |
| 30 | `P07` | BEV schema echoes preset | PASS | `2022 Tesla Model 3 RWD thrml.yaml` | `2022 Tesla Model 3 RWD thrml.yaml` |
| 31 | `P08` | BEV parameters use the concrete pt_type.BEV. prefix | PASS | `20` | `> 0` |
| 32 | `P09` | BEV schema exposes res + em paths | PASS | `{"res": 10, "em": 7}` | `both > 0` |
| 33 | `P10` | BEV schema has NO fc paths | PASS | `none` | `[]` |
| 34 | `P11` | Conv and BEV parameter counts differ | PASS | `Conv=46, BEV=66` | `different` |
| 35 | `C01` | POST /api/simulate (Fusion + UDDS) -> HTTP 200 | PASS | `200` | `200` |
| 36 | `C02` | kpis is a non-empty object | PASS | `avg_speed_mph, distance_mi, energy_to_wheels_kwh, fuel_consumed_kwh, fuel_economy_mpgge, max_speed_mph, trace_met_pct` | `non-empty` |
| 37 | `C03` | every KPI entry has value/unit/label_en/label_zh | PASS | `7 KPIs complete` | `[]` |
| 38 | `C04` | distance_mi ≈ 7.451 (±1%) | PASS | `7.45051` | `7.451 ±1.0%` |
| 39 | `C05` | fuel_consumed_kwh ≈ 7.303 (±2%) | PASS | `7.30331` | `7.303 ±2.0%` |
| 40 | `C06` | fuel_economy_mpgge ≈ 34.38 (±2%) | PASS | `34.3792` | `34.38 ±2.0%` |
| 41 | `C07` | trace_met_pct ≈ 100 (±0.5) | PASS | `100` | `100.0 ±0.5%` |
| 42 | `C08` | distance_mi unit == 'mi' | PASS | `mi` | `mi` |
| 43 | `C09` | fuel_economy_mpgge unit == 'mpgge' | PASS | `mpgge` | `mpgge` |
| 44 | `C10` | Conv exposes NO soc_* KPI | PASS | `none` | `[]` |
| 45 | `C11` | timeseries has time_s / speed_target_mps / speed_achieved_mps | PASS | `cum_distance_mi, cum_energy_kwh, cum_fuel_kwh, engine_power_kw, fuel_power_kw, grade_pct, speed_achieved_mps, speed_target_mps, time_s` | `3 mandatory keys` |
| 46 | `C12` | all timeseries arrays are the same length | PASS | `{"time_s": 1370, "speed_target_mps": 1370, "speed_achieved_mps": 1370, "grade_pct": 1370, "engine_power_kw": 1370, "fuel_power_kw": 1370, "cum_fuel_kwh": 1370, "cum_energy_kwh": 1370, "cum_distance_mi": 1370}` | `one shared length` |
| 47 | `C13` | timeseries length == 1370 (UDDS steps incl. initial state) | PASS | `1370` | `1370` |
| 48 | `C14` | matplotlib_png_base64 decodes to a PNG (magic \x89PNG) | PASS | `198832 bytes, magic=b'\x89PNG'` | `PNG magic` |
| 49 | `C15` | KPI labels ship both EN and ZH text | PASS | `('Distance', '行驶距离'), ('Fuel Energy', '燃料能量'), ('Fuel Economy', '燃油经济性')` | `distinct EN/ZH labels` |
| 50 | `B01` | POST /api/simulate (Tesla BEV + UDDS) -> HTTP 200 | PASS | `200` | `200` |
| 51 | `B02` | soc_start_pct ≈ 98 (±1) | PASS | `98` | `98.0 ±1.0` |
| 52 | `B03` | soc_end_pct ≈ 95.9 (±0.5) | PASS | `95.9087` | `95.9 ±0.5` |
| 53 | `B04` | electric_economy_wh_per_mi ≈ 146.7 (±2%) | PASS | `146.681` | `146.7 ±2.0%` |
| 54 | `B05` | BEV exposes NO fuel_economy_mpgge | PASS | `absent` | `absent` |
| 55 | `B06` | BEV exposes NO fuel_consumed_kwh | PASS | `absent` | `absent` |
| 56 | `B07` | BEV timeseries includes soc_pct | PASS | `battery_power_kw, cum_distance_mi, cum_energy_kwh, grade_pct, motor_power_kw, soc_pct, speed_achieved_mps, speed_target_mps, time_s` | `soc_pct present` |
| 57 | `B08` | BEV timeseries has NO fuel_power_kw | PASS | `absent` | `absent` |
| 58 | `B09` | BEV timeseries arrays are all equal length | PASS | `{"time_s": 1370, "speed_target_mps": 1370, "speed_achieved_mps": 1370, "grade_pct": 1370, "motor_power_kw": 1370, "battery_power_kw": 1370, "soc_pct": 1370, "cum_energy_kwh": 1370, "cum_distance_mi": 1370}` | `one shared length` |
| 59 | `B10` | BEV matplotlib_png_base64 decodes to a PNG | PASS | `255147 bytes` | `PNG magic` |
| 60 | `T01` | POST /api/simulate render_matplotlib:false -> HTTP 200 | PASS | `200` | `200` |
| 61 | `T02` | render_matplotlib:false response carries NO PNG | PASS | `absent` | `absent` |
| 62 | `T03` | render_matplotlib:false response carries NO SVG | PASS | `absent` | `absent` |
| 63 | `T04` | render_matplotlib:false is faster than :true | PASS | `true=589.6 ms, false=31.883399933576584 ms, ratio=18.49x` | `false < true` |
| 64 | `T05` | speed-up magnitude >= 1.15x | PASS | `18.49x` | `>= 1.15x` |
| 65 | `T06` | KPIs identical with and without matplotlib | PASS | `identical` | `identical` |
| 66 | `O01` | override chassis.drag_coef=0.30 -> HTTP 200 | PASS | `200` | `200` |
| 67 | `O02` | fuel_economy_mpgge moves to ≈35.372 | PASS | `35.3722` | `35.372 ±0.5%` |
| 68 | `O03` | override actually changes fuel economy | PASS | `base=34.379216, overridden=35.372215` | `different` |
| 69 | `O04` | unknown override path -> HTTP 400 | PASS | `400` | `400` |
| 70 | `O05` | unknown override error string | PASS | `Unknown parameter path` | `Unknown parameter path` |
| 71 | `O06` | unknown-path 400 body is JSON | PASS | `application/json; charset=utf-8` | `application/json*` |
| 72 | `O07` | unknown preset -> HTTP 4xx | PASS | `400` | `400 or 404` |
| 73 | `L01` | POST /api/simulate lang:'zh' -> HTTP 200 | PASS | `200` | `200` |
| 74 | `L02` | response echoes lang:'zh' | PASS | `zh` | `zh` |
| 75 | `L03` | kpis are byte-identical between zh and en | PASS | `identical` | `identical` |
| 76 | `L04` | timeseries are byte-identical between zh and en | PASS | `identical` | `identical` |
| 77 | `L05` | zh PNG is still a valid PNG | PASS | `196780 bytes` | `PNG magic` |
| 78 | `L06` | zh PNG bytes differ from en (translated figure text) | PASS | `en=198832 B, zh=196780 B, identical=False` | `different` |
| 79 | `L07` | explicit lang:'en' echoes 'en' | PASS | `en` | `en` |

### Task 2 — Exported Python must actually run  (`e2e_export.py`)

| # | Check | What was checked | Result | Observed | Expected |
|---|---|---|---|---|---|
| 1 | `E00` | reference /api/simulate for the same config -> 200 | PASS | `status=200, fuel_economy=35.372215` | `200 + value` |
| 2 | `E01` | POST /api/export-python (Fusion+UDDS+override) -> HTTP 200 | PASS | `200` | `200` |
| 3 | `E02` | response has a 'code' string | PASS | `793 chars` | `non-empty string` |
| 4 | `X01` | exported source is a non-empty string | PASS | `793` | `> 50 chars` |
| 5 | `X02` | exported source contains 'import fastsim' | PASS | `True` | `True` |
| 6 | `X03` | exported source contains NO class definition | PASS | `none` | `no 'class '` |
| 7 | `X04` | exported source contains NO function definition | PASS | `none` | `no 'def '` |
| 8 | `X05` | exported source contains NO main()/print-wrapper | PASS | `False` | `False` |
| 9 | `X06` | no flask/fastapi/http.server/matplotlib/tkinter imports | PASS | `import fastsim` | `only fastsim` |
| 10 | `X07` | every import root is fastsim (or yaml) | PASS | `fastsim` | `{fastsim,yaml}` |
| 11 | `X08` | exported source is short (<= 40 lines) | PASS | `22 lines` | `<= 40` |
| 12 | `X09` | exported source loads the exact preset resource | PASS | `True` | `2012_Ford_Fusion.yaml` |
| 13 | `X10` | exported source calls set_save_interval(1) | PASS | `True` | `True` |
| 14 | `X11` | exported source prints a tidy aligned block | PASS | `print(f"{key:...}` | `True` |
| 15 | `X12` | exported source applies the user's override | PASS | `for path, val in {"chassis.drag_coef": 0.3}.items():` | `override present` |
| 16 | `X13` | no matplotlib/plotting code in the export | PASS | `none` | `none` |
| 17 | `E03` | exported source written to gui/verify/_tmp/exported_fusion.py | PASS | `815 bytes` | `> 50` |
| 18 | `E04` | exported script executed as a FILE exits 0 | PASS | `0` | `0` |
| 19 | `E05` | file run prints a 'Fuel economy' number | PASS | `35.37` | `a number` |
| 20 | `E06` | file-run fuel economy within 1% of /api/simulate | PASS | `api=35.372215, exported=35.37, delta=0.0063%` | `<= 1%` |
| 21 | `E07` | file run distance matches /api/simulate | PASS | `api=7.45051, exported=7.451` | `match` |
| 22 | `E08` | file run stdout has no traceback | PASS | `clean` | `clean` |
| 23 | `E09` | exported source runs INLINE via python -c, exits 0 | PASS | `0` | `0` |
| 24 | `E10` | inline run reproduces the same fuel economy | PASS | `file=35.37, inline=35.37` | `match` |
| 25 | `E11` | inline run needs no file (stdout non-empty) | PASS | `168 chars` | `> 0` |
| 26 | `V01` | POST /api/export-python (Tesla BEV) -> HTTP 200 | PASS | `200` | `200` |
| 27 | `Y01` | exported source is a non-empty string | PASS | `787` | `> 50 chars` |
| 28 | `Y02` | exported source contains 'import fastsim' | PASS | `True` | `True` |
| 29 | `Y03` | exported source contains NO class definition | PASS | `none` | `no 'class '` |
| 30 | `Y04` | exported source contains NO function definition | PASS | `none` | `no 'def '` |
| 31 | `Y05` | exported source contains NO main()/print-wrapper | PASS | `False` | `False` |
| 32 | `Y06` | no flask/fastapi/http.server/matplotlib/tkinter imports | PASS | `import fastsim` | `only fastsim` |
| 33 | `Y07` | every import root is fastsim (or yaml) | PASS | `fastsim` | `{fastsim,yaml}` |
| 34 | `Y08` | exported source is short (<= 40 lines) | PASS | `18 lines` | `<= 40` |
| 35 | `Y09` | exported source loads the exact preset resource | PASS | `True` | `2022 Tesla Model 3 RWD thrml.yaml` |
| 36 | `Y10` | exported source calls set_save_interval(1) | PASS | `True` | `True` |
| 37 | `Y11` | exported source prints a tidy aligned block | PASS | `print(f"{key:...}` | `True` |
| 38 | `Y12` | BEV export prints SOC (start + end) | PASS | `{"SOC start": true, "SOC end": true}` | `{"SOC start": true, "SOC end": true}` |
| 39 | `Y13` | BEV export prints electricity economy, not mpgge | PASS | `{"electricity": true, "fuel": false}` | `{"electricity": true, "fuel": false}` |
| 40 | `V02` | BEV exported script exits 0 | PASS | `0` | `0` |
| 41 | `V03` | BEV exported script stdout has no traceback | PASS | `clean` | `clean` |
| 42 | `V04` | BEV export prints SOC start/end values | PASS | `98.0 -> 95.91` | `two numbers` |
| 43 | `V05` | exported SOC start matches /api/simulate | PASS | `api=98.0, exported=98.0` | `match` |
| 44 | `V06` | exported SOC end matches /api/simulate | PASS | `api=95.908674, exported=95.91` | `match` |
| 45 | `V07` | exported electricity economy within 1% of /api/simulate | PASS | `api=146.680796, exported=146.68, delta=0.0005%` | `<= 1%` |
| 46 | `V08` | BEV exported source runs INLINE via python -c, exits 0 | PASS | `0` | `0` |

### Task 3 — Static serving + security  (`e2e_static.py`)

| # | Check | What was checked | Result | Observed | Expected |
|---|---|---|---|---|---|
| 1 | `W01` | GET / -> HTTP 200 | PASS | `200` | `200` |
| 2 | `W02` | GET / content-type is text/html | PASS | `text/html; charset=utf-8` | `text/html*` |
| 3 | `W03` | GET / serves the real index.html (not the 503 placeholder) | PASS | `9574 bytes` | `index.html` |
| 4 | `W04` | index.html loads /vendor/plotly.min.js | PASS | `True` | `True` |
| 5 | `W05` | GET /styles.css -> HTTP 200 | PASS | `200` | `200` |
| 6 | `W06` | GET /styles.css content-type is text/css | PASS | `text/css; charset=utf-8` | `text/css*` |
| 7 | `W07` | /styles.css body is non-trivial | PASS | `31437 bytes` | `> 5000` |
| 8 | `W08` | GET /app.js -> HTTP 200 | PASS | `200` | `200` |
| 9 | `W09` | GET /app.js content-type is javascript | PASS | `text/javascript; charset=utf-8` | `*javascript*` |
| 10 | `W10/charts.js` | GET /charts.js -> 200 javascript | PASS | `200 text/javascript; charset=utf-8` | `200 + javascript` |
| 11 | `W10/i18n.js` | GET /i18n.js -> 200 javascript | PASS | `200 text/javascript; charset=utf-8` | `200 + javascript` |
| 12 | `W11` | GET /vendor/plotly.min.js -> HTTP 200 | PASS | `200` | `200` |
| 13 | `W12` | vendored plotly.min.js is > 4 MB | PASS | `4653932 bytes (4.44 MB)` | `> 4 MB` |
| 14 | `W13` | vendored plotly looks like the Plotly bundle | PASS | `found` | `found` |
| 15 | `W14` | unknown static file -> HTTP 404 | PASS | `404` | `404` |
| 16 | `W15` | static responses carry Content-Length | PASS | `9574` | `present` |
| 17 | `W20` | control: gui/CONTRACT.md on disk contains the marker string | PASS | `True` | `True` |
| 18 | `W21` | control: /CONTRACT.md is NOT served at all | PASS | `404` — frontend root only | `404` |
| 19 | `W31` | traversal blocked: /%2e%2e/CONTRACT.md -> 4xx | PASS | `403` | `4xx` |
| 20 | `W41` | traversal leaked no contract text: /%2e%2e/CONTRACT.md | PASS | `no leak` | `no leak` |
| 21 | `W32` | traversal blocked: /..%2fCONTRACT.md -> 4xx | PASS | `403` | `4xx` |
| 22 | `W42` | traversal leaked no contract text: /..%2fCONTRACT.md | PASS | `no leak` | `no leak` |
| 23 | `W33` | traversal blocked: /vendor/../../CONTRACT.md -> 4xx | PASS | `403` | `4xx` |
| 24 | `W43` | traversal leaked no contract text: /vendor/../../CONTRACT.md | PASS | `no leak` | `no leak` |
| 25 | `W34` | traversal blocked: /..%2F..%2FCONTRACT.md -> 4xx | PASS | `403` | `4xx` |
| 26 | `W44` | traversal leaked no contract text: /..%2F..%2FCONTRACT.md | PASS | `no leak` | `no leak` |
| 27 | `W35` | traversal blocked: /%2e%2e%2f%2e%2e%2fCONTRACT.md -> 4xx | PASS | `403` | `4xx` |
| 28 | `W45` | traversal leaked no contract text: /%2e%2e%2f%2e%2e%2fCONTRACT.md | PASS | `no leak` | `no leak` |
| 29 | `W50` | unknown API path -> HTTP 404 | PASS | `404` | `404` |
| 30 | `W51` | unknown API path 404 body is JSON | PASS | `application/json; charset=utf-8` | `application/json*` |
| 31 | `W52` | unknown API 404 body carries an 'error' field | PASS | `{"error": "Not found", "detail": "no API endpoint at /api/does-not-exist"}` | `{'error': ...}` |
| 32 | `W53` | GET /api/simulate -> 405 (POST only) | PASS | `405` | `405` |
| 33 | `W54` | unknown preset in /api/schema -> HTTP 404 | PASS | `404` | `404` |
| 34 | `W55` | unknown-preset 404 body is JSON with 'error' | PASS | `{"error": "Unknown preset", "detail": "unknown preset: not-a-vehicle.yaml"}` | `{'error': ...}` |
| 35 | `W56` | malformed JSON body -> HTTP 400 | PASS | `400` | `400` |
| 36 | `W57` | malformed JSON 400 body is JSON | PASS | `{"error": "Invalid JSON", "detail": "Expecting property name enclosed in double quotes at line 1 column 2"}` | `JSON object` |

### Task 4 — Frontend static sanity  (`e2e_frontend.py`)

| # | Check | What was checked | Result | Observed | Expected |
|---|---|---|---|---|---|
| 1 | `F01` | node is available on PATH | PASS | `C:\Program Files\nodejs\node.EXE` | `a node binary` |
| 2 | `F02-app.js` | app.js exists | PASS | `64599 bytes` | `present` |
| 3 | `F03-app.js` | node --check app.js exits 0 | PASS | `0` | `0` |
| 4 | `F02-charts.js` | charts.js exists | PASS | `13885 bytes` | `present` |
| 5 | `F03-charts.js` | node --check charts.js exits 0 | PASS | `0` | `0` |
| 6 | `F02-i18n.js` | i18n.js exists | PASS | `20148 bytes` | `present` |
| 7 | `F03-i18n.js` | node --check i18n.js exits 0 | PASS | `0` | `0` |
| 8 | `F10` | no script/link/img URL uses http(s):// or // (no CDN) | PASS | `data:image/svg+xml,%3Csvg xmlns='http://www.w3.org/2000/svg' viewBox='0 0 32 32'%3E%3Crect width='32' height='32' rx='7' fill='%231a5fd0'/%3E%3Cpath d='M7 22l5-12 4 8 5-11 4 15' fill='none' stroke='%23ffffff' stroke-width='2.4' stroke-linecap='round' stroke-linejoin='round'/%3E%3C/svg%3E, /styles...` | `[]` |
| 9 | `F11` | no http(s):// remains outside data: URIs in index.html | PASS | `none` | `[]` |
| 10 | `F12` | index.html references /vendor/plotly.min.js | PASS | `True` | `True` |
| 11 | `F13` | every <script src> is a same-origin absolute path | PASS | `/vendor/plotly.min.js, /hant.js, /i18n.js, /charts.js, /app.js` | `all start with '/'` |
| 12 | `F14` | index.html has a rel="icon" link | PASS | `<link rel="icon" href="data:image/svg+xml,%3Csvg xmlns='http://www.w3.org/2000/svg' viewBo` | `present` |
| 13 | `F15` | favicon is an inlined data: URI (no /favicon.ico round-trip) | PASS | `<link rel="icon" href="data:image/svg+xml,%3Csvg xmlns='http...` | `data:image/...` |
| 14 | `F16` | index.html loads /styles.css | PASS | `/styles.css` | `['/styles.css']` |
| 15 | `F17-app.js` | index.html loads /app.js | PASS | `True` | `True` |
| 16 | `F17-charts.js` | index.html loads /charts.js | PASS | `True` | `True` |
| 17 | `F17-i18n.js` | index.html loads /i18n.js | PASS | `True` | `True` |
| 18 | `F18` | index.html declares an EN/ZH language switch | PASS | `{"en": true, "zh": true}` | `True` |
| 19 | `F20` | styles.css has a bare [hidden] rule | PASS | `[hidden] { display: none !important; }` | `[hidden] { ... }` |
| 20 | `F21` | [hidden] rule sets display: none | PASS | `display: none !important;` | `display: none` |
| 21 | `F22` | .view-pane hidden state cannot be defeated by the base rule | PASS | `{"base_display": "flex", ".view-pane[hidden] rule": true, "[hidden] !important": true}` | `hidden guard present` |
| 22 | `F23` | styles.css defines the KPI card surface | PASS | `{".kpi-row": true, ".kpi-card": true}` | `{"kpi-row": true, "kpi-card": true}` |
| 23 | `F30` | app.js posts to /api/simulate | PASS | `True` | `True` |
| 24 | `F31` | app.js posts to /api/export-python | PASS | `True` | `True` |
| 25 | `F32` | app.js fetches /api/schema | PASS | `True` | `True` |
| 26 | `F33` | app.js sends 'lang' in the JSON payloads | PASS | `I18N.lang, I18N.lang` | `['I18N.lang', ...]` |
| 27 | `F34` | the lang value comes from the i18n state (I18N.lang) | PASS | `I18N.lang` | `I18N.lang` |
| 28 | `F35` | simulate payload carries preset/cycle/overrides/render_matplotlib/lang | PASS | `{"simulate_payloads": 2, "complete_payloads": 2, "keys_in_first": ["preset", "cycle", "overrides", "render_matplotlib", "lang"]}` | `at least one complete payload` |
| 29 | `F36` | no external chart library is loaded at runtime | PASS | `no CDN reference` | `True` |
| 30 | `F37` | app.js renders the base64 matplotlib PNG | PASS | `True` | `True` |
| 31 | `F38` | app.js surfaces backend errors (error banner) | PASS | `{"error": true}` | `True` |
| 32 | `F39` | charts.js drives Plotly | PASS | `True` | `True` |
| 33 | `F40` | i18n.js defines an EN and a ZH dictionary | PASS | `{"zh_strings": true}` | `True` |
| 34 | `F41` | i18n.js defaults to English | PASS | `'en', 'en', 'zh', 'en', 'zh', 'en'` | `an 'en' default` |

### Task 5 — Full-sweep robustness  (`e2e_sweep.py`)

| # | Check | What was checked | Result | Observed | Expected |
|---|---|---|---|---|---|
| 1 | `SW01` | sweep covers all 9 presets | PASS | `9` | `9` |
| 2 | `SW02` | sweep covers both cycles | PASS | `udds.csv, hwfet.csv` | `udds.csv, hwfet.csv` |
| 3 | `SW03` | total combinations exercised | PASS | `18` | `18` |
| 4 | `SW-01` | 2012_Ford_Fusion.yaml / udds.csv -> 200 with kpis + equal-length series | PASS | `status=200, kpis=7, ts_len=1370, equal=True` | `status=200, kpis>0, equal` |
| 5 | `SW-02` | 2012_Ford_Fusion.yaml / hwfet.csv -> 200 with kpis + equal-length series | PASS | `status=200, kpis=7, ts_len=766, equal=True` | `status=200, kpis>0, equal` |
| 6 | `SW-03` | 2016_TOYOTA_Prius_Two.yaml / udds.csv -> 200 with kpis + equal-length series | PASS | `status=200, kpis=12, ts_len=1370, equal=True` | `status=200, kpis>0, equal` |
| 7 | `SW-04` | 2016_TOYOTA_Prius_Two.yaml / hwfet.csv -> 200 with kpis + equal-length series | PASS | `status=200, kpis=12, ts_len=766, equal=True` | `status=200, kpis>0, equal` |
| 8 | `SW-05` | 2016 Nissan Leaf 30 kWh thrml.yaml / udds.csv -> 200 with kpis + equal-length series | PASS | `status=200, kpis=10, ts_len=1370, equal=True` | `status=200, kpis>0, equal` |
| 9 | `SW-06` | 2016 Nissan Leaf 30 kWh thrml.yaml / hwfet.csv -> 200 with kpis + equal-length series | PASS | `status=200, kpis=10, ts_len=766, equal=True` | `status=200, kpis>0, equal` |
| 10 | `SW-07` | 2022 Tesla Model 3 RWD thrml.yaml / udds.csv -> 200 with kpis + equal-length series | PASS | `status=200, kpis=10, ts_len=1370, equal=True` | `status=200, kpis>0, equal` |
| 11 | `SW-08` | 2022 Tesla Model 3 RWD thrml.yaml / hwfet.csv -> 200 with kpis + equal-length series | PASS | `status=200, kpis=10, ts_len=766, equal=True` | `status=200, kpis>0, equal` |
| 12 | `SW-09` | 2022_Renault_Zoe_ZE50_R135.yaml / udds.csv -> 200 with kpis + equal-length series | PASS | `status=200, kpis=10, ts_len=1370, equal=True` | `status=200, kpis>0, equal` |
| 13 | `SW-10` | 2022_Renault_Zoe_ZE50_R135.yaml / hwfet.csv -> 200 with kpis + equal-length series | PASS | `status=200, kpis=10, ts_len=766, equal=True` | `status=200, kpis>0, equal` |
| 14 | `SW-11` | 2020 Chevrolet Bolt EV thrml.yaml / udds.csv -> 200 with kpis + equal-length series | PASS | `status=200, kpis=10, ts_len=1370, equal=True` | `status=200, kpis>0, equal` |
| 15 | `SW-12` | 2020 Chevrolet Bolt EV thrml.yaml / hwfet.csv -> 200 with kpis + equal-length series | PASS | `status=200, kpis=10, ts_len=766, equal=True` | `status=200, kpis>0, equal` |
| 16 | `SW-13` | 2021_Hyundai_Sonata_Hybrid_Blue_thrml.yaml / udds.csv -> 200 with kpis + equal-length series | PASS | `status=200, kpis=12, ts_len=1370, equal=True` | `status=200, kpis>0, equal` |
| 17 | `SW-14` | 2021_Hyundai_Sonata_Hybrid_Blue_thrml.yaml / hwfet.csv -> 200 with kpis + equal-length series | PASS | `status=200, kpis=12, ts_len=766, equal=True` | `status=200, kpis>0, equal` |
| 18 | `SW-15` | 2026_Chrysler_Pacifica_Select.yaml / udds.csv -> 200 with kpis + equal-length series | PASS | `status=200, kpis=7, ts_len=1370, equal=True` | `status=200, kpis>0, equal` |
| 19 | `SW-16` | 2026_Chrysler_Pacifica_Select.yaml / hwfet.csv -> 200 with kpis + equal-length series | PASS | `status=200, kpis=7, ts_len=766, equal=True` | `status=200, kpis>0, equal` |
| 20 | `SW-17` | 2026_Chrysler_Pacifica_Select_thrml.yaml / udds.csv -> 200 with kpis + equal-length series | PASS | `status=200, kpis=7, ts_len=1370, equal=True` | `status=200, kpis>0, equal` |
| 21 | `SW-18` | 2026_Chrysler_Pacifica_Select_thrml.yaml / hwfet.csv -> 200 with kpis + equal-length series | PASS | `status=200, kpis=7, ts_len=766, equal=True` | `status=200, kpis>0, equal` |
| 22 | `SW10` | all 18 combinations returned HTTP 200 | PASS | `18` | `18` |
| 23 | `SW11` | all 18 combinations returned a non-empty kpis object | PASS | `all non-empty` | `all non-empty` |
| 24 | `SW12` | all 18 combinations returned equal-length time series | PASS | `all equal` | `all equal` |
| 25 | `SW13` | no combination produced an error payload | PASS | `none` | `[]` |
| 26 | `SW20-1` | 2012_Ford_Fusion.yaml / udds.csv with render_matplotlib:true -> valid PNG | PASS | `status=200, png=198832 B` | `status=200, PNG magic` |
| 27 | `SW20-2` | 2022 Tesla Model 3 RWD thrml.yaml / udds.csv with render_matplotlib:true -> valid PNG | PASS | `status=200, png=255147 B` | `status=200, PNG magic` |
| 28 | `SW20-3` | 2016_TOYOTA_Prius_Two.yaml / hwfet.csv with render_matplotlib:true -> valid PNG | PASS | `status=200, png=242420 B` | `status=200, PNG magic` |

## 4. The 9 × 2 sweep (`render_matplotlib: false`)

| # | Preset | Cycle | HTTP | KPIs | KPI keys | Timeseries keys | Array length | Equal lengths | Latency |
|---|---|---|---|---|---|---|---|---|---|
| 1 | `2012_Ford_Fusion.yaml` | `udds.csv` | 200 | 7 | `["avg_speed_mph", "distance_mi", "energy_to_wheels_kwh", "fuel_consumed_kwh", "fuel_economy_mpgge", "max_speed_mph", "trace_met_pct"]` | `["cum_distance_mi", "cum_energy_kwh", "cum_fuel_kwh", "engine_power_kw", "fuel_power_kw", "grade_pct", "speed_achieved_mps", "speed_target_mps", "time_s"]` | 1370 | yes | 30.9 ms |
| 2 | `2012_Ford_Fusion.yaml` | `hwfet.csv` | 200 | 7 | `["avg_speed_mph", "distance_mi", "energy_to_wheels_kwh", "fuel_consumed_kwh", "fuel_economy_mpgge", "max_speed_mph", "trace_met_pct"]` | `["cum_distance_mi", "cum_energy_kwh", "cum_fuel_kwh", "engine_power_kw", "fuel_power_kw", "grade_pct", "speed_achieved_mps", "speed_target_mps", "time_s"]` | 766 | yes | 31.2 ms |
| 3 | `2016_TOYOTA_Prius_Two.yaml` | `udds.csv` | 200 | 12 | `["avg_speed_mph", "distance_mi", "electric_economy_wh_per_mi", "electricity_consumed_kwh", "energy_to_wheels_kwh", "fuel_consumed_kwh", "fuel_economy_mpgge", "max_speed_mph", "regen_recovered_kwh", "soc_end_pct", "soc_start_pct", "trace_met_pct"]` | `["battery_power_kw", "cum_distance_mi", "cum_energy_kwh", "cum_fuel_kwh", "engine_power_kw", "fuel_power_kw", "grade_pct", "motor_power_kw", "soc_pct", "speed_achieved_mps", "speed_target_mps", "time_s"]` | 1370 | yes | 31.3 ms |
| 4 | `2016_TOYOTA_Prius_Two.yaml` | `hwfet.csv` | 200 | 12 | `["avg_speed_mph", "distance_mi", "electric_economy_wh_per_mi", "electricity_consumed_kwh", "energy_to_wheels_kwh", "fuel_consumed_kwh", "fuel_economy_mpgge", "max_speed_mph", "regen_recovered_kwh", "soc_end_pct", "soc_start_pct", "trace_met_pct"]` | `["battery_power_kw", "cum_distance_mi", "cum_energy_kwh", "cum_fuel_kwh", "engine_power_kw", "fuel_power_kw", "grade_pct", "motor_power_kw", "soc_pct", "speed_achieved_mps", "speed_target_mps", "time_s"]` | 766 | yes | 21.7 ms |
| 5 | `2016 Nissan Leaf 30 kWh thrml.yaml` | `udds.csv` | 200 | 10 | `["avg_speed_mph", "distance_mi", "electric_economy_wh_per_mi", "electricity_consumed_kwh", "energy_to_wheels_kwh", "max_speed_mph", "regen_recovered_kwh", "soc_end_pct", "soc_start_pct", "trace_met_pct"]` | `["battery_power_kw", "cum_distance_mi", "cum_energy_kwh", "grade_pct", "motor_power_kw", "soc_pct", "speed_achieved_mps", "speed_target_mps", "time_s"]` | 1370 | yes | 28.8 ms |
| 6 | `2016 Nissan Leaf 30 kWh thrml.yaml` | `hwfet.csv` | 200 | 10 | `["avg_speed_mph", "distance_mi", "electric_economy_wh_per_mi", "electricity_consumed_kwh", "energy_to_wheels_kwh", "max_speed_mph", "regen_recovered_kwh", "soc_end_pct", "soc_start_pct", "trace_met_pct"]` | `["battery_power_kw", "cum_distance_mi", "cum_energy_kwh", "grade_pct", "motor_power_kw", "soc_pct", "speed_achieved_mps", "speed_target_mps", "time_s"]` | 766 | yes | 34.0 ms |
| 7 | `2022 Tesla Model 3 RWD thrml.yaml` | `udds.csv` | 200 | 10 | `["avg_speed_mph", "distance_mi", "electric_economy_wh_per_mi", "electricity_consumed_kwh", "energy_to_wheels_kwh", "max_speed_mph", "regen_recovered_kwh", "soc_end_pct", "soc_start_pct", "trace_met_pct"]` | `["battery_power_kw", "cum_distance_mi", "cum_energy_kwh", "grade_pct", "motor_power_kw", "soc_pct", "speed_achieved_mps", "speed_target_mps", "time_s"]` | 1370 | yes | 51.4 ms |
| 8 | `2022 Tesla Model 3 RWD thrml.yaml` | `hwfet.csv` | 200 | 10 | `["avg_speed_mph", "distance_mi", "electric_economy_wh_per_mi", "electricity_consumed_kwh", "energy_to_wheels_kwh", "max_speed_mph", "regen_recovered_kwh", "soc_end_pct", "soc_start_pct", "trace_met_pct"]` | `["battery_power_kw", "cum_distance_mi", "cum_energy_kwh", "grade_pct", "motor_power_kw", "soc_pct", "speed_achieved_mps", "speed_target_mps", "time_s"]` | 766 | yes | 43.2 ms |
| 9 | `2022_Renault_Zoe_ZE50_R135.yaml` | `udds.csv` | 200 | 10 | `["avg_speed_mph", "distance_mi", "electric_economy_wh_per_mi", "electricity_consumed_kwh", "energy_to_wheels_kwh", "max_speed_mph", "regen_recovered_kwh", "soc_end_pct", "soc_start_pct", "trace_met_pct"]` | `["battery_power_kw", "cum_distance_mi", "cum_energy_kwh", "grade_pct", "motor_power_kw", "soc_pct", "speed_achieved_mps", "speed_target_mps", "time_s"]` | 1370 | yes | 52.9 ms |
| 10 | `2022_Renault_Zoe_ZE50_R135.yaml` | `hwfet.csv` | 200 | 10 | `["avg_speed_mph", "distance_mi", "electric_economy_wh_per_mi", "electricity_consumed_kwh", "energy_to_wheels_kwh", "max_speed_mph", "regen_recovered_kwh", "soc_end_pct", "soc_start_pct", "trace_met_pct"]` | `["battery_power_kw", "cum_distance_mi", "cum_energy_kwh", "grade_pct", "motor_power_kw", "soc_pct", "speed_achieved_mps", "speed_target_mps", "time_s"]` | 766 | yes | 17.4 ms |
| 11 | `2020 Chevrolet Bolt EV thrml.yaml` | `udds.csv` | 200 | 10 | `["avg_speed_mph", "distance_mi", "electric_economy_wh_per_mi", "electricity_consumed_kwh", "energy_to_wheels_kwh", "max_speed_mph", "regen_recovered_kwh", "soc_end_pct", "soc_start_pct", "trace_met_pct"]` | `["battery_power_kw", "cum_distance_mi", "cum_energy_kwh", "grade_pct", "motor_power_kw", "soc_pct", "speed_achieved_mps", "speed_target_mps", "time_s"]` | 1370 | yes | 48.8 ms |
| 12 | `2020 Chevrolet Bolt EV thrml.yaml` | `hwfet.csv` | 200 | 10 | `["avg_speed_mph", "distance_mi", "electric_economy_wh_per_mi", "electricity_consumed_kwh", "energy_to_wheels_kwh", "max_speed_mph", "regen_recovered_kwh", "soc_end_pct", "soc_start_pct", "trace_met_pct"]` | `["battery_power_kw", "cum_distance_mi", "cum_energy_kwh", "grade_pct", "motor_power_kw", "soc_pct", "speed_achieved_mps", "speed_target_mps", "time_s"]` | 766 | yes | 41.6 ms |
| 13 | `2021_Hyundai_Sonata_Hybrid_Blue_thrml.yaml` | `udds.csv` | 200 | 12 | `["avg_speed_mph", "distance_mi", "electric_economy_wh_per_mi", "electricity_consumed_kwh", "energy_to_wheels_kwh", "fuel_consumed_kwh", "fuel_economy_mpgge", "max_speed_mph", "regen_recovered_kwh", "soc_end_pct", "soc_start_pct", "trace_met_pct"]` | `["battery_power_kw", "cum_distance_mi", "cum_energy_kwh", "cum_fuel_kwh", "engine_power_kw", "fuel_power_kw", "grade_pct", "motor_power_kw", "soc_pct", "speed_achieved_mps", "speed_target_mps", "time_s"]` | 1370 | yes | 57.4 ms |
| 14 | `2021_Hyundai_Sonata_Hybrid_Blue_thrml.yaml` | `hwfet.csv` | 200 | 12 | `["avg_speed_mph", "distance_mi", "electric_economy_wh_per_mi", "electricity_consumed_kwh", "energy_to_wheels_kwh", "fuel_consumed_kwh", "fuel_economy_mpgge", "max_speed_mph", "regen_recovered_kwh", "soc_end_pct", "soc_start_pct", "trace_met_pct"]` | `["battery_power_kw", "cum_distance_mi", "cum_energy_kwh", "cum_fuel_kwh", "engine_power_kw", "fuel_power_kw", "grade_pct", "motor_power_kw", "soc_pct", "speed_achieved_mps", "speed_target_mps", "time_s"]` | 766 | yes | 40.4 ms |
| 15 | `2026_Chrysler_Pacifica_Select.yaml` | `udds.csv` | 200 | 7 | `["avg_speed_mph", "distance_mi", "energy_to_wheels_kwh", "fuel_consumed_kwh", "fuel_economy_mpgge", "max_speed_mph", "trace_met_pct"]` | `["cum_distance_mi", "cum_energy_kwh", "cum_fuel_kwh", "engine_power_kw", "fuel_power_kw", "grade_pct", "speed_achieved_mps", "speed_target_mps", "time_s"]` | 1370 | yes | 43.5 ms |
| 16 | `2026_Chrysler_Pacifica_Select.yaml` | `hwfet.csv` | 200 | 7 | `["avg_speed_mph", "distance_mi", "energy_to_wheels_kwh", "fuel_consumed_kwh", "fuel_economy_mpgge", "max_speed_mph", "trace_met_pct"]` | `["cum_distance_mi", "cum_energy_kwh", "cum_fuel_kwh", "engine_power_kw", "fuel_power_kw", "grade_pct", "speed_achieved_mps", "speed_target_mps", "time_s"]` | 766 | yes | 29.1 ms |
| 17 | `2026_Chrysler_Pacifica_Select_thrml.yaml` | `udds.csv` | 200 | 7 | `["avg_speed_mph", "distance_mi", "energy_to_wheels_kwh", "fuel_consumed_kwh", "fuel_economy_mpgge", "max_speed_mph", "trace_met_pct"]` | `["cum_distance_mi", "cum_energy_kwh", "cum_fuel_kwh", "engine_power_kw", "fuel_power_kw", "grade_pct", "speed_achieved_mps", "speed_target_mps", "time_s"]` | 1370 | yes | 36.5 ms |
| 18 | `2026_Chrysler_Pacifica_Select_thrml.yaml` | `hwfet.csv` | 200 | 7 | `["avg_speed_mph", "distance_mi", "energy_to_wheels_kwh", "fuel_consumed_kwh", "fuel_economy_mpgge", "max_speed_mph", "trace_met_pct"]` | `["cum_distance_mi", "cum_energy_kwh", "cum_fuel_kwh", "engine_power_kw", "fuel_power_kw", "grade_pct", "speed_achieved_mps", "speed_target_mps", "time_s"]` | 766 | yes | 25.9 ms |

Total sweep wall time inside the requests: **666.0 ms** over 18 combinations.

### Matplotlib subset (`render_matplotlib: true`)

| Preset | Cycle | HTTP | PNG bytes | Valid PNG magic | Latency |
|---|---|---|---|---|---|
| `2012_Ford_Fusion.yaml` | `udds.csv` | 200 | 198832 | yes | 642.3 ms |
| `2022 Tesla Model 3 RWD thrml.yaml` | `udds.csv` | 200 | 255147 | yes | 758.7 ms |
| `2016_TOYOTA_Prius_Two.yaml` | `hwfet.csv` | 200 | 242420 | yes | 726.6 ms |

## 5. Exported Python — reproduction numbers

### Conv (`2012_Ford_Fusion.yaml` + `udds.csv`) with an override

| Quantity | `/api/simulate` | Exported script (file) | Exported script (`python -c`) | Delta |
|---|---|---|---|---|
| Fuel economy (mpgge) | 35.372215 | 35.37 | 35.37 | 0.00626 % |
| Distance (mi) | 7.45051 | 7.451 | 7.451 | 0.00049 mi |

- Override applied: `{"chassis.drag_coef": 0.3}`  
- Exported source length: **22 lines**  
- Both execution modes (file and `python -c`) exited **0**.

Exported source, verbatim:

```python
import fastsim

veh = fastsim.Vehicle.from_resource("2012_Ford_Fusion.yaml")
cyc = fastsim.Cycle.from_resource("udds.csv")
d = veh.to_dict()
for path, val in {"chassis.drag_coef": 0.3}.items():
    node, keys = d, path.split(".")
    for k in keys[:-1]:
        node = node[k]
    node[keys[-1]] = val
veh = fastsim.Vehicle.from_dict(d)
veh.set_save_interval(1)
sim = fastsim.SimDrive(veh, cyc)
sim.run()
r = sim.to_dict(flatten=True)
P = "veh.pt_type.Conv"
mi = r["veh.state.dist_meters"] / 1609.344
rows = [("Preset", "2012 Ford Fusion"), ("Cycle", "udds.csv"), ("Distance", f"{mi:.3f} mi")]
kwh = r[f"{P}.fc.state.energy_fuel_joules"] / 3.6e6
rows += [("Fuel energy", f"{kwh:.4f} kWh"), ("Fuel economy", f"{mi / (kwh / 33.7):.2f} mpgge")]
for key, val in rows:
    print(f"{key:<22}{val}")
```

Its stdout when run as a file:

```text
Preset                2012 Ford Fusion
Cycle                 udds.csv
Distance              7.451 mi
Fuel energy           7.0983 kWh
Fuel economy          35.37 mpgge
```

### BEV (`2022 Tesla Model 3 RWD thrml.yaml` + `udds.csv`)

| Quantity | `/api/simulate` | Exported script | Delta |
|---|---|---|---|
| SOC start (%) | 98.0 | 98.0 | 0.0 |
| SOC end (%) | 95.908674 | 95.91 | 0.00133 |
| Electricity economy (Wh/mi) | 146.680796 | 146.68 | 0.00054 % |

- Exported source length: **18 lines**; exit code 0 as a file and inline via `python -c`.

```text
Preset                2022 Tesla Model 3 RWD
Cycle                 udds.csv
Distance              7.451 mi
Electricity           1.0928 kWh
Electricity economy   146.68 Wh/mi
SOC start             98.00 %
SOC end               95.91 %
```

### `render_matplotlib` cost

| Mode | Latency |
|---|---|
| `render_matplotlib: true` | 589.6 ms |
| `render_matplotlib: false` (best of 2) | 31.9 ms |
| Speed-up | 18.49× |

### Override effect (`chassis.drag_coef`)

| Configuration | `fuel_economy_mpgge` |
|---|---|
| baseline | 34.379216 |
| `chassis.drag_coef = 0.3` | 35.372215 |

### Language invariance

- `kpis` and `timeseries` for `lang: "zh"` were **exactly equal** to `lang: "en"`; only the figure differed (`198832` B vs `196780` B).

## 6. Scope, honesty notes

- No browser automation was used (per the brief): the frontend was verified statically — JS parses under `node --check`, no CDN/network references, same-origin asset paths, `[hidden]` display guard, `lang` sent in the simulate payload — **not** by driving the DOM.
- Path traversal was probed with raw sockets so the request line reached the server byte-for-byte (`%2e%2e`, `..%2f`, `vendor/../../`); a positive control confirmed the marker string really does exist inside `gui/CONTRACT.md`, so 'no leak' is a meaningful assertion.
- Nothing under `gui/backend/` or `gui/frontend/` was created, edited or deleted by this suite; both were read only.
- Temp artifacts were written to `gui/verify/_tmp/` and removed at the end of the run.
- This run attached to the already running backend at http://127.0.0.1:8000 and did not stop it (it was not started by the verification suite).

**Verdict: GREEN** — 223 passed, 0 failed, 0 warned.
