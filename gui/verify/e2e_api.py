r"""
e2e_api.py -- Task 1: endpoint contract verification.

Covers GET /api/schema, GET /api/schema?preset=<id> for a Conv and a BEV
preset, POST /api/simulate for both powertrains, the render_matplotlib toggle,
override handling, the unknown-path 400, and the lang/zh invariance rule.

Run standalone:  & <venv>\Scripts\python.exe gui\verify\e2e_api.py
Exit code: 0 = every check passed, 1 = at least one check failed.
"""

from __future__ import annotations

import base64
import sys
from pathlib import Path
from urllib.parse import quote

sys.path.insert(0, str(Path(__file__).resolve().parent))

import common
from common import Checker

ALLOWED_TYPES = {"float", "int", "bool", "str", "enum", "curve", "object"}
SCALAR_TYPES = {"float", "int", "bool", "enum"}
PARAM_REQUIRED = ("path", "name", "group", "subgroup", "type", "default",
                  "unit", "description", "editable")
UNITS = {"-", "W", "J", "kg", "s", "m", "m/s", "m/s²", "m²", "m³", "K", "W/K",
         "J/K", "kg·m²", "W/(m²·K)", "W/kg", "J/kg", "kg/m³"}


def _png_ok(payload: dict) -> tuple[bool, int, bytes]:
    raw = payload.get("matplotlib_png_base64")
    if not isinstance(raw, str) or not raw:
        return False, 0, b""
    try:
        blob = base64.b64decode(raw, validate=False)
    except Exception:
        return False, 0, b""
    return blob[:8] == b"\x89PNG\r\n\x1a\n", len(blob), blob


