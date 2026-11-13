r"""
e2e_sweep.py -- Task 5: full-sweep robustness (this is the one that finds bugs).

Loops every preset in the live /api/schema against both cycles with
``render_matplotlib: false`` and asserts a 200 + non-empty kpis + equal-length
time series for each of the combinations. Then re-runs three presets with
``render_matplotlib: true`` and asserts a valid PNG.

Run standalone:  & <venv>\Scripts\python.exe gui\verify\e2e_sweep.py
Exit code: 0 = every combination passed, 1 = at least one failed.
"""

from __future__ import annotations

import base64
import sys
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parent))

import common
from common import Checker

PNG_MAGIC = b"\x89PNG\r\n\x1a\n"
# Three structurally different powertrains: Conv, BEV, HEV.
PNG_SUBSET = [
    ("2012_Ford_Fusion.yaml", "udds.csv"),
    ("2022 Tesla Model 3 RWD thrml.yaml", "udds.csv"),
    ("2016_TOYOTA_Prius_Two.yaml", "hwfet.csv"),
]


def _png(data: dict) -> tuple[bool, int]:
    raw = data.get("matplotlib_png_base64")
    if not isinstance(raw, str) or not raw:
        return False, 0
    try:
        blob = base64.b64decode(raw)
    except Exception:
        return False, 0
    return blob[:8] == PNG_MAGIC, len(blob)


def main() -> int:
    ck = Checker("sweep", "Task 5 — Full-sweep robustness")
    base, provenance = common.ensure_server()
    ck.meta["base_url"] = base
    print(f"[info] verifying against {base} ({provenance})", flush=True)

    schema = common.http_get("/api/schema").json_or_none() or {}
    presets = [p["id"] for p in schema.get("presets") or []]
    cycles = [c["id"] for c in schema.get("cycles") or []]
    ck.eq("SW01", "sweep covers all 9 presets", len(presets), 9)
    ck.eq("SW02", "sweep covers both cycles", cycles, ["udds.csv", "hwfet.csv"])
    combos = [(preset, cycle) for preset in presets for cycle in cycles]
    ck.eq("SW03", "total combinations exercised", len(combos), 18)

    rows: list[dict] = []
    for index, (preset, cycle) in enumerate(combos, start=1):
        resp = common.http_post_json("/api/simulate", {
            "preset": preset, "cycle": cycle, "overrides": {},
            "render_matplotlib": False,
        }, timeout=900.0)
        data = resp.json_or_none() or {}
        kpis = data.get("kpis") or {}
        ts = data.get("timeseries") or {}
        lengths = {k: len(v) for k, v in ts.items() if isinstance(v, list)}
        equal = bool(lengths) and len(set(lengths.values())) == 1
        ts_len = next(iter(lengths.values())) if equal else None
        row = {
            "preset": preset,
            "cycle": cycle,
            "status": resp.status,
            "kpi_count": len(kpis) if isinstance(kpis, dict) else 0,
            "kpi_keys": sorted(kpis.keys()) if isinstance(kpis, dict) else [],
            "ts_keys": sorted(ts.keys()),
            "ts_len": ts_len,
            "equal_lengths": equal,
            "ms": round(resp.elapsed_ms, 1),
            "error": (resp.error or "" if resp.status != 200 else ""),
            "body_snippet": "" if resp.status == 200 else resp.text[:300],
        }
        rows.append(row)
        label = f"{preset} / {cycle}"
        ck.check(f"SW-{index:02d}",
                 f"{label} -> 200 with kpis + equal-length series",
                 resp.status == 200 and row["kpi_count"] > 0 and equal,
                 f"status={resp.status}, kpis={row['kpi_count']}, "
                 f"ts_len={ts_len}, equal={equal}",
                 "status=200, kpis>0, equal", note=row["error"] or "")
        print(f"    {label:<52} status={row['status']} kpis={row['kpi_count']:>2} "
              f"ts={ts_len} {row['ms']} ms", flush=True)

    ok_status = [r for r in rows if r["status"] == 200]
    ck.eq("SW10", "all 18 combinations returned HTTP 200", len(ok_status), 18)
    ck.check("SW11", "all 18 combinations returned a non-empty kpis object",
             all(r["kpi_count"] > 0 for r in rows),
             {r["preset"] + "/" + r["cycle"]: r["kpi_count"] for r in rows
              if r["kpi_count"] == 0} or "all non-empty", "all non-empty")
    ck.check("SW12", "all 18 combinations returned equal-length time series",
             all(r["equal_lengths"] for r in rows),
             [r["preset"] + "/" + r["cycle"] for r in rows
              if not r["equal_lengths"]] or "all equal", "all equal")
    ck.check("SW13", "no combination produced an error payload",
             not [r for r in rows if r["error"]], [r["error"] for r in rows if r["error"]]
             or "none", "[]")
    ck.meta["sweep_rows"] = rows
    ck.meta["sweep_ms_total"] = round(sum(r["ms"] for r in rows), 1)

    png_rows = []
    for index, (preset, cycle) in enumerate(PNG_SUBSET, start=1):
        resp = common.http_post_json("/api/simulate", {
            "preset": preset, "cycle": cycle, "overrides": {},
            "render_matplotlib": True,
        }, timeout=900.0)
        data = resp.json_or_none() or {}
        ok, nbytes = _png(data)
        png_rows.append({"preset": preset, "cycle": cycle, "status": resp.status,
                         "png_bytes": nbytes, "png_ok": ok,
                         "ms": round(resp.elapsed_ms, 1)})
        ck.check(f"SW20-{index}",
                 f"{preset} / {cycle} with render_matplotlib:true -> valid PNG",
                 resp.status == 200 and ok, f"status={resp.status}, png={nbytes} B",
                 "status=200, PNG magic")
    ck.meta["png_rows"] = png_rows

    return ck.finish()


if __name__ == "__main__":
    common.dispatch(main)
