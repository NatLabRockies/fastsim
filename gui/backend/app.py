"""
app.py -- the FASTSim Web GUI backend.

The one and only entry point::

    python gui/backend/app.py

Serves (same origin, no CORS, no external dependencies beyond fastsim + matplotlib):

    GET  /                     -> gui/frontend/index.html (+ static assets)
    GET  /api/schema[?preset=] -> contract 1.1
    POST /api/simulate         -> contract 1.2
    POST /api/export-python    -> contract 1.3
"""

from __future__ import annotations

import json
import mimetypes
import os
import sys
import time
import traceback
from http import HTTPStatus
from http.server import BaseHTTPRequestHandler, ThreadingHTTPServer
from pathlib import Path
from urllib.parse import parse_qs, unquote, urlsplit

BACKEND_DIR = Path(__file__).resolve().parent
if str(BACKEND_DIR) not in sys.path:
    sys.path.insert(0, str(BACKEND_DIR))

GUI_DIR = BACKEND_DIR.parent
# Optional override (handy for testing the "frontend not ready" path); defaults
# to the sibling gui/frontend directory.
FRONTEND_DIR = Path(os.environ.get("FASTSIM_GUI_FRONTEND") or (GUI_DIR / "frontend")).resolve()
INDEX_FILE = FRONTEND_DIR / "index.html"

DEFAULT_HOST = os.environ.get("FASTSIM_GUI_HOST", "127.0.0.1")
DEFAULT_PORT = int(os.environ.get("FASTSIM_GUI_PORT", "8000"))
MAX_BODY_BYTES = 1 << 20  # 1 MiB is plenty for a handful of overrides

MIME_TYPES = {
    ".html": "text/html; charset=utf-8",
    ".htm": "text/html; charset=utf-8",
    ".css": "text/css; charset=utf-8",
    ".js": "text/javascript; charset=utf-8",
    ".mjs": "text/javascript; charset=utf-8",
    ".json": "application/json; charset=utf-8",
    ".map": "application/json; charset=utf-8",
    ".csv": "text/csv; charset=utf-8",
    ".txt": "text/plain; charset=utf-8",
    ".png": "image/png",
    ".jpg": "image/jpeg",
    ".jpeg": "image/jpeg",
    ".gif": "image/gif",
    ".svg": "image/svg+xml",
    ".ico": "image/x-icon",
    ".webp": "image/webp",
    ".woff": "font/woff",
    ".woff2": "font/woff2",
    ".ttf": "font/ttf",
}

PLACEHOLDER_HTML = """<!doctype html>
<html lang="en"><head><meta charset="utf-8">
<title>FASTSim GUI - frontend not ready</title>
<style>
  body {{ font: 15px/1.6 system-ui, sans-serif; margin: 0; padding: 48px;
         background: #0f1720; color: #e6edf3; }}
  code {{ background: #1b2733; padding: 2px 6px; border-radius: 4px; }}
  .box {{ max-width: 720px; margin: 0 auto; background: #16202b;
          border: 1px solid #24313f; border-radius: 12px; padding: 28px 32px; }}
  h1 {{ font-size: 20px; margin: 0 0 12px; }}
  ul {{ padding-left: 20px; }} a {{ color: #6fb2ff; }}
</style></head>
<body><div class="box">
<h1>Backend is running &mdash; frontend files are missing</h1>
<p>The API is fully available, but <code>{index_path}</code> does not exist yet.</p>
<ul>
  <li><code>GET /api/schema</code> &mdash; parameter / preset / cycle schema</li>
  <li><code>POST /api/simulate</code> &mdash; run a simulation</li>
  <li><code>POST /api/export-python</code> &mdash; export runnable Python</li>
</ul>
<p>Once <code>gui/frontend/index.html</code> exists, reload this page.</p>
</div></body></html>
"""


def log(message: str) -> None:
    print(f"[fastsim-gui] {message}", file=sys.stderr, flush=True)


class ApiError(Exception):
    """Client-facing error with a contract-shaped JSON body."""

    def __init__(self, status: int, error: str, detail: str):
        super().__init__(detail)
        self.status = status
        self.error = error
        self.detail = detail