def section_schema(ck: Checker) -> None:
    resp = common.http_get("/api/schema")
    ck.eq("S01", "GET /api/schema -> HTTP 200", resp.status, 200,
          note=resp.error or "")
    ck.check("S02", "GET /api/schema content-type is JSON",
             "application/json" in resp.content_type, resp.content_type,
             "application/json*")
    data = resp.json_or_none()
    if not isinstance(data, dict):
        ck.check("S03", "GET /api/schema body is a JSON object", False,
                 resp.text[:200], "JSON object")
        return
    ck.meta["schema_keys"] = sorted(data.keys())

    presets = data.get("presets") or []
    cycles = data.get("cycles") or []
    params = data.get("parameters") or []

    ck.eq("S03", "schema.presets count == 9", len(presets), 9)
    ck.eq("S04", "schema.presets ids match CONTRACT section 2",
          [p.get("id") for p in presets],
          ["2012_Ford_Fusion.yaml", "2016_TOYOTA_Prius_Two.yaml",
           "2016 Nissan Leaf 30 kWh thrml.yaml", "2022 Tesla Model 3 RWD thrml.yaml",
           "2022_Renault_Zoe_ZE50_R135.yaml", "2020 Chevrolet Bolt EV thrml.yaml",
           "2021_Hyundai_Sonata_Hybrid_Blue_thrml.yaml",
           "2026_Chrysler_Pacifica_Select.yaml",
           "2026_Chrysler_Pacifica_Select_thrml.yaml"])
    ck.check("S05", "every preset carries id/name/pt_type/mass_kg",
             all({"id", "name", "pt_type", "mass_kg"} <= set(p) for p in presets),
             [(p.get("id"), p.get("pt_type"), p.get("mass_kg")) for p in presets][:3],
             "all four keys present")
    ck.meta["presets"] = [(p.get("id"), p.get("pt_type")) for p in presets]

    ck.eq("S06", "schema.cycles count == 2", len(cycles), 2)
    ck.eq("S07", "schema.cycles ids are udds.csv + hwfet.csv",
          [c.get("id") for c in cycles], ["udds.csv", "hwfet.csv"])
    ck.check("S08", "cycles carry name/name_zh/duration_s",
             all({"name", "name_zh", "duration_s"} <= set(c)
                 and isinstance(c.get("duration_s"), int) for c in cycles),
             [(c.get("name"), c.get("name_zh"), c.get("duration_s")) for c in cycles],
             "name, name_zh, int duration_s")

    ck.check("S09", "schema.parameters count >= 40", len(params) >= 40, len(params),
             ">= 40")
    ck.meta["param_count_default"] = len(params)

    missing = []
    for param in params:
        absent = [key for key in PARAM_REQUIRED if key not in param]
        if absent:
            missing.append((param.get("path"), absent))
    ck.check("S10", "every parameter has path/name/group/subgroup/type/default/unit/"
                    "description/editable", not missing,
             missing[:3] if missing else f"{len(params)} parameters complete",
             "no missing keys")

    bad_types = sorted({p.get("type") for p in params} - ALLOWED_TYPES)
    ck.check("S11", "parameter types within the contract set", not bad_types,
             bad_types or sorted({p.get("type") for p in params}), sorted(ALLOWED_TYPES))

    bad_editable = [p.get("path") for p in params
                    if p.get("editable") and p.get("type") not in SCALAR_TYPES]
    ck.check("S12", "editable=True only for float/int/bool/enum", not bad_editable,
             bad_editable[:5] or "all editable params are scalar", "[]")

    paths = [p.get("path") for p in params]
    dupes = sorted({path for path in paths if paths.count(path) > 1})
    ck.check("S13", "parameter paths are unique", not dupes, dupes[:5] or "unique", "[]")
    ck.check("S14", "every parameter path is enabled (no 'None' variants)",
             not [p for p in paths if "None" in str(p).split(".")],
             [p for p in paths if "None" in str(p).split(".")][:5] or "clean", "[]")

    bad_units = sorted({p.get("unit") for p in params} - UNITS)
    ck.check("S15", "units are drawn from the CONTRACT suffix table", not bad_units,
             bad_units or sorted({p.get("unit") for p in params}), sorted(UNITS))

    # unit parsing spot-check against CONTRACT rule 5
    by_name = {}
    for param in params:
        by_name.setdefault(param.get("name"), param.get("unit"))
    for name, expected_unit in (("drag_coef", "-"), ("frontal_area_square_meters", "m²"),
                                ("pwr_out_max_watts", "W"), ("mass_kilograms", "kg"),
                                ("wheel_radius_meters", "m")):
        if name in by_name:
            ck.eq(f"S15u-{name}", f"unit parsed from suffix for '{name}'",
                  by_name[name], expected_unit)

    bare = [p.get("path") for p in params
            if not p.get("group") or not p.get("subgroup")
            or not p.get("group_zh") or not p.get("subgroup_zh")]
    ck.check("S16", "every parameter has group/subgroup + _zh variants", not bare,
             bare[:5] or "all grouped", "[]")

    ck.check("S17", "every parameter has a non-empty English description",
             all(isinstance(p.get("description"), str) and p.get("description")
                 for p in params),
             sum(1 for p in params if p.get("description")), f"{len(params)}")
    ck.check("S18", "schema without ?preset echoes the first preset",
             data.get("preset") == "2012_Ford_Fusion.yaml", data.get("preset"),
             "2012_Ford_Fusion.yaml")


