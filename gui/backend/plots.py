"""
plots.py -- the static 4-panel matplotlib figure of CONTRACT.md section 1.2.

Uses the object-oriented API only (no pyplot global state), the Agg backend,
and never raises: the caller turns any failure into "no figure".

Optional localisation: ``data["lang"]`` selects the label language --
``"zh"`` renders panel titles, axis labels and legends in Simplified Chinese,
``"zh-Hant"`` in Traditional Chinese (Taiwan / Hong Kong usage), anything else
(and the default) in English.  Numbers never depend on ``lang`` -- only strings
and the font family do.  If no CJK font resolves, the figure stays English and
exactly one warning is logged.
"""

from __future__ import annotations

import base64
import io
import sys
import threading

import matplotlib

matplotlib.use("Agg")  # must happen before pyplot / backends are imported

from matplotlib import font_manager  # noqa: E402
from matplotlib.backends.backend_agg import FigureCanvasAgg  # noqa: E402
from matplotlib.figure import Figure  # noqa: E402

# Sequential blue / orange / green / purple palette (CONTRACT.md 1.2)
BLUE = "#1f5fa8"
ORANGE = "#d2691e"
GREEN = "#2e8b57"
PURPLE = "#7d4fa8"
GREY = "#9aa3ad"
AMBER = "#c9a227"

# Mirrors simulate.METERS_PER_MILE (kept local so this module needs no fastsim
# import; the mph twin axis is cosmetic).  Never introduce a second mile value.
METERS_PER_MILE = 1609.344
MPS_TO_MPH = 3600.0 / METERS_PER_MILE

_RENDER_LOCK = threading.Lock()

DPI = 140
FIGSIZE = (12.0, 9.0)
_MPL_DEFAULT_SANS = list(matplotlib.rcParamsDefault.get("font.sans-serif", ["DejaVu Sans"]))

# CJK fallback chains; DejaVu Sans only supplies Latin glyphs, never CJK ones.
# Traditional Chinese prefers Microsoft JhengHei (the Windows TW/ HK UI face),
# Simplified prefers Microsoft YaHei (the Windows PRC UI face).
CJK_FAMILIES = ["Microsoft YaHei", "Microsoft JhengHei", "SimHei", "SimSun"]
HANT_FAMILIES = ["Microsoft JhengHei", "PMingLiU", "Microsoft YaHei", "SimHei", "SimSun"]
SCRIPT_LANGS = ("zh", "zh-Hant")


def _detect_cjk_family(preferred: list[str] | None = None) -> str | None:
    """First CJK family matplotlib can actually resolve, else None."""
    try:
        available = {font.name for font in font_manager.fontManager.ttflist}
    except Exception:  # pragma: no cover - a broken font cache must not break us
        return None
    for family in (preferred or CJK_FAMILIES):
        if family in available:
            return family
    return None


CJK_FAMILY = _detect_cjk_family()
CJK_FAMILY_HANT = _detect_cjk_family(HANT_FAMILIES)
CJK_AVAILABLE = CJK_FAMILY is not None or CJK_FAMILY_HANT is not None
_WARNED = False

if not CJK_AVAILABLE:  # logged once, at import time
    print(
        "[fastsim-gui] no CJK font resolved (tried "
        + ", ".join(CJK_FAMILIES)
        + "); Chinese figures will fall back to English text.",
        file=sys.stderr,
        flush=True,
    )


