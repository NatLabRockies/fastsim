r"""
e2e_static.py -- Task 3: static serving + security.

Verifies that the one server serves the frontend at /, that the vendored
Plotly bundle is intact, that path traversal cannot escape gui/frontend (in
particular cannot read gui/CONTRACT.md), and that unknown API paths return
JSON 404.

Run standalone:  & <venv>\Scripts\python.exe gui\verify\e2e_static.py
Exit code: 0 = every check passed, 1 = at least one check failed.
"""

from __future__ import annotations

import sys
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parent))

import common
from common import Checker

TRAVERSAL_PATHS = [
    "/%2e%2e/CONTRACT.md",
    "/..%2fCONTRACT.md",
    "/vendor/../../CONTRACT.md",
    "/..%2F..%2FCONTRACT.md",
    "/%2e%2e%2f%2e%2e%2fCONTRACT.md",
]


def section_static(ck: Checker) -> None:
    root = common.raw_http_get("/")
    ck.eq("W01", "GET / -> HTTP 200", root.status, 200, note=root.error or "")
    ck.check("W02", "GET / content-type is text/html",
             "text/html" in root.content_type, root.content_type, "text/html*")
    body = root.text
    ck.check("W03", "GET / serves the real index.html (not the 503 placeholder)",
             "<!doctype html" in body.lower() and "frontend files are missing" not in body,
             f"{len(root.body)} bytes", "index.html")
    ck.check("W04", "index.html loads /vendor/plotly.min.js",
             "/vendor/plotly.min.js" in body, "/vendor/plotly.min.js" in body, True)

    css = common.raw_http_get("/styles.css")
    ck.eq("W05", "GET /styles.css -> HTTP 200", css.status, 200, note=css.error or "")
    ck.check("W06", "GET /styles.css content-type is text/css",
             "text/css" in css.content_type, css.content_type, "text/css*")
    ck.check("W07", "/styles.css body is non-trivial", len(css.body) > 5000,
             f"{len(css.body)} bytes", "> 5000")

    app_js = common.raw_http_get("/app.js")
    ck.eq("W08", "GET /app.js -> HTTP 200", app_js.status, 200, note=app_js.error or "")
    ck.check("W09", "GET /app.js content-type is javascript",
             "javascript" in app_js.content_type, app_js.content_type,
             "*javascript*")

    for name in ("/charts.js", "/i18n.js"):
        resp = common.raw_http_get(name)
        ck.check(f"W10{name}", f"GET {name} -> 200 javascript",
                 resp.status == 200 and "javascript" in resp.content_type,
                 f"{resp.status} {resp.content_type}", "200 + javascript")

    plotly = common.raw_http_get("/vendor/plotly.min.js")
    ck.eq("W11", "GET /vendor/plotly.min.js -> HTTP 200", plotly.status, 200,
          note=plotly.error or "")
    ck.check("W12", "vendored plotly.min.js is > 4 MB", len(plotly.body) > 4 * 1024 * 1024,
             f"{len(plotly.body)} bytes ({len(plotly.body) / 1048576:.2f} MB)", "> 4 MB")
    ck.check("W13", "vendored plotly looks like the Plotly bundle",
             b"plotly" in plotly.body[:4000].lower(), "found", "found")

    missing = common.raw_http_get("/definitely-not-here.js")
    ck.eq("W14", "unknown static file -> HTTP 404", missing.status, 404)

    head = common.raw_http_get("/")
    ck.check("W15", "static responses carry Content-Length",
             "content-length" in head.headers, head.headers.get("content-length"),
             "present")


def section_traversal(ck: Checker) -> None:
    # Positive control: the marker really is inside gui/CONTRACT.md on disk, so
    # "body has no marker" is a meaningful assertion rather than a vacuous one.
    contract = common.CONTRACT_MD
    disk_text = contract.read_text(encoding="utf-8", errors="replace") \
        if contract.is_file() else ""
    ck.check("W20", "control: gui/CONTRACT.md on disk contains the marker string",
             common.CONTRACT_MARKER in disk_text, common.CONTRACT_MARKER in disk_text,
             True)

    ck.check("W21", "control: /CONTRACT.md is NOT served at all",
             common.raw_http_get("/CONTRACT.md").status == 404,
             common.raw_http_get("/CONTRACT.md").status, 404,
             note="frontend root only")

    for index, path in enumerate(TRAVERSAL_PATHS, start=1):
        resp = common.raw_http_get(path)
        leaked = common.CONTRACT_MARKER in resp.text
        ck.check(f"W3{index}", f"traversal blocked: {path} -> 4xx",
                 400 <= resp.status < 500, resp.status, "4xx",
                 note=resp.error or "")
        ck.check(f"W4{index}", f"traversal leaked no contract text: {path}",
                 not leaked, "LEAKED" if leaked else "no leak", "no leak")


def section_api_errors(ck: Checker) -> None:
    resp = common.http_get("/api/does-not-exist")
    ck.eq("W50", "unknown API path -> HTTP 404", resp.status, 404, note=resp.error or "")
    ck.check("W51", "unknown API path 404 body is JSON",
             "application/json" in resp.content_type, resp.content_type,
             "application/json*")
    payload = resp.json_or_none()
    ck.check("W52", "unknown API 404 body carries an 'error' field",
             isinstance(payload, dict) and "error" in payload,
             payload if isinstance(payload, dict) else resp.text[:200],
             "{'error': ...}")

    bad_method = common.http_get("/api/simulate")
    ck.check("W53", "GET /api/simulate -> 405 (POST only)",
             bad_method.status == 405, bad_method.status, 405)

    bad_preset = common.http_get("/api/schema?preset=not-a-vehicle.yaml")
    ck.eq("W54", "unknown preset in /api/schema -> HTTP 404", bad_preset.status, 404)
    body = bad_preset.json_or_none()
    ck.check("W55", "unknown-preset 404 body is JSON with 'error'",
             isinstance(body, dict) and "error" in body, body, "{'error': ...}")

    bad_json = common.http_post_json("/api/simulate", None, raw_body="{not json")
    ck.eq("W56", "malformed JSON body -> HTTP 400", bad_json.status, 400)
    body = bad_json.json_or_none()
    ck.check("W57", "malformed JSON 400 body is JSON", isinstance(body, dict), body,
             "JSON object")


def main() -> int:
    ck = Checker("static", "Task 3 — Static serving + security")
    base, provenance = common.ensure_server()
    ck.meta["base_url"] = base
    print(f"[info] verifying against {base} ({provenance})", flush=True)
    section_static(ck)
    section_traversal(ck)
    section_api_errors(ck)
    return ck.finish()


if __name__ == "__main__":
    common.dispatch(main)