def section_schema_presets(ck: Checker) -> tuple[int, int]:
    conv = common.http_get(f"/api/schema?preset={quote(common.FUSION, safe='')}")
    ck.eq("P01", "GET /api/schema?preset=<Conv> -> HTTP 200", conv.status, 200,
          note=conv.error or "")
    cdata = conv.json_or_none() or {}
    cpaths = [p.get("path", "") for p in cdata.get("parameters") or []]

    ck.eq("P02", "Conv schema echoes preset", cdata.get("preset"), common.FUSION)
    ck.check("P03", "Conv parameters all use the concrete pt_type.Conv. prefix",
             bool(cpaths) and all(p.startswith("pt_type.Conv.") or "." not in p
                                  or not p.startswith("pt_type.")
                                  for p in cpaths),
             sum(1 for p in cpaths if p.startswith("pt_type.Conv.")),
             "no non-Conv pt_type paths")
    ck.check("P04", "Conv schema exposes fc (fuel converter) paths",
             any(".fc." in p for p in cpaths) and not any("fc" in p and "fc" not in p
                                                          for p in cpaths),
             [p for p in cpaths if ".fc." in p][:3], "fc paths present")
    ck.check("P05", "Conv schema has NO res (battery) paths",
             not [p for p in cpaths if ".res." in p],
             [p for p in cpaths if ".res." in p][:3] or "none", "[]")

    bev = common.http_get(f"/api/schema?preset={quote(common.TESLA, safe='')}")
    ck.eq("P06", "GET /api/schema?preset=<BEV> -> HTTP 200", bev.status, 200,
          note=bev.error or "")
    bdata = bev.json_or_none() or {}
    bpaths = [p.get("path", "") for p in bdata.get("parameters") or []]

    ck.eq("P07", "BEV schema echoes preset", bdata.get("preset"), common.TESLA)
    ck.check("P08", "BEV parameters use the concrete pt_type.BEV. prefix",
             any(p.startswith("pt_type.BEV.") for p in bpaths),
             sum(1 for p in bpaths if p.startswith("pt_type.BEV.")), "> 0")
    ck.check("P09", "BEV schema exposes res + em paths",
             any(".res." in p for p in bpaths) and any(".em." in p for p in bpaths),
             {"res": sum(1 for p in bpaths if ".res." in p),
              "em": sum(1 for p in bpaths if ".em." in p)}, "both > 0")
    ck.check("P10", "BEV schema has NO fc paths",
             not [p for p in bpaths if ".fc." in p],
             [p for p in bpaths if ".fc." in p][:3] or "none", "[]")
    ck.check("P11", "Conv and BEV parameter counts differ", len(cpaths) != len(bpaths),
             f"Conv={len(cpaths)}, BEV={len(bpaths)}", "different")
    ck.meta["conv_param_count"] = len(cpaths)
    ck.meta["bev_param_count"] = len(bpaths)
    return len(cpaths), len(bpaths)


