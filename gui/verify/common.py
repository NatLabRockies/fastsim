"""
common.py -- shared helpers for the FASTSim Web GUI verification suite.

Owned by Subagent 3 (verification only).  Nothing here modifies the backend or
the frontend; every check talks to the running server over HTTP or reads the
frontend sources read-only.

Environment:
    FASTSIM_VERIFY_BASE_URL   e.g. http://127.0.0.1:8012  (set by run_all.py)
    FASTSIM_VERIFY_PORT       the port belonging to that base URL
"""

from __future__ import annotations

import atexit
import json
import os
import socket
import subprocess
import sys
import time
from pathlib import Path
from urllib import error as urlerror
from urllib import parse as urlparse
from urllib import request as urlrequest

# --------------------------------------------------------------------------
# Paths / constants
# --------------------------------------------------------------------------
VERIFY_DIR = Path(__file__).resolve().parent
GUI_DIR = VERIFY_DIR.parent
WORKSPACE = GUI_DIR.parent
FRONTEND_DIR = GUI_DIR / "frontend"
BACKEND_DIR = GUI_DIR / "backend"
BACKEND_APP = BACKEND_DIR / "app.py"
CONTRACT_MD = GUI_DIR / "CONTRACT.md"
VENV_PYTHON = WORKSPACE / "venv" / "Scripts" / "python.exe"
TMP_DIR = VERIFY_DIR / "_tmp"

HOST = "127.0.0.1"
PRIMARY_PORT = 8000
FALLBACK_PORT = 8012

UDDS = "udds.csv"
HWFET = "hwfet.csv"
FUSION = "2012_Ford_Fusion.yaml"
TESLA = "2022 Tesla Model 3 RWD thrml.yaml"

# The unique marker inside gui/CONTRACT.md; a traversal leak would expose it.
CONTRACT_MARKER = "LOCKED CONTRACT"


def utf8_stdout() -> None:
    for stream in (sys.stdout, sys.stderr):
        try:
            stream.reconfigure(encoding="utf-8", errors="replace")
        except Exception:
            pass


# --------------------------------------------------------------------------
# Minimal HTTP client (never raises for 4xx/5xx)
# --------------------------------------------------------------------------
class Response:
    __slots__ = ("status", "body", "headers", "elapsed_ms", "error", "url")

    def __init__(self, status, body, headers, elapsed_ms, url, error=None):
        self.status = status
        self.body = body or b""
        self.headers = {str(k).lower(): str(v) for k, v in (headers or {}).items()}
        self.elapsed_ms = elapsed_ms
        self.url = url
        self.error = error

    @property
    def text(self) -> str:
        return self.body.decode("utf-8", "replace")

    @property
    def content_type(self) -> str:
        return self.headers.get("content-type", "")

    def json(self):
        return json.loads(self.text)

    def json_or_none(self):
        try:
            return self.json()
        except Exception:
            return None

    def brief(self) -> str:
        if self.error:
            return f"transport error: {self.error}"
        return f"HTTP {self.status} ({len(self.body)} B)"


def _do(req: urlrequest.Request, timeout: float) -> Response:
    url = req.full_url
    opener = urlrequest.build_opener()
    started = time.perf_counter()
    try:
        with opener.open(req, timeout=timeout) as resp:
            body = resp.read()
            elapsed = (time.perf_counter() - started) * 1000.0
            return Response(resp.status, body, dict(resp.headers), elapsed, url)
    except urlerror.HTTPError as exc:
        try:
            body = exc.read()
        except Exception:
            body = b""
        elapsed = (time.perf_counter() - started) * 1000.0
        return Response(exc.code, body, dict(exc.headers or {}), elapsed, url)
    except Exception as exc:  # connection refused / timeout / DNS
        elapsed = (time.perf_counter() - started) * 1000.0
        return Response(0, b"", {}, elapsed, url, error=f"{type(exc).__name__}: {exc}")


def http_get(path: str, *, base: str | None = None, timeout: float = 120.0) -> Response:
    base = base or base_url()
    return _do(urlrequest.Request(base + path, method="GET"), timeout)


def http_post_json(path: str, payload: dict, *, base: str | None = None,
                   timeout: float = 600.0, raw_body: str | None = None) -> Response:
    base = base or base_url()
    data = (raw_body if raw_body is not None else json.dumps(payload)).encode("utf-8")
    req = urlrequest.Request(
        base + path, data=data, method="POST",
        headers={"Content-Type": "application/json; charset=utf-8"},
    )
    return _do(req, timeout)