class FastSimHandler(BaseHTTPRequestHandler):
    server_version = "FASTSimGUI/1.0"
    protocol_version = "HTTP/1.1"

    # ---------------------------------------------------------------- utils
    def log_message(self, fmt: str, *args) -> None:  # noqa: A003 - stdlib signature
        log("%s - %s" % (self.address_string(), fmt % args))

    def _send(self, status: int, body: bytes, content_type: str, *,
              extra_headers: dict[str, str] | None = None, send_body: bool = True) -> None:
        self.send_response(status)
        self.send_header("Content-Type", content_type)
        self.send_header("Content-Length", str(len(body)))
        if extra_headers:
            for key, value in extra_headers.items():
                self.send_header(key, value)
        self.end_headers()
        if send_body and body:
            try:
                self.wfile.write(body)
            except (BrokenPipeError, ConnectionResetError):  # client hung up
                pass

    def _send_json(self, payload, status: int = 200, *, send_body: bool = True) -> None:
        body = json.dumps(payload, ensure_ascii=False, allow_nan=False).encode("utf-8")
        self._send(status, body, "application/json; charset=utf-8",
                   extra_headers={"Cache-Control": "no-store"}, send_body=send_body)

    def _send_error_json(self, status: int, error: str, detail: str, *,
                         send_body: bool = True) -> None:
        self._send_json({"error": error, "detail": detail}, status, send_body=send_body)

    def _send_html(self, status: int, html: str, *, send_body: bool = True) -> None:
        self._send(status, html.encode("utf-8"), "text/html; charset=utf-8",
                   extra_headers={"Cache-Control": "no-store"}, send_body=send_body)

    def _read_json_body(self) -> dict:
        raw_length = self.headers.get("Content-Length")
        if raw_length is None:
            return {}
        try:
            length = int(raw_length)
        except ValueError as exc:
            raise ApiError(400, "Invalid request", "Content-Length is not an integer") from exc
        if length <= 0:
            return {}
        if length > MAX_BODY_BYTES:
            raise ApiError(413, "Request too large",
                           f"body of {length} bytes exceeds {MAX_BODY_BYTES} bytes")
        raw = self.rfile.read(length)
        try:
            text = raw.decode("utf-8")
        except UnicodeDecodeError as exc:
            raise ApiError(400, "Invalid request", "body is not valid UTF-8") from exc
        if not text.strip():
            return {}
        try:
            parsed = json.loads(text)
        except json.JSONDecodeError as exc:
            raise ApiError(400, "Invalid JSON", f"{exc.msg} at line {exc.lineno} column {exc.colno}") from exc
        if not isinstance(parsed, dict):
            raise ApiError(400, "Invalid request", "request body must be a JSON object")
        return parsed

    # ------------------------------------------------------------ static files
    def _resolve_static(self, url_path: str) -> Path:
        relative = unquote(url_path).lstrip("/")
        if not relative or relative.endswith("/"):
            relative = f"{relative}index.html"
        if "\x00" in relative:
            raise ApiError(400, "Invalid path", "path contains a NUL byte")
        root = FRONTEND_DIR.resolve()
        candidate = (root / relative).resolve()
        if candidate != root and not candidate.is_relative_to(root):
            raise ApiError(403, "Forbidden", "path escapes the frontend directory")
        return candidate

    def _serve_static(self, url_path: str, *, send_body: bool = True) -> None:
        try:
            target = self._resolve_static(url_path)
        except ApiError as exc:
            self._send_error_json(exc.status, exc.error, exc.detail, send_body=send_body)
            return

        is_root_request = url_path in ("/", "") or url_path.endswith("/")
        if not target.is_file():
            if is_root_request or target == INDEX_FILE:
                self._send_html(503, PLACEHOLDER_HTML.format(index_path=INDEX_FILE),
                                send_body=send_body)
                return
            self._send_html(404, "<!doctype html><html><head><meta charset='utf-8'>"
                                 "<title>404</title></head><body><h1>404 Not Found</h1>"
                                 f"<p>{url_path}</p></body></html>", send_body=send_body)
            return

        suffix = target.suffix.lower()
        content_type = MIME_TYPES.get(suffix) or mimetypes.guess_type(str(target))[0] \
            or "application/octet-stream"
        try:
            body = target.read_bytes()
        except OSError as exc:
            self._send_error_json(500, "Cannot read file", str(exc), send_body=send_body)
            return
        self._send(200, body, content_type,
                   extra_headers={"Cache-Control": "no-store"}, send_body=send_body)

    # ------------------------------------------------------------------ verbs
    def do_OPTIONS(self) -> None:  # noqa: N802 - stdlib naming
        self._send(204, b"", "text/plain; charset=utf-8", extra_headers={
            "Allow": "GET, HEAD, POST, OPTIONS",
            "Access-Control-Allow-Origin": "*",
            "Access-Control-Allow-Methods": "GET, POST, OPTIONS",
            "Access-Control-Allow-Headers": "Content-Type",
            "Access-Control-Max-Age": "600",
            "Cache-Control": "no-store",
        })

    def do_HEAD(self) -> None:  # noqa: N802 - stdlib naming
        self._dispatch(send_body=False)

    def do_GET(self) -> None:  # noqa: N802 - stdlib naming
        self._dispatch()

    def do_POST(self) -> None:  # noqa: N802 - stdlib naming
        self._dispatch()

    # --------------------------------------------------------------- dispatch
    def _dispatch(self, *, send_body: bool = True) -> None:
        started = time.perf_counter()
        parts = urlsplit(self.path)
        path = parts.path
        method = self.command.upper()
        status_holder = {"status": 200}
        try:
            status_holder["status"] = self._route(method, path, parse_qs(parts.query),
                                                  send_body=send_body)
        except ApiError as exc:
            status_holder["status"] = exc.status
            self._send_error_json(exc.status, exc.error, exc.detail, send_body=send_body)
        except BrokenPipeError:
            status_holder["status"] = 499
            return
        except Exception as exc:  # never leak a traceback to the client
            status_holder["status"] = 500
            log(f"unhandled error on {method} {path}: {type(exc).__name__}: {exc}")
            traceback.print_exc()
            self._send_error_json(500, "Internal server error",
                                  f"{type(exc).__name__}: {exc}"[:1000], send_body=send_body)
        finally:
            elapsed_ms = (time.perf_counter() - started) * 1000.0
            log(f"{method} {path} -> {status_holder['status']} ({elapsed_ms:.0f} ms)")

    def _route(self, method: str, path: str, query: dict[str, list[str]], *,
               send_body: bool) -> int:
        import export_code
        import introspection
        import simulate

        # ---- API -----------------------------------------------------
        if path == "/api/schema":
            if method not in ("GET", "HEAD"):
                raise ApiError(405, "Method not allowed", "use GET /api/schema")
            preset = (query.get("preset") or [None])[0]
            try:
                payload = introspection.build_schema(preset)
            except ValueError as exc:
                raise ApiError(404, "Unknown preset", str(exc)) from exc
            self._send_json(payload, send_body=send_body)
            return 200

        if path == "/api/simulate":
            if method != "POST":
                raise ApiError(405, "Method not allowed", "use POST /api/simulate")
            body = self._read_json_body()
            raw_lang = body.get("lang")
            if raw_lang is not None and not isinstance(raw_lang, str):
                raise ApiError(400, "Invalid request",
                               "`lang` must be a string (en | zh | zh-Hant)")
            try:
                payload = simulate.run_simulation(
                    preset=body.get("preset"),
                    cycle=body.get("cycle"),
                    overrides=body.get("overrides") or {},
                    render_matplotlib=bool(body.get("render_matplotlib", True)),
                    lang=raw_lang,
                )
            except simulate.SimulationError as exc:
                raise ApiError(exc.status, exc.error, exc.detail) from exc
            except ValueError as exc:
                raise ApiError(400, "Invalid simulation request", str(exc)) from exc
            self._send_json(payload, send_body=send_body)
            return 200

        if path == "/api/export-python":
            if method != "POST":
                raise ApiError(405, "Method not allowed", "use POST /api/export-python")
            body = self._read_json_body()
            try:
                code = export_code.generate(
                    preset=body.get("preset"),
                    cycle=body.get("cycle"),
                    overrides=body.get("overrides") or {},
                )
            except simulate.SimulationError as exc:
                raise ApiError(exc.status, exc.error, exc.detail) from exc
            except ValueError as exc:
                raise ApiError(400, "Invalid export request", str(exc)) from exc
            self._send_json({"code": code}, send_body=send_body)
            return 200

        if path.startswith("/api/"):
            raise ApiError(404, "Not found", f"no API endpoint at {path}")

        # ---- static --------------------------------------------------
        if method not in ("GET", "HEAD"):
            raise ApiError(405, "Method not allowed", "use GET for static files")
        self._serve_static(path, send_body=send_body)
        return 200