def section_conv(ck: Checker) -> dict:
    payload = {
        "preset": common.FUSION,
        "cycle": common.UDDS,
        "overrides": {},
        "render_matplotlib": True,
    }
    resp = common.http_post_json("/api/simulate", payload)
    ck.eq("C01", "POST /api/simulate (Fusion + UDDS) -> HTTP 200", resp.status, 200,
          note=resp.error or "")
    data = resp.json_or_none() or {}
    if resp.status != 200 or not data:
        ck.check("C02", "Conv simulate returned a JSON body", False,
                 resp.text[:300], "JSON object")
        return {}
    ck.meta["conv_render_ms"] = round(resp.elapsed_ms, 1)

    kpis = data.get("kpis") or {}
    ck.check("C02", "kpis is a non-empty object", isinstance(kpis, dict) and bool(kpis),
             sorted(kpis.keys()), "non-empty")

    incomplete = [key for key, entry in kpis.items()
                  if not isinstance(entry, dict)
                  or not {"value", "unit", "label_en", "label_zh"} <= set(entry)]
    ck.check("C03", "every KPI entry has value/unit/label_en/label_zh",
             not incomplete, incomplete or f"{len(kpis)} KPIs complete", "[]")

    ck.approx("C04", "distance_mi ≈ 7.451 (±1%)",
              (kpis.get("distance_mi") or {}).get("value"), 7.451, 1.0)
    ck.approx("C05", "fuel_consumed_kwh ≈ 7.303 (±2%)",
              (kpis.get("fuel_consumed_kwh") or {}).get("value"), 7.303, 2.0)
    ck.approx("C06", "fuel_economy_mpgge ≈ 34.38 (±2%)",
              (kpis.get("fuel_economy_mpgge") or {}).get("value"), 34.38, 2.0)
    ck.approx("C07", "trace_met_pct ≈ 100 (±0.5)",
              (kpis.get("trace_met_pct") or {}).get("value"), 100.0, 0.5)
    ck.eq("C08", "distance_mi unit == 'mi'",
          (kpis.get("distance_mi") or {}).get("unit"), "mi")
    ck.eq("C09", "fuel_economy_mpgge unit == 'mpgge'",
          (kpis.get("fuel_economy_mpgge") or {}).get("unit"), "mpgge")

    soc_keys = sorted(k for k in kpis if k.startswith("soc_"))
    ck.check("C10", "Conv exposes NO soc_* KPI", not soc_keys, soc_keys or "none", "[]")

    ts = data.get("timeseries") or {}
    ck.check("C11", "timeseries has time_s / speed_target_mps / speed_achieved_mps",
             {"time_s", "speed_target_mps", "speed_achieved_mps"} <= set(ts),
             sorted(ts.keys()), "3 mandatory keys")
    lengths = {key: len(value) for key, value in ts.items() if isinstance(value, list)}
    equal = len(set(lengths.values())) == 1
    ck.check("C12", "all timeseries arrays are the same length", equal, lengths,
             "one shared length")
    ck.eq("C13", "timeseries length == 1370 (UDDS steps incl. initial state)",
          lengths.get("time_s"), 1370)

    ok, nbytes, blob = _png_ok(data)
    ck.check("C14", "matplotlib_png_base64 decodes to a PNG (magic \\x89PNG)", ok,
             f"{nbytes} bytes, magic={blob[:4]!r}", "PNG magic")
    ck.meta["conv_png_bytes"] = nbytes

    zh_pairs = [(kpis[k].get("label_en"), kpis[k].get("label_zh")) for k in kpis
                if isinstance(kpis[k], dict)]
    ck.check("C15", "KPI labels ship both EN and ZH text",
             all(en and zh for en, zh in zh_pairs)
             and any(en != zh for en, zh in zh_pairs),
             zh_pairs[:3], "distinct EN/ZH labels")

    ck.meta["conv_kpis"] = {k: v.get("value") for k, v in kpis.items()
                            if isinstance(v, dict)}
    ck.meta["conv_ts_lengths"] = lengths
    return data


def section_bev(ck: Checker) -> dict:
    payload = {
        "preset": common.TESLA,
        "cycle": common.UDDS,
        "overrides": {},
        "render_matplotlib": True,
    }
    resp = common.http_post_json("/api/simulate", payload)
    ck.eq("B01", "POST /api/simulate (Tesla BEV + UDDS) -> HTTP 200", resp.status, 200,
          note=resp.error or "")
    data = resp.json_or_none() or {}
    if resp.status != 200 or not data:
        ck.check("B02", "BEV simulate returned a JSON body", False,
                 resp.text[:300], "JSON object")
        return {}
    kpis = data.get("kpis") or {}
    ts = data.get("timeseries") or {}

    ck.close("B02", "soc_start_pct ≈ 98 (±1)", (kpis.get("soc_start_pct") or {}).get("value"),
             98.0, 1.0)
    ck.close("B03", "soc_end_pct ≈ 95.9 (±0.5)", (kpis.get("soc_end_pct") or {}).get("value"),
             95.9, 0.5)
    ck.approx("B04", "electric_economy_wh_per_mi ≈ 146.7 (±2%)",
              (kpis.get("electric_economy_wh_per_mi") or {}).get("value"), 146.7, 2.0)
    ck.check("B05", "BEV exposes NO fuel_economy_mpgge", "fuel_economy_mpgge" not in kpis,
             "fuel_economy_mpgge" if "fuel_economy_mpgge" in kpis else "absent", "absent")
    ck.check("B06", "BEV exposes NO fuel_consumed_kwh", "fuel_consumed_kwh" not in kpis,
             "fuel_consumed_kwh" if "fuel_consumed_kwh" in kpis else "absent", "absent")
    ck.check("B07", "BEV timeseries includes soc_pct", "soc_pct" in ts,
             sorted(ts.keys()), "soc_pct present")
    ck.check("B08", "BEV timeseries has NO fuel_power_kw", "fuel_power_kw" not in ts,
             "fuel_power_kw" if "fuel_power_kw" in ts else "absent", "absent")
    lengths = {k: len(v) for k, v in ts.items() if isinstance(v, list)}
    ck.check("B09", "BEV timeseries arrays are all equal length",
             len(set(lengths.values())) == 1, lengths, "one shared length")

    ok, nbytes, _ = _png_ok(data)
    ck.check("B10", "BEV matplotlib_png_base64 decodes to a PNG", ok, f"{nbytes} bytes",
             "PNG magic")

    ck.meta["bev_kpis"] = {k: v.get("value") for k, v in kpis.items()
                           if isinstance(v, dict)}
    ck.meta["bev_ts_keys"] = sorted(ts.keys())
    return data


