r"""
e2e_frontend.py -- Task 4: frontend static sanity (no browser automation).

Verifies JS syntax with ``node --check``, that index.html references only
same-origin or ``data:`` assets (no CDN, no build step), that the favicon is
inlined as a data URI, that styles.css guards ``[hidden]`` with
``display: none``, and that app.js sends ``lang`` in the simulate payload.

Run standalone:  & <venv>\Scripts\python.exe gui\verify\e2e_frontend.py
Exit code: 0 = every check passed, 1 = at least one check failed.
"""

from __future__ import annotations

import re
import shutil
import subprocess
import sys
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parent))

import common
from common import Checker

JS_FILES = ("app.js", "charts.js", "i18n.js")
ATTR_RE = re.compile(
    r"""<(script|link|img|iframe|source|video|audio|embed)\b[^>]*?"""
    r"""\b(src|href)\s*=\s*(?:"([^"]*)"|'([^']*)')""",
    re.IGNORECASE | re.DOTALL,
)


def _node() -> str | None:
    return shutil.which("node") or shutil.which("node.exe")


def section_js_syntax(ck: Checker) -> None:
    node = _node()
    ck.check("F01", "node is available on PATH", bool(node), node or "missing",
             "a node binary")
    if not node:
        return
    version = subprocess.run([node, "--version"], capture_output=True, text=True)
    ck.meta["node_version"] = (version.stdout or "").strip()
    for name in JS_FILES:
        target = common.FRONTEND_DIR / name
        ck.check(f"F02-{name}", f"{name} exists", target.is_file(),
                 f"{target.stat().st_size} bytes" if target.is_file() else "missing",
                 "present")
        if not target.is_file():
            continue
        run = subprocess.run([node, "--check", str(target)], capture_output=True,
                             text=True, encoding="utf-8", errors="replace")
        ck.eq(f"F03-{name}", f"node --check {name} exits 0", run.returncode, 0,
              note=(run.stderr or "").strip().replace("\n", " | ")[:300])


def section_no_cdn(ck: Checker) -> None:
    index = common.FRONTEND_DIR / "index.html"
    html = index.read_text(encoding="utf-8", errors="replace")

    urls = []
    for match in ATTR_RE.finditer(html):
        tag = match.group(1).lower()
        attr = match.group(2).lower()
        value = (match.group(3) if match.group(3) is not None else match.group(4)) or ""
        urls.append((tag, attr, value.strip()))

    ck.meta["asset_refs"] = urls
    external = [u for u in urls if u[2].lower().startswith(("http://", "https://", "//"))]
    ck.check("F10", "no script/link/img URL uses http(s):// or // (no CDN)",
             not external, external or [u[2] for u in urls], "[]")

    # Ignore string literals inside inline SVG/data URIs; scan for real network
    # references only: an http(s):// that is NOT inside a data: URI.
    stripped = html
    for _tag, _attr, value in urls:
        if value.lower().startswith("data:"):
            stripped = stripped.replace(value, "")
    residual = re.findall(r"https?://[^\s\"'<>]+", stripped)
    ck.check("F11", "no http(s):// remains outside data: URIs in index.html",
             not residual, residual or "none", "[]")

    ck.check("F12", "index.html references /vendor/plotly.min.js",
             "/vendor/plotly.min.js" in html, "/vendor/plotly.min.js" in html, True)
    scripts = [u[2] for u in urls if u[0] == "script"]
    ck.check("F13", "every <script src> is a same-origin absolute path",
             bool(scripts) and all(s.startswith("/") and not s.startswith("//")
                                   for s in scripts), scripts,
             "all start with '/'")

    icon = re.search(r"""<link[^>]*rel\s*=\s*["']icon["'][^>]*>""", html, re.IGNORECASE)
    ck.check("F14", "index.html has a rel=\"icon\" link", bool(icon),
             icon.group(0)[:90] if icon else "missing", "present")
    ck.check("F15", "favicon is an inlined data: URI (no /favicon.ico round-trip)",
             bool(icon) and "data:" in icon.group(0),
             (icon.group(0)[:60] + "...") if icon else "missing", "data:image/...")

    css_links = [u[2] for u in urls
                 if u[0] == "link" and u[1] == "href"
                 and not u[2].lower().startswith("data:")]
    ck.check("F16", "index.html loads /styles.css", "/styles.css" in css_links,
             css_links, "['/styles.css']")

    for name in JS_FILES:
        ck.check(f"F17-{name}", f"index.html loads /{name}", f"/{name}" in html,
                 f"/{name}" in html, True)

    ck.check("F18", "index.html declares an EN/ZH language switch",
             'data-lang="en"' in html and 'data-lang="zh"' in html,
             {"en": 'data-lang="en"' in html, "zh": 'data-lang="zh"' in html}, True)


