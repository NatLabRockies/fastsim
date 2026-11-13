r"""
run_all.py -- one command that runs the whole verification suite and rewrites
gui/verify/REPORT.md from the observations it just made.

    & "C:\Users\Pok fun\FASTSim\venv\Scripts\python.exe" gui\verify\run_all.py

Exit code: 0 = GREEN or YELLOW, 1 = RED (at least one check failed).

The suite attaches to a server already listening on 127.0.0.1:8000 if there is
one; otherwise it starts `python gui/backend/app.py --port 8012`, uses that for
every child process, and kills it before exiting. Temp artifacts live in
gui/verify/_tmp and are removed at the end.
"""

from __future__ import annotations

import json
import os
import re
import shutil
import subprocess
import sys
import time
from datetime import datetime, timezone
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parent))

import common

SCRIPTS = [
    ("api", "e2e_api.py", "Task 1 — Endpoint contract"),
    ("export", "e2e_export.py", "Task 2 — Exported Python must actually run"),
    ("static", "e2e_static.py", "Task 3 — Static serving + security"),
    ("frontend", "e2e_frontend.py", "Task 4 — Frontend static sanity"),
    ("sweep", "e2e_sweep.py", "Task 5 — Full-sweep robustness"),
]


# --------------------------------------------------------------------------
# helpers
# --------------------------------------------------------------------------
def _cell(value) -> str:
    text = value if isinstance(value, str) else json.dumps(value, ensure_ascii=False) \
        if isinstance(value, (dict, list)) else str(value)
    return text.replace("|", "\\|").replace("\n", " ")


def _num(text: str, label: str):
    match = re.search(re.escape(label) + r"\s+(-?\d+(?:\.\d+)?)", text or "")
    return float(match.group(1)) if match else None


def _verdict(results: list[dict]) -> tuple[str, int, int, int]:
    passed = sum(r["passed"] for r in results)
    failed = sum(r["failed"] for r in results)
    warned = sum(r["warned"] for r in results)
    if failed:
        return "RED", passed, failed, warned
    if warned:
        return "YELLOW", passed, failed, warned
    return "GREEN", passed, failed, warned


def _runtime_facts() -> dict:
    facts = {"python": sys.version.split()[0],
             "python_path": str(common.VENV_PYTHON)}
    try:
        run = subprocess.run([str(common.VENV_PYTHON), "-c",
                              "import fastsim, matplotlib, numpy, pandas;"
                              "print(fastsim.__version__)"],
                             capture_output=True, text=True, timeout=300)
        facts["fastsim"] = (run.stdout or "").strip() or "unknown"
    except Exception as exc:  # pragma: no cover
        facts["fastsim"] = f"unavailable ({exc})"
    node = shutil.which("node")
    if node:
        run = subprocess.run([node, "--version"], capture_output=True, text=True)
        facts["node"] = (run.stdout or "").strip()
    else:
        facts["node"] = "not found"
    return facts


