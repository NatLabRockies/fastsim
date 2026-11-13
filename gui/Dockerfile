# FASTSim Studio — container image
#
# The backend needs Python + fastsim + matplotlib. Cloudflare Workers/Pages cannot
# run this (JS/WASM only), so any container host (Cloud Run, Fly.io, a VPS) is the
# deployment target.
#
#   docker build -f gui/Dockerfile -t fastsim-studio .
#   docker run -p 8000:8000 fastsim-studio
#
FROM python:3.13-slim

ENV PYTHONUNBUFFERED=1 \
    PYTHONDONTWRITEBYTECODE=1 \
    MPLBACKEND=Agg

# matplotlib wheel bundles its own freetype; no extra system libs are needed for
# the Agg backend, but fonts must be present for any text rendering at all.
RUN apt-get update \
 && apt-get install -y --no-install-recommends fonts-dejavu-core \
 && rm -rf /var/lib/apt/lists/*

WORKDIR /app

COPY gui /app/gui

# Only the two runtime requirements; the GUI itself is stdlib-only.
RUN pip install --no-cache-dir fastsim matplotlib

EXPOSE 8000

# 0.0.0.0 is required inside a container; the script defaults to 127.0.0.1.
CMD ["python", "gui/backend/app.py", "--host", "0.0.0.0", "--port", "8000"]
