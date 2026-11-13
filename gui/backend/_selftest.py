"""
_selftest.py -- backend self-test for the FASTSim Web GUI (temporary file).

Starts app.py on a free port as a subprocess and exercises every endpoint
defined in gui/CONTRACT.md, including running the exported Python in a child
process.  Usage:

    python gui/backend/_selftest.py
"""

from __future__ import annotations

import base64
import json
import os
import shutil
import socket
import subprocess
import sys
import time
import urllib.error
import urllib.request
from pathlib import Path

BACKEND_DIR = Path(__file__).resolve().parent
PYTHON = BACKEND_DIR.parent.parent / "venv" / "Scripts" / "python.exe"
TMP_DIR = BACKEND_DIR / "_selftest_tmp"

RESULTS: list[tuple[bool, str]] = []


def check(name: str, condition: bool, evidence: str = "") -> None:
    RESULTS.append((bool(condition), f"{name}{('  -- ' + evidence) if evidence else ''}"))
    mark = "PASS" if condition else "FAIL"
    print(f"[{mark}] {name}" + (f"  ({evidence})" if evidence else ""), flush=True)


def free_port() -> int:
    with socket.socket(socket.AF_INET, socket.SOCK_STREAM) as sock:
        sock.bind(("127.0.0.1", 0))
        return int(sock.getsockname()[1])


def request(base: str, method: str, path: str, body: dict | None = None,
            timeout: float = 120.0):
    data = None
    headers = {}
    if body is not None:
        data = json.dumps(body).encode("utf-8")
        headers["Content-Type"] = "application/json"
    req = urllib.request.Request(base + path, data=data, headers=headers, method=method)
    try:
        with urllib.request.urlopen(req, timeout=timeout) as response:
            raw = response.read()
            return response.status, dict(response.headers), raw
    except urllib.error.HTTPError as exc:
        return exc.code, dict(exc.headers), exc.read()


def get_json(base: str, path: str):
    status, headers, raw = request(base, "GET", path)
    return status, headers, json.loads(raw.decode("utf-8"))


def post_json(base: str, path: str, body: dict, timeout: float = 180.0):
    status, headers, raw = request(base, "POST", path, body, timeout=timeout)
    return status, headers, json.loads(raw.decode("utf-8"))


def wait_ready(base: str, process: subprocess.Popen, timeout: float = 60.0) -> bool:
    deadline = time.time() + timeout
    while time.time() < deadline:
        if process.poll() is not None:
            return False
        try:
            status, _headers, _raw = request(base, "GET", "/api/schema", timeout=5.0)
            if status == 200:
                return True
        except Exception:
            time.sleep(0.25)
    return False