class FastSimServer(ThreadingHTTPServer):
    daemon_threads = True
    allow_reuse_address = True


def _parse_argv(argv: list[str]) -> tuple[str, int]:
    host, port = DEFAULT_HOST, DEFAULT_PORT
    args = list(argv[1:])
    while args:
        token = args.pop(0)
        if token in ("--port", "-p") and args:
            port = int(args.pop(0))
        elif token.startswith("--port="):
            port = int(token.split("=", 1)[1])
        elif token in ("--host",) and args:
            host = args.pop(0)
        elif token.startswith("--host="):
            host = token.split("=", 1)[1]
        elif token in ("--help", "-h"):
            print("usage: python gui/backend/app.py [--host 127.0.0.1] [--port 8000]")
            raise SystemExit(0)
        else:
            raise SystemExit(f"unknown argument: {token}")
    return host, port


def main(argv: list[str] | None = None) -> int:
    host, port = _parse_argv(list(sys.argv if argv is None else argv))
    frontend_ready = INDEX_FILE.is_file()

    banner = [
        "",
        "  FASTSim Web GUI backend",
        "  " + "-" * 62,
        f"  URL          http://{host}:{port}",
        f"  Frontend     {FRONTEND_DIR}",
        f"  index.html   {'found' if frontend_ready else 'MISSING (503 page will be served)'}",
        "  Endpoints    GET  /api/schema[?preset=<id>]",
        "               POST /api/simulate",
        "               POST /api/export-python",
        "  The frontend is served from this same origin; no CORS, no build step.",
        "  Press Ctrl+C to stop.",
        "",
    ]
    print("\n".join(banner), flush=True)

    try:
        httpd = FastSimServer((host, port), FastSimHandler)
    except OSError as exc:
        log(f"cannot bind {host}:{port} -> {exc}")
        return 1

    try:
        httpd.serve_forever()
    except KeyboardInterrupt:
        print("", flush=True)
        log("shutting down")
    finally:
        httpd.server_close()
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