# --------------------------------------------------------------------------
# Localised strings (units stay Latin)
# --------------------------------------------------------------------------
LABELS: dict[str, dict[str, str]] = {
    "en": {
        "time": "Time [s]",
        "speed_mps": "Speed [m/s]",
        "speed_mph": "Speed [mph]",
        "power_kw": "Power [kW]",
        "soc_pct": "SOC [%]",
        "energy_kwh": "Energy [kWh]",
        "distance_mi": "Distance [mi]",
        "title_speed": "Vehicle Speed",
        "title_power": "Powertrain Power Split",
        "title_soc": "Battery SOC",
        "title_energy": "Cumulative Fuel & Energy",
        "leg_target": "target",
        "leg_achieved": "achieved",
        "leg_engine": "engine",
        "leg_motor": "motor",
        "leg_battery": "battery",
        "leg_cum_fuel": "cumulative fuel",
        "leg_cum_energy": "cumulative fuel + electricity",
        "leg_soc": "SOC",
        "no_power": "no powertrain power channels",
        "no_energy": "no cumulative energy channels",
    },
    "zh": {
        "time": "\u65f6\u95f4 [s]",
        "speed_mps": "\u8f66\u901f [m/s]",
        "speed_mph": "\u8f66\u901f [mph]",
        "power_kw": "\u529f\u7387 [kW]",
        "soc_pct": "\u7535\u6c60 SOC [%]",
        "energy_kwh": "\u80fd\u91cf [kWh]",
        "distance_mi": "\u8ddd\u79bb [mi]",
        "title_speed": "\u8f66\u901f",
        "title_power": "\u52a8\u529b\u603b\u6210\u529f\u7387\u5206\u914d",
        "title_soc": "\u7535\u6c60 SOC",
        "title_energy": "\u7d2f\u79ef\u71c3\u6599\u4e0e\u80fd\u91cf",
        "leg_target": "\u76ee\u6807\u8f66\u901f",
        "leg_achieved": "\u5b9e\u9645\u8f66\u901f",
        "leg_engine": "\u53d1\u52a8\u673a",
        "leg_motor": "\u7535\u673a",
        "leg_battery": "\u7535\u6c60",
        "leg_cum_fuel": "\u7d2f\u79ef\u71c3\u6599",
        "leg_cum_energy": "\u7d2f\u79ef\u80fd\u91cf",
        "leg_soc": "SOC",
        "no_power": "\u65e0\u52a8\u529b\u603b\u6210\u529f\u7387\u901a\u9053",
        "no_energy": "\u65e0\u7d2f\u79ef\u80fd\u91cf\u901a\u9053",
    },
    # Traditional Chinese (Taiwan / Hong Kong usage); generated from the "zh"
    # table with opencc s2twp + hand fixes, see gui/frontend/hant.js for the twin
    # map the browser uses for API-supplied text.
    "zh-Hant": {
        "time": "\u6642\u9593 [s]",
        "speed_mps": "\u8eca\u901f [m/s]",
        "speed_mph": "\u8eca\u901f [mph]",
        "power_kw": "\u529f\u7387 [kW]",
        "soc_pct": "\u96fb\u6c60 SOC [%]",
        "energy_kwh": "\u80fd\u91cf [kWh]",
        "distance_mi": "\u8ddd\u96e2 [mi]",
        "title_speed": "\u8eca\u901f",
        "title_power": "\u52d5\u529b\u7e3d\u6210\u529f\u7387\u5206\u914d",
        "title_soc": "\u96fb\u6c60 SOC",
        "title_energy": "\u7d2f\u7a4d\u71c3\u6599\u8207\u80fd\u91cf",
        "leg_target": "\u76ee\u6a19\u8eca\u901f",
        "leg_achieved": "\u5be6\u969b\u8eca\u901f",
        "leg_engine": "\u767c\u52d5\u6a5f",
        "leg_motor": "\u96fb\u6a5f",
        "leg_battery": "\u96fb\u6c60",
        "leg_cum_fuel": "\u7d2f\u7a4d\u71c3\u6599",
        "leg_cum_energy": "\u7d2f\u7a4d\u80fd\u91cf",
        "leg_soc": "SOC",
        "no_power": "\u7121\u52d5\u529b\u7e3d\u6210\u529f\u7387\u901a\u9053",
        "no_energy": "\u7121\u7d2f\u7a4d\u80fd\u91cf\u901a\u9053",
    },
}


def labels_for(lang: str) -> dict[str, str]:
    """Label table for a language, honouring the CJK availability fallback."""
    if lang in SCRIPT_LANGS and CJK_AVAILABLE:
        return LABELS[lang]
    return LABELS["en"]


