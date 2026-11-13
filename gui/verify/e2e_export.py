r"""
e2e_export.py -- Task 2: the exported Python must actually run.

Verifies POST /api/export-python for a Conv preset (with an override) and a BEV
preset: the source must be minimal/pure, must run as a file, must run inline via
``python -c``, and must reproduce the API's numbers.

Run standalone:  & <venv>\Scripts\python.exe gui\verify\e2e_export.py
Exit code: 0 = every check passed, 1 = at least one check failed.
"""

from __future__ import annotations

import os
import re
import subprocess
import sys
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parent))

import common
from common import Checker

FORBIDDEN_IMPORTS = ("flask", "fastapi", "http.server", "matplotlib", "tkinter",
                     "uvicorn", "starlette", "requests", "numpy", "pandas")
NUM = r"(-?\d+(?:\.\d+)?)"


def _env() -> dict:
    env = dict(os.environ)
    env["PYTHONUTF8"] = "1"
    env["PYTHONIOENCODING"] = "utf-8"
    return env


def _run(argv: list[str], timeout: float = 900.0) -> subprocess.CompletedProcess:
    return subprocess.run(argv, cwd=str(common.WORKSPACE), env=_env(),
                          capture_output=True, text=True, encoding="utf-8",
                          errors="replace", timeout=timeout)


def _grab(text: str, label: str):
    match = re.search(re.escape(label) + r"\s+" + NUM, text)
    return float(match.group(1)) if match else None


def _purity_checks(ck: Checker, prefix: str, code: str, preset: str) -> None:
    ck.check(f"{prefix}01", "exported source is a non-empty string",
             isinstance(code, str) and len(code) > 50, len(code) if code else 0,
             "> 50 chars")
    ck.check(f"{prefix}02", "exported source contains 'import fastsim'",
             "import fastsim" in code, "import fastsim" in code, True)
    ck.check(f"{prefix}03", "exported source contains NO class definition",
             "class " not in code,
             [ln for ln in code.splitlines() if "class " in ln][:2] or "none",
             "no 'class '")
    ck.check(f"{prefix}04", "exported source contains NO function definition",
             not re.search(r"(?m)^\s*def\s+", code),
             [ln for ln in code.splitlines() if re.search(r"(?m)^\s*def\s+", ln)][:2]
             or "none", "no 'def '")
    ck.check(f"{prefix}05", "exported source contains NO main()/print-wrapper",
             "def main" not in code and "__main__" not in code,
             "def main" in code or "__main__" in code, False)

    import_lines = [ln.strip() for ln in code.splitlines()
                    if re.match(r"\s*(import|from)\s+", ln)]
    offenders = [ln for ln in import_lines
                 if any(mod in ln for mod in FORBIDDEN_IMPORTS)]
    ck.check(f"{prefix}06",
             "no flask/fastapi/http.server/matplotlib/tkinter imports",
             not offenders, offenders or import_lines, "only fastsim")
    roots = set()
    for line in import_lines:
        tokens = line.replace(",", " ").split()
        if not tokens:
            continue
        if tokens[0] == "from":
            roots.add(tokens[1].split(".")[0])
        else:
            roots.update(tok.split(".")[0] for tok in tokens[1:]
                         if tok not in ("as",))
    ck.check(f"{prefix}07", "every import root is fastsim (or yaml)",
             roots <= {"fastsim", "yaml"}, sorted(roots) or "none", "{fastsim,yaml}")

    line_count = len(code.rstrip("\n").splitlines())
    ck.check(f"{prefix}08", "exported source is short (<= 40 lines)",
             line_count <= 40, f"{line_count} lines", "<= 40")
    ck.check(f"{prefix}09", "exported source loads the exact preset resource",
             preset in code and "from_resource" in code, preset in code,
             preset)
    ck.check(f"{prefix}10", "exported source calls set_save_interval(1)",
             "set_save_interval(1)" in code, "set_save_interval(1)" in code, True)
    ck.check(f"{prefix}11", "exported source prints a tidy aligned block",
             bool(re.search(r"print\(f\"\{key:", code)), "print(f\"{key:...}", True)