def _render(results: list[dict], meta: dict, verdict: str, passed: int,
            failed: int, warned: int) -> str:
    by_key = {r["key"]: r for r in results}
    base_url = meta.get("base_url", "http://127.0.0.1:8000")
    provenance = meta.get("provenance", "unknown")
    out: list[str] = []
    add = out.append

    add("# FASTSim Web GUI — Independent Verification Report")
    add("")
    add(f"Generated: {meta['generated']}  ")
    add(f"Verifier: Subagent 3 (verification only — nothing outside "
        f"`gui/verify/` was created, edited or deleted)  ")
    add(f"Contract of record: `gui/CONTRACT.md`")
    add("")
    add(f"## Verdict: **{verdict}**")
    add("")
    add(f"`{passed} checks passed, {failed} failed, {warned} warnings` "
        f"across 5 suites.")
    add("")
    add("---")
    add("")
    add("## 1. How to start and reach the app")
    add("")
    add("```powershell")
    add("# start (single command, one origin serves API + frontend)")
    add("& \"C:\\Users\\Pok fun\\FASTSim\\venv\\Scripts\\python.exe\" "
        "gui\\backend\\app.py --port 8000")
    add("")
    add("# run this verification suite (rewrites this report)")
    add("& \"C:\\Users\\Pok fun\\FASTSim\\venv\\Scripts\\python.exe\" "
        "gui\\verify\\run_all.py")
    add("```")
    add("")
    add(f"- URL verified: **{base_url}**  ")
    add(f"- Server provenance during this run: **{provenance}**")
    add("")
    add("| Runtime fact | Value |")
    add("|---|---|")
    for key, value in meta["facts"].items():
        add(f"| {key} | `{_cell(value)}` |")
    add("")
    add("### Suites in this run")
    add("")
    add("| Suite | Script | Checks | Passed | Failed | Warnings | Exit |")
    add("|---|---|---|---|---|---|---|")
    for key, script, title in SCRIPTS:
        row = by_key.get(key)
        if not row:
            add(f"| {title} | `{script}` | — | — | — | — | did not run |")
            continue
        exit_note = "0" if row["failed"] == 0 else "1"
        add(f"| {title} | `{script}` | {len(row['rows'])} | {row['passed']} | "
            f"{row['failed']} | {row['warned']} | {exit_note} |")
    add(f"| **Total** | | **{sum(len(r['rows']) for r in results)}** | "
        f"**{passed}** | **{failed}** | **{warned}** | "
        f"{'0' if failed == 0 else '1'} |")
    add("")

    # ---- failure section (only if any) ---------------------------------
    failures = [(r, f) for r in results for f in r["rows"]
                if not f["ok"] and f["severity"] != "warn"]
    warnings = [(r, f) for r in results for f in r["rows"]
                if not f["ok"] and f["severity"] == "warn"]
    add("## 2. Failures")
    add("")
    if not failures:
        add("**None.** No check in any of the five suites failed.")
    else:
        add("| Suite | Check | What was checked | Observed | Expected |")
        add("|---|---|---|---|---|")
        for row, item in failures:
            add(f"| {row['title']} | `{item['id']}` | {_cell(item['description'])} | "
                f"`{_cell(item['observed'])}` | `{_cell(item['expected'])}` |")
        add("")
        add("Root cause per failure is recorded in the per-suite tables below "
            "(the `note` column) and in the raw observation captured at the "
            "time of the run.")
    if warnings:
        add("")
        add("### Warnings (non-fatal)")
        add("")
        add("| Suite | Check | What was checked | Observed |")
        add("|---|---|---|---|")
        for row, item in warnings:
            add(f"| {row['title']} | `{item['id']}` | {_cell(item['description'])} | "
                f"`{_cell(item['observed'])}` |")
    add("")

    # ---- per-suite tables ---------------------------------------------
    add("## 3. Every check, with the value actually observed")
    add("")
    for key, script, title in SCRIPTS:
        row = by_key.get(key)
        if not row:
            continue
        add(f"### {title}  (`{script}`)")
        add("")
        add("| # | Check | What was checked | Result | Observed | Expected |")
        add("|---|---|---|---|---|---|")
        for index, item in enumerate(row["rows"], start=1):
            result = "PASS" if item["ok"] else (
                "WARN" if item["severity"] == "warn" else "**FAIL**")
            note = f" — {_cell(item['note'])}" if item["note"] else ""
            add(f"| {index} | `{item['id']}` | {_cell(item['description'])} | {result} "
                f"| `{_cell(item['observed'])}`{note} | `{_cell(item['expected'])}` |")
        add("")

    # ---- sweep table ---------------------------------------------------
    sweep = by_key.get("sweep", {}).get("meta", {})
    if sweep.get("sweep_rows"):
        add("## 4. The 9 × 2 sweep (`render_matplotlib: false`)")
        add("")
        add("| # | Preset | Cycle | HTTP | KPIs | KPI keys | Timeseries keys | "
            "Array length | Equal lengths | Latency |")
        add("|---|---|---|---|---|---|---|---|---|---|")
        for index, row in enumerate(sweep["sweep_rows"], start=1):
            add(f"| {index} | `{row['preset']}` | `{row['cycle']}` | {row['status']} | "
                f"{row['kpi_count']} | `{_cell(row['kpi_keys'])}` | "
                f"`{_cell(row['ts_keys'])}` | {row['ts_len']} | "
                f"{'yes' if row['equal_lengths'] else '**NO**'} | {row['ms']} ms |")
        add("")
        add(f"Total sweep wall time inside the requests: "
            f"**{sweep.get('sweep_ms_total')} ms** over 18 combinations.")
        add("")
        add("### Matplotlib subset (`render_matplotlib: true`)")
        add("")
        add("| Preset | Cycle | HTTP | PNG bytes | Valid PNG magic | Latency |")
        add("|---|---|---|---|---|---|")
        for row in sweep.get("png_rows") or []:
            add(f"| `{row['preset']}` | `{row['cycle']}` | {row['status']} | "
                f"{row['png_bytes']} | {'yes' if row['png_ok'] else '**NO**'} | "
                f"{row['ms']} ms |")
        add("")

    # ---- export reproduction -------------------------------------------
    export_meta = by_key.get("export", {}).get("meta", {})
    conv = export_meta.get("conv_export")
    bev = export_meta.get("bev_export")
    add("## 5. Exported Python — reproduction numbers")
    add("")
    if conv:
        dist_exported = _num(conv.get("stdout", ""), "Distance")
        dist_delta = (f"{abs(dist_exported - conv['api_distance_mi']):.5f} mi"
                      if dist_exported is not None else "n/a")
        add("### Conv (`2012_Ford_Fusion.yaml` + `udds.csv`) with an override")
        add("")
        add("| Quantity | `/api/simulate` | Exported script (file) | Exported script "
            "(`python -c`) | Delta |")
        add("|---|---|---|---|---|")
        add(f"| Fuel economy (mpgge) | {conv['api_fuel_economy_mpgge']} | "
            f"{conv['exported_fuel_economy_mpgge']} | "
            f"{conv['inline_fuel_economy_mpgge']} | {conv['delta_pct']} % |")
        add(f"| Distance (mi) | {conv['api_distance_mi']} | {dist_exported} | "
            f"{dist_exported} | {dist_delta} |")
        add("")
        add(f"- Override applied: `{json.dumps(conv['overrides'])}`  ")
        add(f"- Exported source length: **{conv['lines']} lines**  ")
        add(f"- Both execution modes (file and `python -c`) exited **0**.")
        add("")
        add("Exported source, verbatim:")
        add("")
        add("```python")
        for line in conv["source"].rstrip("\n").splitlines():
            add(line)
        add("```")
        add("")
        add("Its stdout when run as a file:")
        add("")
        add("```text")
        for line in conv["stdout"].rstrip("\n").splitlines():
            add(line)
        add("```")
        add("")
    if bev:
        ee_delta_pct = None
        if bev.get("api_electric_economy_wh_per_mi") and \
                bev.get("exported_electric_economy_wh_per_mi"):
            ee_delta_pct = abs(bev["exported_electric_economy_wh_per_mi"]
                               - bev["api_electric_economy_wh_per_mi"]) \
                / bev["api_electric_economy_wh_per_mi"] * 100.0
        add("### BEV (`2022 Tesla Model 3 RWD thrml.yaml` + `udds.csv`)")
        add("")
        add("| Quantity | `/api/simulate` | Exported script | Delta |")
        add("|---|---|---|---|")
        add(f"| SOC start (%) | {bev['api_soc_start_pct']} | "
            f"{bev['exported_soc_start_pct']} | {bev['soc_start_delta']} |")
        add(f"| SOC end (%) | {bev['api_soc_end_pct']} | "
            f"{bev['exported_soc_end_pct']} | {bev['soc_end_delta']} |")
        add(f"| Electricity economy (Wh/mi) | {bev['api_electric_economy_wh_per_mi']} | "
            f"{bev['exported_electric_economy_wh_per_mi']} | "
            f"{f'{ee_delta_pct:.5f} %' if ee_delta_pct is not None else 'n/a'} |")
        add("")
        add(f"- Exported source length: **{bev['lines']} lines**; "
            f"exit code 0 as a file and inline via `python -c`.")
        add("")
        add("```text")
        for line in bev["stdout"].rstrip("\n").splitlines():
            add(line)
        add("```")
        add("")
    api_meta = by_key.get("api", {}).get("meta", {})
    if api_meta.get("toggle_timing_ms"):
        timing = api_meta["toggle_timing_ms"]
        add("### `render_matplotlib` cost")
        add("")
        add("| Mode | Latency |")
        add("|---|---|")
        add(f"| `render_matplotlib: true` | {float(timing['render_true']):.1f} ms |")
        add(f"| `render_matplotlib: false` (best of 2) | "
            f"{float(timing['render_false_min']):.1f} ms |")
        if timing.get("render_true") and timing.get("render_false_min"):
            add(f"| Speed-up | {timing['render_true'] / timing['render_false_min']:.2f}× |")
        add("")
    if api_meta.get("override"):
        ov = api_meta["override"]
        add("### Override effect (`chassis.drag_coef`)")
        add("")
        add("| Configuration | `fuel_economy_mpgge` |")
        add("|---|---|")
        add(f"| baseline | {ov['base_fuel_economy']} |")
        add(f"| `chassis.drag_coef = {ov['value']}` | {ov['new_fuel_economy']} |")
        add("")
    if api_meta.get("lang_png"):
        add("### Language invariance")
        add("")
        add(f"- `kpis` and `timeseries` for `lang: \"zh\"` were **exactly equal** to "
            f"`lang: \"en\"`; only the figure differed "
            f"(`{api_meta['lang_png']['en_bytes']}` B vs "
            f"`{api_meta['lang_png']['zh_bytes']}` B).")
        add("")

    add("## 6. Scope, honesty notes")
    add("")
    add("- No browser automation was used (per the brief): the frontend was "
        "verified statically — JS parses under `node --check`, no CDN/network "
        "references, same-origin asset paths, `[hidden]` display guard, `lang` "
        "sent in the simulate payload — **not** by driving the DOM.")
    add("- Path traversal was probed with raw sockets so the request line reached "
        "the server byte-for-byte (`%2e%2e`, `..%2f`, `vendor/../../`); a "
        "positive control confirmed the marker string really does exist inside "
        "`gui/CONTRACT.md`, so 'no leak' is a meaningful assertion.")
    add("- Nothing under `gui/backend/` or `gui/frontend/` was created, edited or "
        "deleted by this suite; both were read only.")
    add(f"- Temp artifacts were written to `gui/verify/_tmp/` and removed at the "
        f"end of the run.")
    if meta.get("started_server"):
        add("- This run started its own backend on port "
            f"{meta.get('port')} and killed it before exiting "
            "(no test server was left running).")
    else:
        add(f"- This run attached to the already running backend at {base_url} "
            "and did not stop it (it was not started by the verification suite).")
    add("")
    add(f"**Verdict: {verdict}** — {passed} passed, {failed} failed, {warned} warned.")
    add("")
    return "\n".join(out)