def _apply_fonts(lang: str) -> None:
    """Point rcParams at the right font stack for this render (called under lock)."""
    global _WARNED
    if lang in SCRIPT_LANGS:
        if CJK_AVAILABLE:
            if lang == "zh-Hant":
                preferred, chain = CJK_FAMILY_HANT, HANT_FAMILIES
            else:
                preferred, chain = CJK_FAMILY, CJK_FAMILIES
            face = preferred or CJK_FAMILY or CJK_FAMILY_HANT
            matplotlib.rcParams["font.sans-serif"] = (
                [face] + [f for f in chain if f != face] + ["DejaVu Sans"])
            matplotlib.rcParams["axes.unicode_minus"] = False
            return
        if not _WARNED:
            _WARNED = True
            print("[fastsim-gui] Chinese figure requested but no CJK font resolved; "
                  "rendering English text instead.", file=sys.stderr, flush=True)
    matplotlib.rcParams["font.sans-serif"] = _MPL_DEFAULT_SANS
    matplotlib.rcParams["axes.unicode_minus"] = True


# --------------------------------------------------------------------------
# Panels
# --------------------------------------------------------------------------
def _style_axes(ax, title: str, ylabel: str, xlabel: str) -> None:
    ax.set_title(title, fontsize=11, fontweight="bold", pad=8)
    ax.set_xlabel(xlabel, fontsize=9)
    ax.set_ylabel(ylabel, fontsize=9)
    ax.grid(True, alpha=0.3)
    ax.tick_params(labelsize=8)


def _has(data, key: str) -> bool:
    value = data.get(key)
    return value is not None and len(value) > 0


def _panel_speed(ax, data, L: dict[str, str]) -> None:
    time_s = data["time_s"]
    ax.plot(time_s, data["speed_target"], linestyle="--", color=GREY, linewidth=1.4,
            label=L["leg_target"])
    ax.plot(time_s, data["speed_achieved"], linestyle="-", color=BLUE, linewidth=1.6,
            label=L["leg_achieved"])
    _style_axes(ax, L["title_speed"], L["speed_mps"], L["time"])
    ax.legend(loc="upper right", fontsize=8, framealpha=0.9)
    ax2 = ax.twinx()
    ax2.set_ylabel(L["speed_mph"], fontsize=9, color=ORANGE)
    ax2.tick_params(axis="y", labelsize=8, colors=ORANGE)
    ax2.set_ylim([y * MPS_TO_MPH for y in ax.get_ylim()])
    for spine in ax2.spines.values():
        spine.set_color(ORANGE)


def _panel_powers(ax, data, L: dict[str, str]) -> None:
    time_s = data["time_s"]
    plotted = False
    for key, color, label in (
        ("engine_kw", BLUE, L["leg_engine"]),
        ("motor_kw", ORANGE, L["leg_motor"]),
        ("battery_kw", GREEN, L["leg_battery"]),
    ):
        if _has(data, key):
            ax.plot(time_s, data[key], color=color, linewidth=1.4, label=label)
            plotted = True
    ax.axhline(0.0, color="#444444", linewidth=0.9, linestyle="-")
    if not plotted:
        ax.text(0.5, 0.5, L["no_power"], ha="center", va="center",
                fontsize=10, color=GREY, transform=ax.transAxes)
    _style_axes(ax, L["title_power"], L["power_kw"], L["time"])
    if plotted:
        ax.legend(loc="upper right", fontsize=8, ncol=3, framealpha=0.9)


def _panel_soc(ax, data, L: dict[str, str]) -> None:
    ax.plot(data["time_s"], data["soc_pct"], color=GREEN, linewidth=1.8, label=L["leg_soc"])
    _style_axes(ax, L["title_soc"], L["soc_pct"], L["time"])
    lo = float(min(data["soc_pct"]))
    hi = float(max(data["soc_pct"]))
    pad = max(0.5, (hi - lo) * 0.15)
    ax.set_ylim([max(0.0, lo - pad), min(100.0, hi + pad)])
    ax.legend(loc="upper right", fontsize=8, framealpha=0.9)


