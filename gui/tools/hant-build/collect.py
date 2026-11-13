"""Collect every Simplified-Chinese string the backend can put on screen.

Runs in-process (no HTTP): builds the schema for every preset and runs one
simulation per distinct powertrain so KPI labels are covered too.

Output: corpus.json  {"strings": [{"kind":..., "text":...}, ...]}
"""

import json
import re
import sys
from pathlib import Path

BACKEND = Path(r"C:\Users\Pok fun\FASTSim\gui\backend")
sys.path.insert(0, str(BACKEND))

import introspection  # noqa: E402
import simulate  # noqa: E402

CJK = re.compile(r"[\u3400-\u9fff\uf900-\ufaff\u3000-\u303f\uff00-\uffef\u2014\u2026]")


def has_cjk(s: str) -> bool:
    return bool(isinstance(s, str) and CJK.search(s))


def walk(node, kind, out):
    if isinstance(node, dict):
        for k, v in node.items():
            walk(v, f"{kind}.{k}", out)
    elif isinstance(node, list):
        for v in node:
            walk(v, kind, out)
    elif isinstance(node, str) and has_cjk(node):
        out.append({"kind": kind, "text": node})


def main() -> int:
    out: list[dict] = []

    ids = [p["id"] for p in introspection.list_presets()] if hasattr(introspection, "list_presets") else []
    if not ids:
        base = introspection.build_schema(None)
        ids = [p["id"] for p in base["presets"]]
        walk(base, "schema", out)

    for pid in ids:
        try:
            walk(introspection.build_schema(pid), f"schema[{pid}]", out)
        except Exception as exc:  # pragma: no cover
            print(f"[warn] {pid}: {exc}", file=sys.stderr)

    seen_pt = set()
    for pid in ids:
        try:
            schema = introspection.build_schema(pid)
        except Exception:
            continue
        pt = schema.get("pt_type") or (schema.get("presets") or [{}])[0].get("pt_type")
        if pt in seen_pt:
            continue
        seen_pt.add(pt)
        try:
            payload = simulate.run_simulation(
                preset=pid, cycle="udds.csv", overrides={}, render_matplotlib=False, lang="zh"
            )
            walk(payload, f"simulate[{pt}]", out)
        except Exception as exc:  # pragma: no cover
            print(f"[warn] simulate {pid}: {exc}", file=sys.stderr)

    # backend modules' label tables that the frontend can display verbatim
    import metadata  # noqa: E402
    import plots  # noqa: E402

    for table, name in ((plots.LABELS.get("zh", {}), "plots.zh"),):
        for k, v in table.items():
            if has_cjk(v):
                out.append({"kind": f"{name}.{k}", "text": v})
    for path in ("mass_kilograms", "pwr_aux_base_watts", "name", "chassis.drag_coef",
                 "pt_type.fc.fs.fuel_type", "pt_type.res.max_charge_rate_watts",
                 "pt_type.em.rated_power_watts"):
        try:
            g, gz, sg, sgz = metadata.classify(path)
            for v in (gz, sgz):
                if has_cjk(v):
                    out.append({"kind": "classify", "text": v})
        except Exception:
            pass

    uniq = {}
    for item in out:
        uniq.setdefault(item["text"], item["kind"])
    strings = [{"kind": k, "text": t} for t, k in sorted(uniq.items())]
    dest = Path(__file__).with_name("corpus.json")
    dest.write_text(json.dumps({"count": len(strings), "strings": strings},
                               ensure_ascii=False, indent=1), encoding="utf-8")
    print(f"collected {len(strings)} distinct backend strings -> {dest.name}")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