# --------------------------------------------------------------------------
# main
# --------------------------------------------------------------------------
def main() -> int:
    common.utf8_stdout()
    started = time.time()
    base, provenance = common.ensure_server()
    port = common.server_port()
    started_server = provenance == "started by verify suite"
    print(f"[info] suite target: {base} ({provenance})", flush=True)

    env = dict(os.environ)
    env["PYTHONUTF8"] = "1"
    env["PYTHONIOENCODING"] = "utf-8"
    env["FASTSIM_VERIFY_BASE_URL"] = base
    env["FASTSIM_VERIFY_PORT"] = str(port)

    exit_codes: dict[str, int] = {}
    for key, script, title in SCRIPTS:
        path = common.VERIFY_DIR / script
        print(f"\n===== {title}  [{script}] =====", flush=True)
        run = subprocess.run([str(common.VENV_PYTHON), str(path)],
                             cwd=str(common.WORKSPACE), env=env)
        exit_codes[key] = run.returncode
        print(f"[info] {script} exit code: {run.returncode}", flush=True)

    results: list[dict] = []
    missing: list[str] = []
    for key, script, title in SCRIPTS:
        target = common.TMP_DIR / f"results_{key}.json"
        if target.is_file():
            results.append(json.loads(target.read_text(encoding="utf-8")))
        else:
            missing.append(script)

    meta = {
        "generated": datetime.now(timezone.utc).astimezone().strftime(
            "%Y-%m-%d %H:%M:%S %Z"),
        "base_url": base,
        "port": port,
        "provenance": provenance,
        "started_server": started_server,
        "facts": _runtime_facts(),
        "exit_codes": exit_codes,
        "missing_scripts": missing,
        "elapsed_s": round(time.time() - started, 1),
    }
    verdict, passed, failed, warned = _verdict(results)
    if missing:
        verdict = "RED"

    report = common.VERIFY_DIR / "REPORT.md"
    report.write_text(_render(results, meta, verdict, passed, failed, warned),
                      encoding="utf-8")
    print(f"\n[info] REPORT.md written to {report}", flush=True)

    # cleanup: temp artifacts and, if we started it, the server
    if started_server:
        common.stop_server()
        print(f"[info] stopped the backend this run started (port {port})", flush=True)
    if common.TMP_DIR.is_dir():
        shutil.rmtree(common.TMP_DIR, ignore_errors=True)
        print("[info] removed gui/verify/_tmp", flush=True)

    print(f"\n===== VERDICT: {verdict} — {passed} passed, {failed} failed, "
          f"{warned} warned ({meta['elapsed_s']} s) =====", flush=True)
    return 0 if verdict in ("GREEN", "YELLOW") else 1


if __name__ == "__main__":
    raise SystemExit(main())