def raw_http_get(path_literal: str, *, port: int | None = None,
                 timeout: float = 120.0) -> Response:
    """Send ``GET <path_literal>`` byte-for-byte (no urllib normalisation).

    Needed for path-traversal probes: urllib rewrites ``/../`` before it hits
    the wire, which would not exercise the server's own guard.
    """
    port = port or server_port()
    request_bytes = (
        f"GET {path_literal} HTTP/1.1\r\n"
        f"Host: {HOST}:{port}\r\n"
        "User-Agent: fastsim-verify/1.0\r\n"
        "Accept: */*\r\n"
        "Connection: close\r\n"
        "\r\n"
    ).encode("latin-1")
    started = time.perf_counter()
    try:
        chunks = []
        with socket.create_connection((HOST, port), timeout=timeout) as sock:
            sock.sendall(request_bytes)
            while True:
                block = sock.recv(1 << 16)
                if not block:
                    break
                chunks.append(block)
    except Exception as exc:
        elapsed = (time.perf_counter() - started) * 1000.0
        return Response(0, b"", {}, elapsed, path_literal,
                        error=f"{type(exc).__name__}: {exc}")
    elapsed = (time.perf_counter() - started) * 1000.0
    data = b"".join(chunks)
    head, _, body = data.partition(b"\r\n\r\n")
    lines = head.split(b"\r\n")
    status = 0
    if lines and lines[0].startswith(b"HTTP/"):
        try:
            status = int(lines[0].split(b" ")[1])
        except (IndexError, ValueError):
            status = 0
    headers = {}
    for line in lines[1:]:
        key, sep, value = line.partition(b":")
        if sep:
            headers[key.decode("latin-1").strip()] = value.decode("latin-1").strip()
    if "chunked" in headers.get("Transfer-Encoding", "").lower():
        body = _dechunk(body)
    return Response(status, body, headers, elapsed, path_literal)


def _dechunk(payload: bytes) -> bytes:
    out = bytearray()
    rest = payload
    while True:
        line, sep, rest = rest.partition(b"\r\n")
        if not sep:
            break
        try:
            size = int(line.split(b";")[0].strip() or b"0", 16)
        except ValueError:
            break
        if size == 0:
            break
        out += rest[:size]
        rest = rest[size + 2:]
    return bytes(out)


# --------------------------------------------------------------------------
# Server discovery / lifecycle
# --------------------------------------------------------------------------
_STATE: dict[str, object] = {"base": None, "port": None, "proc": None, "log": None}


def base_url() -> str:
    if _STATE["base"]:
        return str(_STATE["base"])
    env = os.environ.get("FASTSIM_VERIFY_BASE_URL")
    if env:
        _STATE["base"] = env.rstrip("/")
        _STATE["port"] = int(os.environ.get("FASTSIM_VERIFY_PORT")
                             or env.rsplit(":", 1)[-1])
        return str(_STATE["base"])
    ensure_server()
    return str(_STATE["base"])


def server_port() -> int:
    base_url()
    return int(_STATE["port"] or PRIMARY_PORT)


def _get_base(port: int, timeout: float = 8.0) -> Response:
    return _do(urlrequest.Request(f"http://{HOST}:{port}/api/schema", method="GET"), timeout)


def ensure_server() -> tuple[str, str]:
    """Return (base_url, provenance). Starts a private server only if needed."""
    if _STATE["base"]:
        return str(_STATE["base"]), "already attached"
    env = os.environ.get("FASTSIM_VERIFY_BASE_URL")
    if env:
        _STATE["base"] = env.rstrip("/")
        _STATE["port"] = int(os.environ.get("FASTSIM_VERIFY_PORT")
                             or env.rsplit(":", 1)[-1])
        return str(_STATE["base"]), "FASTSIM_VERIFY_BASE_URL"

    probe = _get_base(PRIMARY_PORT, timeout=6.0)
    if probe.status == 200:
        _STATE["base"] = f"http://{HOST}:{PRIMARY_PORT}"
        _STATE["port"] = PRIMARY_PORT
        return str(_STATE["base"]), "pre-existing server"

    TMP_DIR.mkdir(parents=True, exist_ok=True)
    log_path = TMP_DIR / "server_verify.log"
    log_handle = open(log_path, "wb")
    proc = subprocess.Popen(
        [str(VENV_PYTHON), str(BACKEND_APP), "--port", str(FALLBACK_PORT)],
        cwd=str(WORKSPACE), stdout=log_handle, stderr=subprocess.STDOUT,
    )
    _STATE["proc"] = proc
    _STATE["log"] = log_handle
    atexit.register(stop_server)
    deadline = time.time() + 90.0
    while time.time() < deadline:
        if proc.poll() is not None:
            raise SystemExit(f"verification server exited early, see {log_path}")
        if _get_base(FALLBACK_PORT, timeout=3.0).status == 200:
            _STATE["base"] = f"http://{HOST}:{FALLBACK_PORT}"
            _STATE["port"] = FALLBACK_PORT
            return str(_STATE["base"]), "started by verify suite"
        time.sleep(0.4)
    raise SystemExit(f"verification server did not become ready, see {log_path}")