def section_css(ck: Checker) -> None:
    css = (common.FRONTEND_DIR / "styles.css").read_text(encoding="utf-8",
                                                         errors="replace")
    match = re.search(r"(?m)^\s*\[hidden\]\s*\{([^}]*)\}", css)
    ck.check("F20", "styles.css has a bare [hidden] rule", bool(match),
             match.group(0)[:80] if match else "missing", "[hidden] { ... }")
    ok = bool(match) and re.search(r"display\s*:\s*none", match.group(1))
    ck.check("F21", "[hidden] rule sets display: none", bool(ok),
             match.group(1).strip() if match else "n/a", "display: none")

    # The class-vs-attribute display bug: a `.view-pane { display: flex }` base
    # rule outranks the bare `[hidden]` attribute selector unless [hidden] wins
    # by !important or a `.view-pane[hidden]` rule re-asserts display: none.
    rules = re.findall(r"(?m)^\s*\.view-pane\s*\{([^}]*)\}", css)
    pane_hidden = re.search(r"(?m)^\s*\.view-pane\[hidden\]\s*\{[^}]*display\s*:\s*none",
                            css)
    bare_important = bool(match) and "!important" in match.group(1)
    ck.check("F22", ".view-pane hidden state cannot be defeated by the base rule",
             bool(rules) and (bool(pane_hidden) or bare_important),
             {"base_display": (re.search(r"display\s*:\s*([^;]+)", rules[0]).group(1).strip()
                               if rules and re.search(r"display\s*:\s*([^;]+)", rules[0])
                               else "no display"),
              ".view-pane[hidden] rule": bool(pane_hidden),
              "[hidden] !important": bare_important}, "hidden guard present")

    ck.check("F23", "styles.css defines the KPI card surface",
             ".kpi-row" in css and ".kpi-card" in css,
             {".kpi-row": ".kpi-row" in css, ".kpi-card": ".kpi-card" in css},
             {"kpi-row": True, "kpi-card": True})


def section_app_js(ck: Checker) -> None:
    app = (common.FRONTEND_DIR / "app.js").read_text(encoding="utf-8", errors="replace")

    ck.check("F30", "app.js posts to /api/simulate", "/api/simulate" in app,
             "/api/simulate" in app, True)
    ck.check("F31", "app.js posts to /api/export-python",
             "/api/export-python" in app, "/api/export-python" in app, True)
    ck.check("F32", "app.js fetches /api/schema", "/api/schema" in app,
             "/api/schema" in app, True)

    lang_fields = re.findall(r"lang\s*:\s*([A-Za-z_$][\w.$]*)", app)
    ck.check("F33", "app.js sends 'lang' in the JSON payloads",
             bool(lang_fields), lang_fields, "['I18N.lang', ...]")
    ck.check("F34", "the lang value comes from the i18n state (I18N.lang)",
             any(value == "I18N.lang" for value in lang_fields),
             sorted(set(lang_fields)), "I18N.lang")

    # The simulate payload must carry preset, cycle, overrides, render_matplotlib
    # and lang; every payload object literal that renders a figure is inspected.
    blocks = re.findall(r"var payload = \{(.*?)\};", app, re.DOTALL)
    simulate_payloads = [b for b in blocks if "render_matplotlib" in b]
    required = ("preset:", "cycle:", "overrides:", "render_matplotlib:", "lang:")
    complete = [b for b in simulate_payloads
                if all(key in b for key in required)]
    ck.check("F35", "simulate payload carries preset/cycle/overrides/"
                    "render_matplotlib/lang", bool(complete),
             {"simulate_payloads": len(simulate_payloads),
              "complete_payloads": len(complete),
              "keys_in_first": [k.rstrip(":") for k in required
                                if simulate_payloads and k in simulate_payloads[0]]},
             "at least one complete payload")

    ck.check("F36", "no external chart library is loaded at runtime",
             "cdn" not in app.lower() and "unpkg" not in app.lower()
             and "jsdelivr" not in app.lower(), "no CDN reference", True)
    ck.check("F37", "app.js renders the base64 matplotlib PNG",
             "matplotlib_png_base64" in app, "matplotlib_png_base64" in app, True)
    ck.check("F38", "app.js surfaces backend errors (error banner)",
             "error" in app.lower() and ("banner" in app.lower() or "detail" in app),
             {"error": "error" in app.lower()}, True)

    charts = (common.FRONTEND_DIR / "charts.js").read_text(encoding="utf-8",
                                                          errors="replace")
    ck.check("F39", "charts.js drives Plotly", "Plotly" in charts,
             "Plotly" in charts, True)

    i18n = (common.FRONTEND_DIR / "i18n.js").read_text(encoding="utf-8",
                                                      errors="replace")
    ck.check("F40", "i18n.js defines an EN and a ZH dictionary",
             bool(re.search(r"\ben\b", i18n)) and ("zh" in i18n) and "中文" in i18n,
             {"zh_strings": "中文" in i18n}, True)
    ck.check("F41", "i18n.js defaults to English",
             bool(re.search(r"""(default(Lang)?|LANG|lang)\s*[:=]\s*['"]en['"]""", i18n)),
             re.findall(r"""['"](?:en|zh)['"]""", i18n)[:6], "an 'en' default")


def main() -> int:
    ck = Checker("frontend", "Task 4 — Frontend static sanity")
    ck.meta["frontend_dir"] = str(common.FRONTEND_DIR)
    section_js_syntax(ck)
    section_no_cdn(ck)
    section_css(ck)
    section_app_js(ck)
    return ck.finish()


if __name__ == "__main__":
    common.dispatch(main)
