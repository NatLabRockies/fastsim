"""Prove the en/zh payloads are byte-identical to the pre-change backend.

Runs the same two simulations against the current backend and against the
pristine copy in orig-backend/, then compares the JSON and the PNG bytes.
"""

import hashlib
import json
import subprocess
import sys
from pathlib import Path

HERE = Path(__file__).parent
PY = r"C:\Users\Pok fun\FASTSim\venv\Scripts\python.exe"

PROBE = r"""
import json, sys
sys.path.insert(0, sys.argv[1])
import simulate
out = {}
for lang in ("en", "zh", "zh-Hant"):
    try:
        payload = simulate.run_simulation(
            preset="2012_Ford_Fusion.yaml", cycle="udds.csv", overrides={},
            render_matplotlib=True, lang=lang)
    except Exception as exc:
        out[lang] = {"error": f"{type(exc).__name__}: {exc}"}
        continue
    out[lang] = {
        "lang": payload.get("lang"),
        "kpis": payload.get("kpis"),
        "timeseries": payload.get("timeseries"),
        "png": payload.get("matplotlib_png_base64"),
        "svg_len": len(payload.get("matplotlib_svg") or ""),
    }
print(json.dumps(out))
"""


def run(backend_dir: Path, label: str) -> dict:
    proc = subprocess.run([PY, "-c", PROBE, str(backend_dir)], capture_output=True, text=True,
                          encoding="utf-8", errors="replace", env=None)
    if proc.returncode != 0:
        print(proc.stderr[-3000:])
        raise SystemExit(f"{label} probe failed")
    return json.loads(proc.stdout.strip().splitlines()[-1])


def digest(value) -> str:
    blob = json.dumps(value, sort_keys=True, ensure_ascii=False).encode("utf-8")
    return hashlib.sha256(blob).hexdigest()[:16]


def main() -> int:
    old = run(HERE / "orig-backend", "original")
    new = run(Path(r"C:\Users\Pok fun\FASTSim\gui\backend"), "current")
    ok = True
    for lang in ("en", "zh"):
        for field in ("kpis", "timeseries", "png", "lang"):
            same = old[lang][field] == new[lang][field]
            ok &= same
            detail = digest(old[lang][field]) if field != "lang" else repr(old[lang][field])
            print(f"{lang:5s} {field:10s} identical={same}  {detail}")
    print(f"zh-Hant present in new only: {new['zh-Hant']['lang']!r}, "
          f"PNG differs from en: {new['zh-Hant']['png'] != new['en']['png']}")
    print(f"zh-Hant numbers == en numbers: "
          f"{new['zh-Hant']['kpis'] == new['en']['kpis'] and new['zh-Hant']['timeseries'] == new['en']['timeseries']}")
    ok &= new["zh-Hant"]["kpis"] == new["en"]["kpis"]
    print("\nRESULT:", "PASS — en/zh are byte-identical to the pre-change backend" if ok else "FAIL")
    return 0 if ok else 1


if __name__ == "__main__":
    sys.exit(main())