def stop_server() -> None:
    proc = _STATE.get("proc")
    if proc is not None and proc.poll() is None:
        proc.terminate()
        try:
            proc.wait(timeout=15)
        except subprocess.TimeoutExpired:
            proc.kill()
    log_handle = _STATE.get("log")
    if log_handle is not None:
        try:
            log_handle.close()
        except Exception:
            pass
    _STATE["proc"] = None
    _STATE["log"] = None


# --------------------------------------------------------------------------
# Result collection
# --------------------------------------------------------------------------
class Checker:
    """Records PASS/FAIL checks plus free-form meta data for the report."""

    def __init__(self, key: str, title: str):
        self.key = key
        self.title = title
        self.rows: list[dict] = []
        self.meta: dict = {}

    # -- recording ---------------------------------------------------------
    def add(self, check_id: str, description: str, ok: bool, observed,
            expected="", severity: str = "fail", note: str = "") -> bool:
        self.rows.append({
            "id": check_id,
            "description": description,
            "ok": bool(ok),
            "observed": _fmt(observed),
            "expected": _fmt(expected),
            "severity": severity,
            "note": note,
        })
        marker = "PASS" if ok else ("WARN" if severity == "warn" else "FAIL")
        print(f"[{marker}] {check_id}: {description} | observed={_fmt(observed)}"
              + (f" | expected={_fmt(expected)}" if expected != "" else "")
              + (f" | {note}" if note else ""), flush=True)
        return bool(ok)

    def true(self, check_id: str, description: str, observed, note: str = "") -> bool:
        return self.add(check_id, description, True, observed, "True", note=note)

    def check(self, check_id: str, description: str, ok: bool, observed,
              expected=True, note: str = "", severity: str = "fail") -> bool:
        return self.add(check_id, description, ok, observed, expected,
                        severity=severity, note=note)

    def approx(self, check_id: str, description: str, value, target: float,
               rel_pct: float, note: str = "") -> bool:
        ok = isinstance(value, (int, float)) and abs(float(value) - target) \
            <= abs(target) * rel_pct / 100.0
        return self.add(check_id, description, ok, value,
                        f"{target} ±{rel_pct}%", note=note)

    def close(self, check_id: str, description: str, value, target: float,
              abs_tol: float, note: str = "") -> bool:
        ok = isinstance(value, (int, float)) and abs(float(value) - target) <= abs_tol
        return self.add(check_id, description, ok, value,
                        f"{target} ±{abs_tol}", note=note)

    def eq(self, check_id: str, description: str, value, expected, note: str = "") -> bool:
        return self.add(check_id, description, value == expected, value, expected,
                        note=note)

    # -- summary -----------------------------------------------------------
    def counts(self) -> tuple[int, int, int]:
        passed = sum(1 for r in self.rows if r["ok"])
        warned = sum(1 for r in self.rows if not r["ok"] and r["severity"] == "warn")
        failed = sum(1 for r in self.rows if not r["ok"] and r["severity"] != "warn")
        return passed, failed, warned

    def failures(self) -> list[dict]:
        return [r for r in self.rows if not r["ok"] and r["severity"] != "warn"]

    def payload(self) -> dict:
        passed, failed, warned = self.counts()
        return {
            "key": self.key,
            "title": self.title,
            "rows": self.rows,
            "meta": self.meta,
            "passed": passed,
            "failed": failed,
            "warned": warned,
        }

    def write(self) -> Path:
        TMP_DIR.mkdir(parents=True, exist_ok=True)
        target = TMP_DIR / f"results_{self.key}.json"
        target.write_text(json.dumps(self.payload(), indent=2, ensure_ascii=False),
                          encoding="utf-8")
        return target

    def finish(self) -> int:
        self.write()
        passed, failed, warned = self.counts()
        print(f"\n== {self.title}: {passed} passed, {failed} failed, {warned} warned ==",
              flush=True)
        if failed:
            print("FAILED CHECKS:", flush=True)
            for row in self.failures():
                print(f"  - {row['id']}: {row['description']} | observed={row['observed']}"
                      f" | expected={row['expected']}", flush=True)
        return 1 if failed else 0


def _fmt(value) -> str:
    if isinstance(value, str):
        return value if len(value) <= 300 else value[:297] + "..."
    if isinstance(value, float):
        return f"{value:.6g}"
    if isinstance(value, (list, tuple)):
        text = ", ".join(str(v) for v in value)
        return text if len(text) <= 300 else text[:297] + "..."
    if isinstance(value, dict):
        try:
            text = json.dumps(value, ensure_ascii=False)
        except Exception:
            text = str(value)
        return text if len(text) <= 300 else text[:297] + "..."
    return str(value)


def run_script(script: Path) -> int:
    return 0 if script.exists() else 1


def dispatch(main) -> None:
    """Run ``main(checker) -> int`` with UTF-8 stdout and a stable exit code."""
    utf8_stdout()
    raise SystemExit(main())


_ = urlparse  # keep import for future query-building needs