def section_render_toggle(ck: Checker, conv_data: dict) -> dict:
    payload = {
        "preset": common.FUSION,
        "cycle": common.UDDS,
        "overrides": {},
        "render_matplotlib": False,
    }
    timings = []
    data = {}
    for _ in range(2):
        resp = common.http_post_json("/api/simulate", payload)
        if resp.status == 200:
            timings.append(resp.elapsed_ms)
            data = resp.json_or_none() or {}
    ck.eq("T01", "POST /api/simulate render_matplotlib:false -> HTTP 200",
          common.http_post_json("/api/simulate", payload).status, 200)
    ck.check("T02", "render_matplotlib:false response carries NO PNG",
             "matplotlib_png_base64" not in data,
             "present" if "matplotlib_png_base64" in data else "absent", "absent")
    ck.check("T03", "render_matplotlib:false response carries NO SVG",
             "matplotlib_svg" not in data,
             "present" if "matplotlib_svg" in data else "absent", "absent")
    fast = min(timings) if timings else None
    slow = ck.meta.get("conv_render_ms")
    ck.meta["toggle_timing_ms"] = {"render_true": slow, "render_false_min": fast}
    faster = fast is not None and slow is not None and fast < slow
    ratio = (slow / fast) if (fast and slow) else None
    ck.check("T04", "render_matplotlib:false is faster than :true", faster,
             f"true={slow} ms, false={fast} ms, ratio={ratio:.2f}x" if ratio else "n/a",
             "false < true",
             severity="fail" if not faster else "fail",
             note="" if (ratio or 0) >= 1.15 else "ratio below 1.15x")
    if ratio is not None and ratio < 1.15:
        ck.add("T05", "speed-up magnitude >= 1.15x", False, f"{ratio:.2f}x", ">= 1.15x",
               severity="warn")
    else:
        ck.add("T05", "speed-up magnitude >= 1.15x", True,
               f"{ratio:.2f}x" if ratio else "n/a", ">= 1.15x")
    if conv_data:
        ck.check("T06", "KPIs identical with and without matplotlib",
                 (data.get("kpis") or {}) == (conv_data.get("kpis") or {}),
                 "identical" if data.get("kpis") == conv_data.get("kpis") else "differ",
                 "identical")
    return data