def _panel_cumulative(ax, data, L: dict[str, str]) -> None:
    time_s = data["time_s"]
    plotted = False
    if _has(data, "cum_fuel_kwh"):
        ax.plot(time_s, data["cum_fuel_kwh"], color=ORANGE, linewidth=1.7,
                label=L["leg_cum_fuel"])
        plotted = True
    if _has(data, "cum_energy_kwh"):
        ax.plot(time_s, data["cum_energy_kwh"], color=PURPLE, linewidth=1.7,
                label=L["leg_cum_energy"])
        plotted = True
    if not plotted:
        ax.text(0.5, 0.5, L["no_energy"], ha="center", va="center",
                fontsize=10, color=GREY, transform=ax.transAxes)
    _style_axes(ax, L["title_energy"], L["energy_kwh"], L["time"])
    if plotted:
        ax.legend(loc="upper left", fontsize=8, framealpha=0.9)

    # Secondary top axis: cumulative distance in miles over the same steps.
    if _has(data, "cum_distance_mi"):
        distance = data["cum_distance_mi"]
        first, last = float(distance[0]), float(distance[-1])
        if last > first:
            top = ax.twiny()
            top.set_xlim([first, last])
            top.set_xlabel(L["distance_mi"], fontsize=9)
            top.tick_params(axis="x", labelsize=8)
            top.grid(False)


# --------------------------------------------------------------------------
# Public entry point
# --------------------------------------------------------------------------
def render_figures(data: dict) -> dict | None:
    """Render the figure once; return {"png_base64": str, "svg": str} or None.

    ``matplotlib_svg`` is the raw SVG text so the browser can download it as
    ``image/svg+xml`` directly (the PNG stays base64 per CONTRACT.md 1.2).
    ``data["lang"]`` picks the label language ("en" default, "zh" Simplified,
    "zh-Hant" Traditional).
    """
    if not data or not _has(data, "time_s"):
        return None
    lang = data.get("lang") or "en"
    try:
        with _RENDER_LOCK:
            _apply_fonts(lang)
            labels = labels_for(lang)

            fig = Figure(figsize=FIGSIZE, dpi=DPI)
            fig.patch.set_facecolor("white")
            FigureCanvasAgg(fig)

            has_res = _has(data, "soc_pct")
            layouts = [["speed", "powers"]]
            if has_res:
                layouts.append(["soc", "cumulative"])
            elif _has(data, "cum_fuel_kwh") or _has(data, "cum_energy_kwh"):
                layouts.append(["cumulative"])
            else:
                layouts.append(["speed"])

            nrows = len(layouts)
            ncols = 2 if has_res or len(layouts[0]) == 2 else 1
            axes_list = fig.subplots(nrows, ncols, squeeze=False).ravel()

            builders = {
                "speed": _panel_speed,
                "powers": _panel_powers,
                "soc": _panel_soc,
                "cumulative": _panel_cumulative,
            }
            slot = 0
            for row in layouts:
                for name in row:
                    if slot >= axes_list.size:
                        break
                    builders[name](axes_list[slot], data, labels)
                    slot += 1
            for spare in range(slot, axes_list.size):
                axes_list[spare].set_visible(False)

            title = data.get("title")
            if title:
                fig.suptitle(title, fontsize=13, fontweight="bold")
                fig.tight_layout(rect=(0, 0, 1, 0.97))
            else:
                fig.tight_layout()

            png_buffer = io.BytesIO()
            fig.savefig(png_buffer, format="png", dpi=DPI, facecolor="white")
            png_bytes = png_buffer.getvalue()

            svg_text = ""
            try:
                svg_buffer = io.BytesIO()
                fig.savefig(svg_buffer, format="svg", facecolor="white")
                svg_text = svg_buffer.getvalue().decode("utf-8", "replace")
            except Exception:  # an SVG failure must not lose the PNG
                svg_text = ""

            fig.clear()
            del fig
        return {
            "png_base64": base64.b64encode(png_bytes).decode("ascii"),
            "svg": svg_text,
        }
    except Exception:  # plotting must never propagate
        import traceback

        print("[fastsim-gui] plots.render_figures failed", file=sys.stderr)
        traceback.print_exc()
        return None


def render_png(data: dict) -> str | None:
    """Backwards-compatible helper: base64 PNG only."""
    figures = render_figures(data)
    return figures["png_base64"] if figures else None