def section_conv(ck: Checker) -> None:
    override = {"chassis.drag_coef": 0.30}
    config = {"preset": common.FUSION, "cycle": common.UDDS, "overrides": override}

    api_resp = common.http_post_json("/api/simulate", {**config,
                                                       "render_matplotlib": False})
    api_kpis = (api_resp.json_or_none() or {}).get("kpis") or {}
    api_fe = (api_kpis.get("fuel_economy_mpgge") or {}).get("value")
    api_dist = (api_kpis.get("distance_mi") or {}).get("value")
    ck.check("E00", "reference /api/simulate for the same config -> 200",
             api_resp.status == 200 and api_fe is not None,
             f"status={api_resp.status}, fuel_economy={api_fe}", "200 + value")

    resp = common.http_post_json("/api/export-python", config)
    ck.eq("E01", "POST /api/export-python (Fusion+UDDS+override) -> HTTP 200",
          resp.status, 200, note=resp.error or "")
    data = resp.json_or_none() or {}
    code = data.get("code") or ""
    ck.check("E02", "response has a 'code' string", isinstance(code, str) and bool(code),
             f"{len(code)} chars", "non-empty string")

    _purity_checks(ck, "X", code, common.FUSION)
    ck.check("X12", "exported source applies the user's override",
             "chassis.drag_coef" in code and "0.3" in code,
             [ln for ln in code.splitlines() if "chassis.drag_coef" in ln] or "absent",
             "override present")
    ck.check("X13", "no matplotlib/plotting code in the export",
             "matplotlib" not in code and "plot" not in code.lower(),
             "found" if ("matplotlib" in code or "plot" in code.lower()) else "none",
             "none")

    common.TMP_DIR.mkdir(parents=True, exist_ok=True)
    target = common.TMP_DIR / "exported_fusion.py"
    target.write_text(code, encoding="utf-8")
    ck.check("E03", "exported source written to gui/verify/_tmp/exported_fusion.py",
             target.is_file() and target.stat().st_size > 50,
             f"{target.stat().st_size if target.is_file() else 0} bytes", "> 50")

    run = _run([str(common.VENV_PYTHON), str(target)])
    ck.eq("E04", "exported script executed as a FILE exits 0", run.returncode, 0,
          note=(run.stderr or "").strip().splitlines()[-1][:200] if run.returncode else "")

    file_fe = _grab(run.stdout, "Fuel economy")
    ck.check("E05", "file run prints a 'Fuel economy' number",
             file_fe is not None, file_fe, "a number")
    delta_pct = (abs(file_fe - api_fe) / api_fe * 100.0
                 if (file_fe is not None and api_fe) else None)
    ck.check("E06", "file-run fuel economy within 1% of /api/simulate",
             delta_pct is not None and delta_pct <= 1.0,
             f"api={api_fe}, exported={file_fe}, delta={delta_pct:.4f}%"
             if delta_pct is not None else "n/a", "<= 1%")
    file_dist = _grab(run.stdout, "Distance")
    ck.check("E07", "file run distance matches /api/simulate",
             file_dist is not None and api_dist is not None
             and abs(file_dist - api_dist) <= max(0.002, api_dist * 0.001),
             f"api={api_dist}, exported={file_dist}", "match")
    ck.check("E08", "file run stdout has no traceback",
             "Traceback" not in run.stderr and "Traceback" not in run.stdout,
             (run.stderr or "").strip()[-300:] or "clean", "clean")

    inline = _run([str(common.VENV_PYTHON), "-c", code])
    ck.eq("E09", "exported source runs INLINE via python -c, exits 0",
          inline.returncode, 0,
          note=(inline.stderr or "").strip().splitlines()[-1][:200]
          if inline.returncode else "")
    inline_fe = _grab(inline.stdout, "Fuel economy")
    ck.check("E10", "inline run reproduces the same fuel economy",
             inline_fe is not None and file_fe is not None
             and abs(inline_fe - file_fe) < 0.02,
             f"file={file_fe}, inline={inline_fe}", "match")
    ck.check("E11", "inline run needs no file (stdout non-empty)",
             bool(inline.stdout.strip()), f"{len(inline.stdout)} chars", "> 0")

    ck.meta["conv_export"] = {
        "preset": common.FUSION,
        "cycle": common.UDDS,
        "overrides": override,
        "api_fuel_economy_mpgge": api_fe,
        "api_distance_mi": api_dist,
        "exported_fuel_economy_mpgge": file_fe,
        "inline_fuel_economy_mpgge": inline_fe,
        "delta_pct": round(delta_pct, 5) if delta_pct is not None else None,
        "lines": len(code.rstrip("\n").splitlines()),
        "source": code,
        "stdout": run.stdout,
    }


