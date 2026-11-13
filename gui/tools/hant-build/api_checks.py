"""API checks against the running server on :8021 (port from argv[1])."""

import base64
import json
import sys
import urllib.request

BASE = f"http://127.0.0.1:{sys.argv[1] if len(sys.argv) > 1 else 8021}"


def get(path):
    with urllib.request.urlopen(BASE + path, timeout=120) as r:
        return r.status, json.loads(r.read().decode("utf-8"))


def post(path, payload):
    body = json.dumps(payload).encode("utf-8")
    req = urllib.request.Request(BASE + path, data=body,
                                 headers={"Content-Type": "application/json"}, method="POST")
    with urllib.request.urlopen(req, timeout=300) as r:
        return r.status, json.loads(r.read().decode("utf-8"))


def main() -> int:
    ok = True
    status, schema = get("/api/schema")
    n = len(schema["parameters"])
    print(f"1. GET /api/schema -> {status}, {n} parameters, "
          f"{len(schema['presets'])} presets, {len(schema['cycles'])} cycles")
    ok &= status == 200 and n == 46

    conv = next(p["id"] for p in schema["presets"] if p["pt_type"] == "Conv")
    bev = next(p["id"] for p in schema["presets"] if p["pt_type"] == "BEV")
    print(f"   Conv preset: {conv}\n   BEV  preset: {bev}")

    results = {}
    for lang in ("en", "zh", "zh-Hant"):
        status, data = post("/api/simulate", {
            "preset": conv, "cycle": "udds.csv", "overrides": {},
            "render_matplotlib": True, "lang": lang,
        })
        png = base64.b64decode(data["matplotlib_png_base64"])
        results[lang] = data
        print(f"2. simulate lang={lang!r} -> HTTP {status}, echoed {data.get('lang')!r}, "
              f"{len(data['kpis'])} KPIs, PNG {len(png)} B, svg {len(data.get('matplotlib_svg') or '')} B")
        ok &= status == 200 and data.get("lang") == lang

    en, zh, hant = (results[k] for k in ("en", "zh", "zh-Hant"))
    same_nums_hant = en["kpis"] == hant["kpis"] and en["timeseries"] == hant["timeseries"]
    same_nums_zh = en["kpis"] == zh["kpis"] and en["timeseries"] == zh["timeseries"]
    print(f"3. KPIs+timeseries identical en vs zh-Hant: {same_nums_hant}; en vs zh: {same_nums_zh}")
    ok &= same_nums_hant and same_nums_zh

    en_png = base64.b64decode(en["matplotlib_png_base64"])
    zh_png = base64.b64decode(zh["matplotlib_png_base64"])
    hant_png = base64.b64decode(hant["matplotlib_png_base64"])
    print(f"4. PNG bytes differ: en!=zh {en_png != zh_png}, en!=zh-Hant {en_png != hant_png}, "
          f"zh!=zh-Hant {zh_png != hant_png}")
    ok &= en_png != hant_png and zh_png != hant_png

    # normalisation of alternate Traditional tags
    for tag in ("zh-TW", "zh-HK", "zh_Hant"):
        status, data = post("/api/simulate", {
            "preset": conv, "cycle": "udds.csv", "overrides": {},
            "render_matplotlib": False, "lang": tag,
        })
        print(f"5. lang={tag!r} -> echo {data.get('lang')!r} (HTTP {status})")
        ok &= data.get("lang") == "zh-Hant"
        ok &= data["kpis"] == en["kpis"]

    # Traditional figure really contains Traditional text: decode the SVG, which
    # carries the same strings as the PNG raster.
    for lang, must, mustnot in (("zh-Hant", ["\u8eca\u901f", "\u6642\u9593", "\u7d2f\u7a4d"],
                                            ["\u8f66\u901f", "\u65f6\u95f4", "\u7d2f\u79ef"]),
                                ("zh", ["\u8f66\u901f", "\u65f6\u95f4"],
                                       ["\u8eca\u901f", "\u6642\u9593"])):
        svg = (results[lang].get("matplotlib_svg") or "")
        have = [s for s in must if s in svg]
        have_not = [s for s in mustnot if s in svg]
        print(f"6. {lang} SVG: contains {[s for s in have]} ; forbidden present {have_not}")
        ok &= len(have) == len(must) and not have_not

    # a BEV run in Traditional must also work and keep numbers stable
    status, bev_hant = post("/api/simulate", {
        "preset": bev, "cycle": "hwfet.csv", "overrides": {},
        "render_matplotlib": True, "lang": "zh-Hant",
    })
    status_en, bev_en = post("/api/simulate", {
        "preset": bev, "cycle": "hwfet.csv", "overrides": {},
        "render_matplotlib": False, "lang": "en",
    })
    print(f"7. BEV lang=zh-Hant -> HTTP {status}, {len(bev_hant['kpis'])} KPIs; "
          f"numbers match en: {bev_hant['kpis'] == bev_en['kpis'] and bev_hant['timeseries'] == bev_en['timeseries']}")
    ok &= status == 200 and bev_hant["kpis"] == bev_en["kpis"]

    out = {"en": en["kpis"], "hant_kpis_keys": sorted(hant["kpis"].keys()),
           "hant_svg_len": len(hant.get("matplotlib_svg") or "")}
    with open("api-check.json", "w", encoding="utf-8") as fh:
        json.dump(out, fh, ensure_ascii=False, indent=1)

    print("\nRESULT:", "PASS" if ok else "FAIL")
    return 0 if ok else 1


if __name__ == "__main__":
    raise SystemExit(main())
