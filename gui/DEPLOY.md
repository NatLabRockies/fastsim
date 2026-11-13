# Deploying FASTSim Studio

FASTSim Studio is a **Python** application: the simulation backend imports `fastsim`
(a 46 MB compiled Rust `.pyd`) and `matplotlib`. That single fact decides where it can run.

| Runtime | Can host FASTSim Studio? |
|---|---|
| Any machine with Python 3.10–3.15 + fastsim + matplotlib | ✅ |
| A VM / VPS / container / Cloud Run / Fly.io | ✅ |
| **Cloudflare Workers / Pages Functions** | ❌ — JS/WASM only, cannot import Python C-extensions |
| **Static hosting (GitHub Pages, CF Pages static)** | ❌ for the API — frontend only, `Run` would have no backend |

So "put it under a custom domain / behind the DSH entry" always means:
**expose a Python host**, then point a URL at it.

The frontend is pure static files, so it can be split from the API if you want —
but the API must live somewhere Python-capable.

---

## Option A — Cloudflare Tunnel (fastest; keeps running on your machine)

Requires `cloudflared` (already installed on this workstation) and a Cloudflare account.

```powershell
# terminal 1 — the app
& "C:\Users\Pok fun\FASTSim\venv\Scripts\python.exe" gui\backend\app.py --port 8000

# terminal 2 — public HTTPS URL
cloudflared tunnel --url http://127.0.0.1:8000
```

`cloudflared` prints a `https://<random>.trycloudflare.com` URL. That URL is live
immediately, supports HTTPS and WebSockets, and needs **no inbound port** and no router
configuration.

**Caveats — read these:**

- A quick tunnel's hostname changes every restart. For a stable hostname use a
  *named* tunnel bound to a domain you own:
  ```powershell
  cloudflared tunnel login
  cloudflared tunnel create fastsim-studio
  cloudflared tunnel route dns fastsim-studio fastsim.example.com
  cloudflared tunnel run --url http://127.0.0.1:8000 fastsim-studio
  ```
- **The app has no authentication.** Anyone with the URL can run simulations on your
  machine. Do **not** expose a quick tunnel publicly as-is. Either
  1. put it behind **Cloudflare Access** (Zero Trust → Access → Application → your
     hostname → policy: allow your email only), or
  2. keep the URL private and only link to it from an already-protected page
     (the DSH entry already returns 401 without the passphrase — that protects the
     *link*, not the *URL*).
- The machine must stay awake. If it sleeps, the site is gone.

This is the right choice when the goal is "make it reachable from the DSH entry today".

---

## Option B — real container host (Cloud Run / Fly.io / any VPS)

No dependency on your workstation. Needs a Dockerfile and a host account.

`gui/Dockerfile` is provided:

```dockerfile
FROM python:3.12-slim
WORKDIR /app
COPY gui /app/gui
RUN pip install --no-cache-dir fastsim matplotlib
ENV PYTHONUNBUFFERED=1
EXPOSE 8000
CMD ["python", "gui/backend/app.py", "--host", "0.0.0.0", "--port", "8000"]
```

```bash
# any container host
docker build -f gui/Dockerfile -t fastsim-studio .
docker run -p 8000:8000 fastsim-studio
```

- **Cloud Run**: `gcloud run deploy fastsim-studio --source . --port 8000 --allow-unauthenticated`
  (drop `--allow-unauthenticated` to require IAM auth instead).
- **Fly.io**: `fly launch --dockerfile gui/Dockerfile` then `fly deploy`.
- **VPS**: run the container behind nginx/Caddy with TLS.

Notes:
- Allow `--host 0.0.0.0` inside containers; the default binds `127.0.0.1` only.
- A cold container pays the fastsim import cost (~1 s) on first request.
- Still **no built-in auth** — put it behind your platform's IAM, or Cloudflare Access,
  or add a shared-secret check in `app.py`.

---

## Option C — split: static frontend on Cloudflare, API elsewhere

Only worth it if you want a CDN-hosted UI. The frontend hard-codes same-origin
`/api/...` paths, so splitting requires either a Worker route that proxies `/api/*`
to the Python host, or editing `app.js` to use an absolute API base. Not recommended
for a first deployment.

---

## Pointing the DSH entry at it

The DSH entry is a Cloudflare Worker that serves the harness UI behind a passphrase.
Two ways to surface the GUI:

1. **Add a link** in that Worker's page (a nav/landing link to the tunnel or container
   URL). No proxying, no Python on Cloudflare — just a link. Simplest and least risky.
2. **Proxy a subpath** (`/fastsim/*` → the Python host) from the Worker. This keeps the
   URL inside your existing origin and inherits the passphrase gate, but every request
   (including the ~265 KB base64 figure) passes through the Worker, and you must forward
   method, body and `Content-Type`. Doable, but the Worker is a separate project —
   it must be updated and redeployed from its own source.

Option 1 is recommended; option 2 needs the Worker's source tree, which is not part of
this repository.

---

## Health check

```bash
curl -s http://127.0.0.1:8000/api/schema | python -c "import json,sys; d=json.load(sys.stdin); print(len(d['presets']), len(d['parameters']))"
# expect: 9 46
```