def main() -> int:
    if not PYTHON.is_file():
        print(f"venv python not found: {PYTHON}")
        return 2
    TMP_DIR.mkdir(exist_ok=True)

    port = free_port()
    base = f"http://127.0.0.1:{port}"
    log_path = TMP_DIR / "server.log"
    log_file = log_path.open("w", encoding="utf-8", errors="replace")
    process = subprocess.Popen(
        [str(PYTHON), str(BACKEND_DIR / "app.py"), "--port", str(port)],
        stdout=log_file,
        stderr=subprocess.STDOUT,
        cwd=str(BACKEND_DIR.parent.parent),  # launched from the workspace root on purpose
    )
    try:
        ready = wait_ready(base, process)
        check("server starts and answers GET /api/schema", ready, f"port {port}")
        if not ready:
            log_file.flush()
            print(log_path.read_text(encoding="utf-8", errors="replace")[-4000:])
            return 1

        # ---------------- 0. one mile constant everywhere ----------------
        sys.path.insert(0, str(BACKEND_DIR))
        import export_code as export_mod
        import plots as plots_mod
        import simulate as simulate_mod
        check("mile constant 1609.344 is single-sourced",
              simulate_mod.METERS_PER_MILE == 1609.344
              and plots_mod.METERS_PER_MILE == 1609.344
              and abs(simulate_mod.MPS_TO_MPH - plots_mod.MPS_TO_MPH) < 1e-12
              and f"{simulate_mod.METERS_PER_MILE!r}" == "1609.344",
              f"sim {simulate_mod.METERS_PER_MILE} plots {plots_mod.METERS_PER_MILE} "
              f"mph {simulate_mod.MPS_TO_MPH:.10f}")
        generated = export_mod.generate("2012_Ford_Fusion.yaml", "udds.csv", {})
        check("exported source divides by 1609.344",
              "/ 1609.344" in generated and "1609.34\\n" not in generated,
              [line.strip() for line in generated.splitlines()
               if "dist_meters" in line][0])

        # ---------------- 1. schema ----------------
        status, headers, schema = get_json(base, "/api/schema")
        params = schema.get("parameters", [])
        check("GET /api/schema -> 200", status == 200, f"status {status}")
        check("schema cache-control no-store",
              headers.get("Cache-Control") == "no-store", str(headers.get("Cache-Control")))
        check("parameter count >= 40", len(params) >= 40, f"{len(params)} parameters")
        check("schema echoes resolved preset",
              schema.get("preset") == "2012_Ford_Fusion.yaml", str(schema.get("preset")))
        check("9 presets listed", len(schema.get("presets", [])) == 9,
              str(len(schema.get("presets", []))))
        check("2 cycles with documented steps",
              [c.get("duration_s") for c in schema.get("cycles", [])] == [1369, 766],
              str([c.get("duration_s") for c in schema.get("cycles", [])]))

        by_path = {p["path"]: p for p in params}
        drag = by_path.get("chassis.drag_coef", {})
        check("chassis.drag_coef present",
              bool(drag), f"default={drag.get('default')}")
        check("chassis.drag_coef unit '-'", drag.get("unit") == "-", str(drag.get("unit")))
        check("chassis.drag_coef group Chassis/Aero",
              (drag.get("group"), drag.get("subgroup")) == ("Chassis", "Aero"),
              f"{drag.get('group')}/{drag.get('subgroup')}")
        check("chassis.drag_coef editable float",
              drag.get("type") == "float" and drag.get("editable") is True,
              f"{drag.get('type')} editable={drag.get('editable')}")
        check("chassis.drag_coef has EN + ZH description",
              bool(drag.get("description")) and bool(drag.get("description_zh")),
              f"{drag.get('description')!r} / {drag.get('description_zh')!r}")

        area = by_path.get("chassis.frontal_area_square_meters", {})
        check("unit suffix m² parsed", area.get("unit") == "m\u00b2", str(area.get("unit")))
        radius = by_path.get("chassis.wheel_radius_meters", {})
        check("unit suffix m parsed", radius.get("unit") == "m", str(radius.get("unit")))
        mass = by_path.get("mass_kilograms", {})
        check("mass_kilograms group Vehicle/Body",
              (mass.get("group"), mass.get("subgroup")) == ("Vehicle", "Body"),
              f"{mass.get('group')}/{mass.get('subgroup')}")
        fc_pwr = by_path.get("pt_type.Conv.fc.pwr_out_max_watts", {})
        check("powertrain path includes concrete type",
              bool(fc_pwr) and fc_pwr.get("unit") == "W",
              f"{fc_pwr.get('path')} unit={fc_pwr.get('unit')}")
        check("fc group Powertrain/Fuel Converter",
              (fc_pwr.get("group"), fc_pwr.get("subgroup")) == ("Powertrain", "Fuel Converter"),
              f"{fc_pwr.get('group')}/{fc_pwr.get('subgroup')}")
        drive = by_path.get("chassis.drive_type", {})
        check("drive_type enum FWD/RWD/AWD",
              drive.get("type") == "enum" and set(["FWD", "RWD", "AWD"]).issubset(
                  set(drive.get("enum_options") or [])),
              str(drive.get("enum_options")))
        dfco = by_path.get("pt_type.Conv.dfco_cntrl.dfco_enabled", {})
        check("dfco_enabled is bool", dfco.get("type") == "bool", str(dfco.get("type")))
        curve = by_path.get("pt_type.Conv.fc.eff_interp_from_pwr_out", {})
        check("efficiency interp exposed as read-only curve",
              curve.get("type") == "curve" and curve.get("editable") is False,
              f"{curve.get('type')} editable={curve.get('editable')}")
        check("curve internals not exposed as editable floats",
              not any(k.startswith("pt_type.Conv.fc.eff_interp_from_pwr_out.data")
                      for k in by_path), "")
        check("state/history/save_interval excluded",
              not any((".state." in k or ".history." in k or k.endswith("save_interval"))
                      for k in by_path), "")
        check("cabin/hvac absent-string not a parameter",
              by_path.get("cabin") is None and by_path.get("hvac") is None, "")

        status, _h, bev_schema = get_json(base, "/api/schema?preset=2022%20Tesla%20Model%203%20RWD%20thrml.yaml")
        bev_paths = {p["path"] for p in bev_schema.get("parameters", [])}
        check("preset-specific schema (BEV has em/res, no fc/fs)",
              status == 200
              and any(p.startswith("pt_type.BEV.em.") for p in bev_paths)
              and any(p.startswith("pt_type.BEV.res.") for p in bev_paths)
              and not any(".fc." in p for p in bev_paths),
              f"{len(bev_paths)} BEV params")
        check("BEV schema echoes BEV preset",
              bev_schema.get("preset") == "2022 Tesla Model 3 RWD thrml.yaml",
              str(bev_schema.get("preset")))
        thermal_paths = {p["path"] for p in params}
        check("Fusion has no battery params", not any(".res." in p for p in thermal_paths), "")

        status, _h, err = get_json(base, "/api/schema?preset=nope.yaml")
        check("unknown preset -> 4xx JSON error",
              status >= 400 and isinstance(err.get("error"), str), f"{status} {err.get('error')}")

        # ---------------- 2. static serving ----------------
        status, headers, raw = request(base, "GET", "/")
        check("GET / serves HTML (index.html or 503 placeholder)",
              status in (200, 503) and "text/html" in headers.get("Content-Type", ""),
              f"status {status} {headers.get('Content-Type')}")
        index_file = BACKEND_DIR.parent / "frontend" / "index.html"
        if index_file.is_file():
            check("GET / returns the real index.html when present",
                  status == 200 and b"<html" in raw.lower(), f"{len(raw)} bytes")
            for asset, expected in (("/styles.css", "text/css"),
                                    ("/app.js", "text/javascript"),
                                    ("/vendor/plotly.min.js", "text/javascript")):
                astatus, aheaders, araw = request(base, "GET", asset)
                check(f"static {asset} -> 200 {expected}",
                      astatus == 200 and expected in aheaders.get("Content-Type", "")
                      and len(araw) > 100,
                      f"{astatus} {aheaders.get('Content-Type')} {len(araw)}B")
        status, _h, _raw = request(base, "GET", "/../CONTRACT.md")
        check("path traversal rejected", status in (403, 404), f"status {status}")
        status, _h, _raw = request(base, "GET", "/%2e%2e/CONTRACT.md")
        check("encoded path traversal rejected", status in (403, 404), f"status {status}")
        status, headers, _raw = request(base, "OPTIONS", "/api/simulate")
        check("OPTIONS -> 204 with permissive headers",
              status == 204 and "Access-Control-Allow-Origin" in headers, f"status {status}")
        status, _h, err = get_json(base, "/api/nope")
        check("unknown API path -> 404 JSON", status == 404 and "error" in err, f"status {status}")

        # ---------------- 2b. missing frontend must not crash ----------------
        empty_dir = TMP_DIR / "empty_frontend"
        empty_dir.mkdir(exist_ok=True)
        empty_port = free_port()
        empty_base = f"http://127.0.0.1:{empty_port}"
        empty_log = (TMP_DIR / "server_nofrontend.log").open("w", encoding="utf-8")
        empty_proc = subprocess.Popen(
            [str(PYTHON), str(BACKEND_DIR / "app.py"), "--port", str(empty_port)],
            stdout=empty_log, stderr=subprocess.STDOUT, cwd=str(BACKEND_DIR),
            env={**os.environ, "FASTSIM_GUI_FRONTEND": str(empty_dir)},
        )
        try:
            ok = wait_ready(empty_base, empty_proc)
            check("server starts with no frontend/index.html", ok, f"port {empty_port}")
            if ok:
                status, headers, raw = request(empty_base, "GET", "/")
                check("missing index.html -> 503 HTML placeholder",
                      status == 503 and "text/html" in headers.get("Content-Type", "")
                      and b"index.html" in raw, f"status {status}")
                status, _h, schema2 = get_json(empty_base, "/api/schema")
                check("API still works with no frontend", status == 200
                      and len(schema2.get("parameters", [])) >= 40, f"status {status}")
        finally:
            empty_proc.terminate()
            try:
                empty_proc.wait(timeout=10)
            except subprocess.TimeoutExpired:
                empty_proc.kill()
            empty_log.close()

        # ---------------- 3. Conv simulation ----------------
        t0 = time.time()
        status, _h, conv = post_json(base, "/api/simulate", {
            "preset": "2012_Ford_Fusion.yaml",
            "cycle": "udds.csv",
            "render_matplotlib": True,
        })
        elapsed = time.time() - t0
        check("POST /api/simulate (Conv) -> 200", status == 200, f"status {status}, {elapsed:.2f}s")
        kpis = conv.get("kpis", {})
        mpgge = kpis.get("fuel_economy_mpgge", {}).get("value")
        check("Conv fuel economy ~34.38 mpgge (within 2%)",
              mpgge is not None and abs(mpgge - 34.38) <= 0.02 * 34.38, f"{mpgge}")
        check("Conv distance 7.451 mi at printed precision (3 dp)",
              f"{kpis.get('distance_mi', {}).get('value', 0):.3f}" == "7.451",
              f"{kpis.get('distance_mi', {}).get('value')} -> "
              f"{kpis.get('distance_mi', {}).get('value', 0):.3f} mi")
        check("Conv fuel energy ~7.303 kWh",
              abs(kpis.get("fuel_consumed_kwh", {}).get("value", 0) - 7.303) < 0.05,
              str(kpis.get("fuel_consumed_kwh", {}).get("value")))
        check("KPI entries carry value/unit/label_en/label_zh",
              all(set(("value", "unit", "label_en", "label_zh")) <= set(v) for v in kpis.values()),
              f"{len(kpis)} kpis")
        check("Conv has no SOC KPIs (omitted, not zeroed)",
              "soc_end_pct" not in kpis and "electric_economy_wh_per_mi" not in kpis,
              ",".join(sorted(kpis)))
        check("Conv trace_met_pct present and high",
              kpis.get("trace_met_pct", {}).get("value", 0) > 95,
              str(kpis.get("trace_met_pct", {}).get("value")))

        ts = conv.get("timeseries", {})
        lengths = {len(v) for v in ts.values() if isinstance(v, list)}
        check("Conv timeseries arrays non-empty and equal length",
              lengths and len(lengths) == 1 and min(lengths) > 1000, f"lengths {sorted(lengths)}")
        for key in ("time_s", "speed_target_mps", "speed_achieved_mps", "grade_pct",
                    "engine_power_kw", "fuel_power_kw", "cum_fuel_kwh", "cum_energy_kwh",
                    "cum_distance_mi"):
            check(f"timeseries has {key}", key in ts, "")
        check("Conv has no battery timeseries (omitted)",
              "battery_power_kw" not in ts and "soc_pct" not in ts and "motor_power_kw" not in ts,
              ",".join(sorted(ts)))
        png_b64 = conv.get("matplotlib_png_base64")
        png_bytes = base64.b64decode(png_b64) if png_b64 else b""
        check("matplotlib_png_base64 decodes to a PNG",
              png_bytes.startswith(b"\x89PNG"), f"{len(png_bytes)} bytes")
        check("matplotlib figure is 12x9 in @140dpi",
              len(png_bytes) > 5 and int.from_bytes(png_bytes[16:20], "big") == 1680
              and int.from_bytes(png_bytes[20:24], "big") == 1260,
              f"{int.from_bytes(png_bytes[16:20], 'big')}x{int.from_bytes(png_bytes[20:24], 'big')}")
        svg = conv.get("matplotlib_svg") or ""
        check("matplotlib_svg is raw SVG text (frontend SVG download)",
              svg.startswith("<?xml") and "<svg" in svg, f"{len(svg)} chars")

        # render_matplotlib false must skip all figure work
        t0 = time.time()
        status, _h, fast = post_json(base, "/api/simulate", {
            "preset": "2012_Ford_Fusion.yaml", "cycle": "udds.csv",
            "render_matplotlib": False,
        })
        fast_elapsed = time.time() - t0
        check("render_matplotlib=false omits the figure",
              status == 200 and "matplotlib_png_base64" not in fast,
              f"{fast_elapsed * 1000:.0f} ms")
        check("render_matplotlib=false is much faster than true",
              fast_elapsed < elapsed, f"{fast_elapsed:.2f}s vs {elapsed:.2f}s")

        # ---------------- 3b. localised figure (lang) ----------------
        print(f"---- CJK font for zh figures: {plots_mod.CJK_FAMILY} "
              f"(available={plots_mod.CJK_AVAILABLE}) ----")
        status, _h, conv_zh = post_json(base, "/api/simulate", {
            "preset": "2012_Ford_Fusion.yaml", "cycle": "udds.csv",
            "render_matplotlib": True, "lang": "zh",
        })
        check("lang=zh -> 200", status == 200, f"status {status}")
        check("lang=zh echoed back as 'zh'", conv_zh.get("lang") == "zh",
              str(conv_zh.get("lang")))
        check("lang defaults to 'en' when absent", conv.get("lang") == "en",
              str(conv.get("lang")))
        zh_png = base64.b64decode(conv_zh.get("matplotlib_png_base64") or b"")
        check("lang=zh returns a valid PNG", zh_png.startswith(b"\x89PNG"),
              f"{len(zh_png)} bytes")
        check("lang=zh figure is still 1680x1260",
              len(zh_png) > 24 and int.from_bytes(zh_png[16:20], "big") == 1680
              and int.from_bytes(zh_png[20:24], "big") == 1260,
              f"{int.from_bytes(zh_png[16:20], 'big')}x{int.from_bytes(zh_png[20:24], 'big')}")
        check("lang=zh kpis are byte-identical to lang=en",
              conv_zh.get("kpis") == conv.get("kpis"),
              "identical" if conv_zh.get("kpis") == conv.get("kpis")
              else f"en={conv.get('kpis')} zh={conv_zh.get('kpis')}")
        check("lang=zh timeseries are identical to lang=en",
              conv_zh.get("timeseries") == conv.get("timeseries"),
              f"{len(conv_zh.get('timeseries') or {})} keys")
        check("lang=zh SVG still served",
              (conv_zh.get("matplotlib_svg") or "").startswith("<?xml"),
              f"{len(conv_zh.get('matplotlib_svg') or '')} chars")
        en_png = base64.b64decode(conv.get("matplotlib_png_base64") or b"")
        check("lang=zh figure differs from lang=en (Chinese glyphs rendered)",
              (en_png != zh_png) == plots_mod.CJK_AVAILABLE,
              f"en {len(en_png)}B vs zh {len(zh_png)}B, cjk={plots_mod.CJK_AVAILABLE}")
        status, _h, other_lang = post_json(base, "/api/simulate", {
            "preset": "2012_Ford_Fusion.yaml", "cycle": "udds.csv",
            "render_matplotlib": False, "lang": "zh-CN",
        })
        check("lang accepts zh-CN and normalises to 'zh'",
              status == 200 and other_lang.get("lang") == "zh",
              str(other_lang.get("lang")))
        status, _h, unknown_lang = post_json(base, "/api/simulate", {
            "preset": "2012_Ford_Fusion.yaml", "cycle": "udds.csv",
            "render_matplotlib": False, "lang": "fr",
        })
        check("unknown lang falls back to 'en'",
              status == 200 and unknown_lang.get("lang") == "en",
              str(unknown_lang.get("lang")))
        status, _h, bad_lang = post_json(base, "/api/simulate", {
            "preset": "2012_Ford_Fusion.yaml", "cycle": "udds.csv", "lang": 123,
        })
        check("non-string lang -> 400 JSON error",
              status == 400 and "error" in bad_lang, f"{status} {bad_lang.get('error')}")

        # ---------------- 4. BEV simulation ----------------
        status, _h, bev = post_json(base, "/api/simulate", {
            "preset": "2022 Tesla Model 3 RWD thrml.yaml",
            "cycle": "udds.csv",
            "render_matplotlib": True,
        })
        bev_kpis = bev.get("kpis", {})
        check("POST /api/simulate (BEV) -> 200", status == 200, f"status {status}")
        check("BEV has soc_end_pct", "soc_end_pct" in bev_kpis,
              str(bev_kpis.get("soc_end_pct")))
        check("BEV has NO fuel_economy_mpgge",
              "fuel_economy_mpgge" not in bev_kpis and "fuel_consumed_kwh" not in bev_kpis,
              ",".join(sorted(bev_kpis)))
        soc_start = bev_kpis.get("soc_start_pct", {}).get("value")
        soc_end = bev_kpis.get("soc_end_pct", {}).get("value")
        check("BEV SOC 98.00 -> 95.91 %",
              soc_start is not None and soc_end is not None
              and abs(soc_start - 98.0) < 0.01 and abs(soc_end - 95.91) < 0.05,
              f"{soc_start} -> {soc_end}")
        wh_per_mi = bev_kpis.get("electric_economy_wh_per_mi", {}).get("value")
        check("BEV electricity economy ~144.5 Wh/mi (+/-2%)",
              wh_per_mi is not None and abs(wh_per_mi - 144.5) <= 0.02 * 144.5,
              str(wh_per_mi))
        regen = bev_kpis.get("regen_recovered_kwh", {}).get("value")
        check("BEV regen recovered ~0.647 kWh",
              regen is not None and abs(regen - 0.647) < 0.02, str(regen))
        bev_ts = bev.get("timeseries", {})
        for key in ("battery_power_kw", "soc_pct", "motor_power_kw", "cum_energy_kwh"):
            check(f"BEV timeseries has {key}", key in bev_ts, "")
        check("BEV has no fuel timeseries", "fuel_power_kw" not in bev_ts
              and "engine_power_kw" not in bev_ts, ",".join(sorted(bev_ts)))
        bev_lengths = {len(v) for v in bev_ts.values() if isinstance(v, list)}
        check("BEV timeseries equal length",
              len(bev_lengths) == 1, str(sorted(bev_lengths)))
        bev_png = base64.b64decode(bev.get("matplotlib_png_base64") or b"")
        check("BEV matplotlib PNG renders", bev_png.startswith(b"\x89PNG"), f"{len(bev_png)} bytes")

        # ---------------- 5. overrides ----------------
        status, _h, modified = post_json(base, "/api/simulate", {
            "preset": "2012_Ford_Fusion.yaml",
            "cycle": "udds.csv",
            "overrides": {"chassis.drag_coef": 0.30},
            "render_matplotlib": False,
        })
        mod_mpgge = modified.get("kpis", {}).get("fuel_economy_mpgge", {}).get("value")
        check("override drag_coef -> 200", status == 200, f"status {status}")
        check("override changes the KPI",
              mod_mpgge is not None and abs(mod_mpgge - mpgge) > 0.5,
              f"{mpgge} -> {mod_mpgge}")
        status, _h, mass_mod = post_json(base, "/api/simulate", {
            "preset": "2012_Ford_Fusion.yaml",
            "cycle": "udds.csv",
            "overrides": {"mass_kilograms": 1750},
            "render_matplotlib": False,
        })
        mass_mpgge = mass_mod.get("kpis", {}).get("fuel_economy_mpgge", {}).get("value")
        check("override mass_kilograms changes the KPI",
              abs(mass_mpgge - mpgge) > 0.3, f"{mpgge} -> {mass_mpgge}")
        status, _h, bad = post_json(base, "/api/simulate", {
            "preset": "2012_Ford_Fusion.yaml",
            "cycle": "udds.csv",
            "overrides": {"chassis.drag_coeff_typo": 0.3},
        })
        check("unknown override path -> 400 Unknown parameter path",
              status == 400 and bad.get("error") == "Unknown parameter path"
              and bad.get("detail") == "chassis.drag_coeff_typo",
              f"{status} {bad.get('error')} / {bad.get('detail')}")
        status, _h, bad2 = post_json(base, "/api/simulate", {
            "preset": "2012_Ford_Fusion.yaml",
            "cycle": "udds.csv",
            "overrides": {"pt_type.Conv.fc.eff_interp_from_pwr_out": 0.5},
        })
        check("curve override rejected with JSON error",
              status == 400 and "error" in bad2, f"{status} {bad2.get('error')}")
        status, _h, bad3 = post_json(base, "/api/simulate", {
            "preset": "2012_Ford_Fusion.yaml",
            "cycle": "udds.csv",
            "overrides": {"mass_kilograms": "heavy"},
        })
        check("non-numeric value for a float -> 400",
              status == 400 and "error" in bad3, f"{status} {bad3.get('error')}")

        # ---------------- 6. export-python ----------------
        status, _h, exported = post_json(base, "/api/export-python", {
            "preset": "2012_Ford_Fusion.yaml",
            "cycle": "udds.csv",
            "overrides": {"chassis.drag_coef": 0.30},
        })
        code = exported.get("code", "")
        check("POST /api/export-python -> 200", status == 200, f"status {status}")
        check("export has no web/UI imports",
              "http" not in code and "matplotlib" not in code and "flask" not in code.lower(), "")
        check("export imports only fastsim",
              [line for line in code.splitlines() if line.startswith("import")] == ["import fastsim"],
              str([line for line in code.splitlines() if line.startswith("import")]))
        check("export has no class/def/main",
              "class " not in code and "def " not in code and "__main__" not in code, "")
        line_count = len([line for line in code.splitlines() if line.strip()])
        check("export is short (<= 40 statements)", line_count <= 40, f"{line_count} lines")

        script = TMP_DIR / "exported_run.py"
        script.write_text(code, encoding="utf-8")
        proc = subprocess.run([str(PYTHON), str(script)], capture_output=True, text=True,
                              timeout=180, cwd=str(TMP_DIR))
        check("exported script exits 0", proc.returncode == 0,
              f"rc={proc.returncode} {proc.stderr.strip()[:300]}")
        stdout = proc.stdout
        print("---- exported script output ----")
        print(stdout.strip())
        print("--------------------------------")
        numbers = []
        for line in stdout.splitlines():
            if "mpgge" in line:
                for token in line.replace("mpgge", " ").split():
                    try:
                        numbers.append(float(token))
                    except ValueError:
                        pass
        exported_mpgge = numbers[0] if numbers else None
        check("exported script prints distance",
              any("mi" in line.lower() for line in stdout.splitlines()), "")
        script_distance = None
        for line in stdout.splitlines():
            if line.strip().startswith("Distance"):
                tokens = line.replace("mi", " ").split()
                for token in tokens:
                    try:
                        script_distance = float(token)
                    except ValueError:
                        pass
        api_distance = modified.get("kpis", {}).get("distance_mi", {}).get("value")
        check("API and exported distance agree at printed precision",
              script_distance is not None and api_distance is not None
              and f"{script_distance:.3f}" == f"{api_distance:.3f}" == "7.451",
              f"script {script_distance:.3f} vs api {api_distance:.6f} -> "
              f"{api_distance:.3f}")
        check("exported fuel economy matches the API (within 1%)",
              exported_mpgge is not None and mod_mpgge is not None
              and abs(exported_mpgge - mod_mpgge) <= 0.01 * mod_mpgge,
              f"script {exported_mpgge} vs api {mod_mpgge}")

        # exported script for a BEV (SOC lines)
        status, _h, exported_bev = post_json(base, "/api/export-python", {
            "preset": "2022 Tesla Model 3 RWD thrml.yaml",
            "cycle": "udds.csv",
        })
        bev_script = TMP_DIR / "exported_bev.py"
        bev_script.write_text(exported_bev.get("code", ""), encoding="utf-8")
        proc_bev = subprocess.run([str(PYTHON), str(bev_script)], capture_output=True,
                                  text=True, timeout=180, cwd=str(TMP_DIR))
        check("exported BEV script exits 0", proc_bev.returncode == 0,
              f"rc={proc_bev.returncode} {proc_bev.stderr.strip()[:300]}")
        check("exported BEV script prints SOC",
              "SOC start" in proc_bev.stdout and "SOC end" in proc_bev.stdout, "")
        check("exported BEV SOC matches the API (98.00 -> 95.91)",
              "98.00 %" in proc_bev.stdout and "95.91 %" in proc_bev.stdout,
              " ".join(proc_bev.stdout.split())[:160])
        print("---- exported BEV output ----")
        print(proc_bev.stdout.strip())
        print("----------------------------")

        # python -c execution (contract 1.3: runnable via python -c)
        proc_c = subprocess.run([str(PYTHON), "-c", exported_bev.get("code", "")],
                                capture_output=True, text=True, timeout=180, cwd=str(TMP_DIR))
        check("exported code runs via python -c", proc_c.returncode == 0,
              f"rc={proc_c.returncode} {proc_c.stderr.strip()[:200]}")

    finally:
        process.terminate()
        try:
            process.wait(timeout=10)
        except subprocess.TimeoutExpired:
            process.kill()
        log_file.close()
        shutil.rmtree(TMP_DIR, ignore_errors=True)

    passed = sum(1 for ok, _ in RESULTS if ok)
    failed = [name for ok, name in RESULTS if not ok]
    print("")
    print(f"== {passed}/{len(RESULTS)} checks passed ==")
    for name in failed:
        print(f"   FAILED: {name}")
    return 0 if not failed else 1


if __name__ == "__main__":
    raise SystemExit(main())