def section_overrides(ck: Checker) -> None:
    payload = {
        "preset": common.FUSION,
        "cycle": common.UDDS,
        "overrides": {"chassis.drag_coef": 0.30},
        "render_matplotlib": False,
    }
    resp = common.http_post_json("/api/simulate", payload)
    ck.eq("O01", "override chassis.drag_coef=0.30 -> HTTP 200", resp.status, 200,
          note=resp.error or "")
    data = resp.json_or_none() or {}
    fe = (data.get("kpis") or {}).get("fuel_economy_mpgge", {}).get("value")
    ck.approx("O02", "fuel_economy_mpgge moves to ≈35.372", fe, 35.372, 0.5)
    base_fe = (ck.meta.get("conv_kpis") or {}).get("fuel_economy_mpgge")
    ck.check("O03", "override actually changes fuel economy",
             fe is not None and base_fe is not None and abs(fe - base_fe) > 1e-6,
             f"base={base_fe}, overridden={fe}", "different")
    ck.meta["override"] = {"path": "chassis.drag_coef", "value": 0.30,
                           "base_fuel_economy": base_fe, "new_fuel_economy": fe}

    bad = common.http_post_json("/api/simulate", {
        "preset": common.FUSION, "cycle": common.UDDS,
        "overrides": {"chassis.not_a_real_field": 1.0},
        "render_matplotlib": False,
    })
    ck.eq("O04", "unknown override path -> HTTP 400", bad.status, 400,
          note=bad.error or "")
    bad_json = bad.json_or_none() or {}
    ck.eq("O05", "unknown override error string", bad_json.get("error"),
          "Unknown parameter path")
    ck.check("O06", "unknown-path 400 body is JSON",
             "application/json" in bad.content_type, bad.content_type,
             "application/json*")

    unknown_preset = common.http_post_json("/api/simulate", {
        "preset": "no_such_vehicle.yaml", "cycle": common.UDDS,
        "render_matplotlib": False,
    })
    ck.check("O07", "unknown preset -> HTTP 4xx",
             unknown_preset.status in (400, 404), unknown_preset.status,
             "400 or 404")


def section_lang(ck: Checker, en_data: dict) -> None:
    payload = {
        "preset": common.FUSION,
        "cycle": common.UDDS,
        "overrides": {},
        "render_matplotlib": True,
        "lang": "zh",
    }
    resp = common.http_post_json("/api/simulate", payload)
    ck.eq("L01", "POST /api/simulate lang:'zh' -> HTTP 200", resp.status, 200,
          note=resp.error or "")
    zh = resp.json_or_none() or {}
    ck.eq("L02", "response echoes lang:'zh'", zh.get("lang"), "zh")
    ck.check("L03", "kpis are byte-identical between zh and en",
             (zh.get("kpis") or {}) == (en_data.get("kpis") or {}),
             "identical" if zh.get("kpis") == en_data.get("kpis") else "DIFFER",
             "identical")
    ck.check("L04", "timeseries are byte-identical between zh and en",
             (zh.get("timeseries") or {}) == (en_data.get("timeseries") or {}),
             "identical" if zh.get("timeseries") == en_data.get("timeseries")
             else "DIFFER", "identical")
    en_ok, en_n, en_blob = _png_ok(en_data)
    zh_ok, zh_n, zh_blob = _png_ok(zh)
    ck.check("L05", "zh PNG is still a valid PNG", zh_ok, f"{zh_n} bytes", "PNG magic")
    ck.check("L06", "zh PNG bytes differ from en (translated figure text)",
             en_ok and zh_ok and en_blob != zh_blob,
             f"en={en_n} B, zh={zh_n} B, identical={en_blob == zh_blob}", "different")
    ck.meta["lang_png"] = {"en_bytes": en_n, "zh_bytes": zh_n}

    en_resp = common.http_post_json("/api/simulate", {
        "preset": common.FUSION, "cycle": common.UDDS, "overrides": {},
        "render_matplotlib": False, "lang": "en",
    })
    ck.eq("L07", "explicit lang:'en' echoes 'en'",
          (en_resp.json_or_none() or {}).get("lang"), "en")


def main() -> int:
    ck = Checker("api", "Task 1 — Endpoint contract")
    base, provenance = common.ensure_server()
    ck.meta["base_url"] = base
    ck.meta["server_provenance"] = provenance
    print(f"[info] verifying against {base} ({provenance})", flush=True)

    section_schema(ck)
    section_schema_presets(ck)
    conv = section_conv(ck)
    section_bev(ck)
    section_render_toggle(ck, conv)
    section_overrides(ck)
    section_lang(ck, conv)
    return ck.finish()


if __name__ == "__main__":
    common.dispatch(main)