def section_bev(ck: Checker) -> None:
    config = {"preset": common.TESLA, "cycle": common.UDDS, "overrides": {}}
    api_resp = common.http_post_json("/api/simulate", {**config,
                                                       "render_matplotlib": False})
    kpis = (api_resp.json_or_none() or {}).get("kpis") or {}
    api_soc0 = (kpis.get("soc_start_pct") or {}).get("value")
    api_soc1 = (kpis.get("soc_end_pct") or {}).get("value")
    api_ee = (kpis.get("electric_economy_wh_per_mi") or {}).get("value")

    resp = common.http_post_json("/api/export-python", config)
    ck.eq("V01", "POST /api/export-python (Tesla BEV) -> HTTP 200", resp.status, 200,
          note=resp.error or "")
    code = (resp.json_or_none() or {}).get("code") or ""
    _purity_checks(ck, "Y", code, common.TESLA)
    ck.check("Y12", "BEV export prints SOC (start + end)",
             "SOC start" in code and "SOC end" in code,
             {"SOC start": "SOC start" in code, "SOC end": "SOC end" in code},
             {"SOC start": True, "SOC end": True})
    ck.check("Y13", "BEV export prints electricity economy, not mpgge",
             "Electricity economy" in code and "Fuel economy" not in code,
             {"electricity": "Electricity economy" in code,
              "fuel": "Fuel economy" in code}, {"electricity": True, "fuel": False})

    common.TMP_DIR.mkdir(parents=True, exist_ok=True)
    target = common.TMP_DIR / "exported_tesla.py"
    target.write_text(code, encoding="utf-8")
    run = _run([str(common.VENV_PYTHON), str(target)])
    ck.eq("V02", "BEV exported script exits 0", run.returncode, 0,
          note=(run.stderr or "").strip().splitlines()[-1][:200]
          if run.returncode else "")
    ck.check("V03", "BEV exported script stdout has no traceback",
             "Traceback" not in run.stderr, (run.stderr or "").strip()[-300:] or "clean",
             "clean")

    soc0 = _grab(run.stdout, "SOC start")
    soc1 = _grab(run.stdout, "SOC end")
    ee = _grab(run.stdout, "Electricity economy")
    ck.check("V04", "BEV export prints SOC start/end values",
             soc0 is not None and soc1 is not None, f"{soc0} -> {soc1}", "two numbers")
    ck.check("V05", "exported SOC start matches /api/simulate",
             soc0 is not None and api_soc0 is not None and abs(soc0 - api_soc0) <= 0.02,
             f"api={api_soc0}, exported={soc0}", "match")
    ck.check("V06", "exported SOC end matches /api/simulate",
             soc1 is not None and api_soc1 is not None and abs(soc1 - api_soc1) <= 0.02,
             f"api={api_soc1}, exported={soc1}", "match")
    ee_delta = (abs(ee - api_ee) / api_ee * 100.0
                if (ee is not None and api_ee) else None)
    ck.check("V07", "exported electricity economy within 1% of /api/simulate",
             ee_delta is not None and ee_delta <= 1.0,
             f"api={api_ee}, exported={ee}, delta={ee_delta:.4f}%"
             if ee_delta is not None else "n/a", "<= 1%")

    inline = _run([str(common.VENV_PYTHON), "-c", code])
    ck.eq("V08", "BEV exported source runs INLINE via python -c, exits 0",
          inline.returncode, 0,
          note=(inline.stderr or "").strip().splitlines()[-1][:200]
          if inline.returncode else "")

    ck.meta["bev_export"] = {
        "preset": common.TESLA,
        "cycle": common.UDDS,
        "api_soc_start_pct": api_soc0,
        "api_soc_end_pct": api_soc1,
        "api_electric_economy_wh_per_mi": api_ee,
        "exported_soc_start_pct": soc0,
        "exported_soc_end_pct": soc1,
        "exported_electric_economy_wh_per_mi": ee,
        "soc_start_delta": (round(abs(soc0 - api_soc0), 5)
                            if soc0 is not None and api_soc0 is not None else None),
        "soc_end_delta": (round(abs(soc1 - api_soc1), 5)
                          if soc1 is not None and api_soc1 is not None else None),
        "lines": len(code.rstrip("\n").splitlines()),
        "source": code,
        "stdout": run.stdout,
    }


def main() -> int:
    ck = Checker("export", "Task 2 — Exported Python must actually run")
    base, provenance = common.ensure_server()
    ck.meta["base_url"] = base
    print(f"[info] verifying against {base} ({provenance})", flush=True)
    section_conv(ck)
    section_bev(ck)
    return ck.finish()


if __name__ == "__main__":
    common.dispatch(main)
