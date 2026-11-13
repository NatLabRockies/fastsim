"""
simulate.py -- apply overrides, run a FASTSim SimDrive, and assemble the
``POST /api/simulate`` payload (CONTRACT.md section 1.2).

Everything here is derived from real fastsim output:
  * scalars  <- ``SimDrive.to_dict(flatten=True)``
  * time series <- ``SimDrive.to_dataframe()``
"""

from __future__ import annotations

import copy
import math

import numpy as np
import fastsim

import introspection

# --------------------------------------------------------------------------
# Units / constants -- SINGLE SOURCE OF TRUTH for the GUI backend.
# export_code.py imports METERS_PER_MILE, plots.py mirrors it (Agg figures only);
# never type a mile constant anywhere else.
# --------------------------------------------------------------------------
METERS_PER_MILE = 1609.344
MPS_TO_MPH = 3600.0 / METERS_PER_MILE  # 2.2369362920544025, derived so it cannot drift
JOULES_PER_KWH = 3.6e6
KWH_PER_GGE = 33.7  # gasoline-gallon-equivalent energy content (mpgge convention)
TRACE_TOLERANCE_MPS = 0.1

# --------------------------------------------------------------------------
# KPI table (CONTRACT.md section 1.2) -- key -> (unit, label_en, label_zh)
# --------------------------------------------------------------------------
KPI_DEFINITIONS: tuple[tuple[str, str, str, str], ...] = (
    ("distance_mi", "mi", "Distance", "\u884c\u9a76\u8ddd\u79bb"),
    ("fuel_consumed_kwh", "kWh", "Fuel Energy", "\u71c3\u6599\u80fd\u91cf"),
    ("electricity_consumed_kwh", "kWh", "Electricity Used", "\u8017\u7535\u91cf"),
    ("fuel_economy_mpgge", "mpgge", "Fuel Economy", "\u71c3\u6cb9\u7ecf\u6d4e\u6027"),
    ("electric_economy_wh_per_mi", "Wh/mi", "Electricity Economy", "\u7535\u8017\u7387"),
    ("soc_start_pct", "%", "SOC Start", "\u8d77\u59cb SOC"),
    ("soc_end_pct", "%", "SOC End", "\u7ec8\u6b62 SOC"),
    ("energy_to_wheels_kwh", "kWh", "Energy at Wheels", "\u8f66\u8f6e\u7275\u5f15\u80fd\u91cf"),
    ("regen_recovered_kwh", "kWh", "Regen Recovered", "\u518d\u751f\u56de\u6536\u80fd\u91cf"),
    ("avg_speed_mph", "mph", "Average Speed", "\u5e73\u5747\u8f66\u901f"),
    ("max_speed_mph", "mph", "Max Speed", "\u6700\u9ad8\u8f66\u901f"),
    ("trace_met_pct", "%", "Cycle Trace Match", "\u5de5\u51b5\u8ddf\u968f\u7387"),
)
_KPI_META = {key: (unit, en, zh) for key, unit, en, zh in KPI_DEFINITIONS}


# --------------------------------------------------------------------------
# Errors
# --------------------------------------------------------------------------
class SimulationError(Exception):
    """Base class for clean, client-facing errors."""

    status = 400
    error = "Simulation error"

    def __init__(self, detail: str):
        super().__init__(detail)
        self.detail = detail


class UnknownParameterError(SimulationError):
    error = "Unknown parameter path"


class NotEditableError(SimulationError):
    error = "Parameter is not editable"


class InvalidOverrideError(SimulationError):
    error = "Invalid override value"


def _coerce(existing, value):
    """Coerce an incoming JSON value to the type the field already holds."""
    if isinstance(existing, bool):
        if isinstance(value, bool):
            return value
        if isinstance(value, str) and value.lower() in ("true", "false"):
            return value.lower() == "true"
        raise InvalidOverrideError(f"expected a boolean, got {value!r}")
    if isinstance(existing, int):
        if isinstance(value, bool):
            raise InvalidOverrideError(f"expected an integer, got {value!r}")
        if isinstance(value, (int, float)):
            return int(round(value))
        raise InvalidOverrideError(f"expected an integer, got {value!r}")
    if isinstance(existing, float):
        if isinstance(value, bool) or not isinstance(value, (int, float)):
            raise InvalidOverrideError(f"expected a number, got {value!r}")
        return float(value)
    if isinstance(value, (dict, list)):
        raise InvalidOverrideError(f"expected a scalar, got {type(value).__name__}")
    return value


def apply_overrides(veh_dict: dict, overrides: dict) -> None:
    """Set dotted paths in-place.  Never creates keys (CONTRACT.md 1.1 rule 2)."""
    for path, value in overrides.items():
        if not isinstance(path, str) or not path:
            raise UnknownParameterError(str(path))
        parts = path.split(".")
        node = veh_dict
        for depth, key in enumerate(parts[:-1]):
            if not isinstance(node, dict) or key not in node:
                raise UnknownParameterError(path)
            node = node[key]
        last = parts[-1]
        if not isinstance(node, dict) or last not in node:
            raise UnknownParameterError(path)
        existing = node[last]
        if isinstance(existing, (dict, list)):
            raise NotEditableError(path)
        node[last] = _coerce(existing, value)


# --------------------------------------------------------------------------
# Helpers
# --------------------------------------------------------------------------
def _finite(value):
    if isinstance(value, bool):
        return None
    if isinstance(value, (int, float)) and math.isfinite(value):
        return value
    return None


def _kpi(key: str, value) -> dict | None:
    """Build one ``{value, unit, label_en, label_zh}`` entry, or None if unusable."""
    numeric = _finite(float(value)) if isinstance(value, (int, float)) else None
    if numeric is None:
        return None
    unit, label_en, label_zh = _KPI_META[key]
    return {
        # 6 dp keeps far more precision than any consumer prints, so the API and
        # the exported script agree at every printed precision (3 dp distance,
        # 4 dp kWh, 2 dp economy) -- do not round coarser here.
        "value": round(float(numeric), 6),
        "unit": unit,
        "label_en": label_en,
        "label_zh": label_zh,
    }


def _series(values, ndigits: int = 5) -> list:
    """JSON-safe list of floats (non-finite -> None)."""
    out: list = []
    for v in values:
        f = _finite(float(v))
        out.append(None if f is None else round(f, ndigits))
    return out


def _overall_trace_match(speed_ach: np.ndarray, speed_target: np.ndarray) -> float:
    if speed_ach.size == 0:
        return 0.0
    met = np.abs(speed_ach - speed_target) <= TRACE_TOLERANCE_MPS
    return float(met.mean() * 100.0)


def _column(df, name: str):
    return df[name].to_numpy(dtype=float) if name in df.columns else None


def _pt_sub(veh_dict: dict, key: str) -> dict | None:
    pt_type = veh_dict.get("pt_type")
    if not isinstance(pt_type, dict) or len(pt_type) != 1:
        return None
    sub = next(iter(pt_type.values()))
    if not isinstance(sub, dict):
        return None
    value = sub.get(key)
    return value if isinstance(value, dict) else None


# --------------------------------------------------------------------------
# Main entry point
# --------------------------------------------------------------------------
SUPPORTED_LANGS = ("en", "zh", "zh-Hant")

# Chinese variants that all mean "Traditional Chinese" (Taiwan / Hong Kong).
_HANT_TAGS = {"zh-hant", "zh-tw", "zh-hk", "zh-mo", "tw", "hk"}


def resolve_lang(value) -> str:
    """Normalise the optional ``lang`` request field to a supported language tag.

    Accepts 'en', 'zh'/'zh-CN'/'zh_Hans' (Simplified), and
    'zh-Hant'/'zh-TW'/'zh-HK'/'zh_MO' (Traditional); anything else (including a
    missing value) resolves to the default 'en'.  Language affects figure text
    and the fonts used for it -- never a single number.
    """
    if isinstance(value, str):
        tag = value.strip().lower().replace("_", "-")
        if tag in _HANT_TAGS or tag.startswith("zh-hant") or tag.startswith("zh-tw") \
                or tag.startswith("zh-hk"):
            return "zh-Hant"
        primary = tag.split("-")[0]
        if primary == "zh":
            return "zh"
    return "en"


def run_simulation(
    preset: str | None,
    cycle: str | None,
    overrides: dict | None = None,
    render_matplotlib: bool = True,
    lang: str | None = None,
) -> dict:
    """Run one simulation and return the API payload."""
    preset_id = introspection.resolve_preset_id(preset)
    cycle_id = introspection.resolve_cycle_id(cycle)
    lang_code = resolve_lang(lang)
    overrides = overrides or {}
    if not isinstance(overrides, dict):
        raise InvalidOverrideError("`overrides` must be an object of path -> value")

    # A fresh deep copy per request: the cached template must never be mutated.
    veh_dict = copy.deepcopy(introspection.load_vehicle_dict(preset_id))
    apply_overrides(veh_dict, overrides)

    try:
        veh = fastsim.Vehicle.from_dict(veh_dict)
    except Exception as exc:  # fastsim rejects the modified configuration
        raise InvalidOverrideError(str(exc)) from exc

    veh.set_save_interval(1)  # MUST run before the simulation, else history is empty
    cyc = fastsim.Cycle.from_resource(cycle_id)
    sim_drive = fastsim.SimDrive(veh, cyc)
    sim_drive.run()

    df = sim_drive.to_dataframe()
    flat = sim_drive.to_dict(flatten=True)

    pt_type = introspection.powertrain_key(veh_dict) or ""
    has_fc = _pt_sub(veh_dict, "fc") is not None
    has_em = _pt_sub(veh_dict, "em") is not None
    has_res = _pt_sub(veh_dict, "res") is not None
    prefix = f"veh.pt_type.{pt_type}"

    # ---------------- time base ----------------
    time_s = _column(df, "cyc.time_seconds")
    if time_s is None:
        time_s = np.arange(len(df), dtype=float)
    dt = np.diff(time_s, prepend=time_s[0] if time_s.size else 0.0)

    speed_target = _column(df, "cyc.speed_meters_per_second")
    speed_ach = _column(df, "veh.history.speed_ach_meters_per_second")
    if speed_target is None:
        speed_target = np.zeros_like(time_s)
    if speed_ach is None:
        speed_ach = np.zeros_like(time_s)

    fuel_pwr = _column(df, f"{prefix}.fc.history.pwr_fuel_watts") if has_fc else None
    eng_pwr = _column(df, f"{prefix}.fc.history.pwr_prop_watts") if has_fc else None
    mot_pwr = _column(df, f"{prefix}.em.history.pwr_mech_prop_out_watts") if has_em else None
    batt_pwr = _column(df, f"{prefix}.res.history.pwr_out_electrical_watts") if has_res else None
    soc = _column(df, f"{prefix}.res.history.soc") if has_res else None

    grade = _column(df, "cyc.grade")
    cum_dist = _column(df, "cyc.dist_meters")
    if cum_dist is None:
        cum_dist = _column(df, "veh.history.dist_meters")

    # ---------------- cumulative energies ----------------
    cum_fuel_kwh = None
    if fuel_pwr is not None:
        cum_fuel_kwh = np.cumsum(fuel_pwr * dt) / JOULES_PER_KWH
    cum_elec_kwh = None
    if batt_pwr is not None:
        cum_elec_kwh = np.cumsum(batt_pwr * dt) / JOULES_PER_KWH
    cum_energy_kwh = None
    if cum_fuel_kwh is not None and cum_elec_kwh is not None:
        cum_energy_kwh = cum_fuel_kwh + cum_elec_kwh
    elif cum_fuel_kwh is not None:
        cum_energy_kwh = cum_fuel_kwh
    elif cum_elec_kwh is not None:
        cum_energy_kwh = cum_elec_kwh

    # ---------------- KPI scalars ----------------
    dist_m = _finite(flat.get("veh.state.dist_meters")) or 0.0
    distance_mi = dist_m / METERS_PER_MILE

    fuel_kwh = None
    if has_fc:
        joules = flat.get(f"{prefix}.fc.state.energy_fuel_joules")
        if joules is None and cum_fuel_kwh is not None and cum_fuel_kwh.size:
            joules = float(cum_fuel_kwh[-1]) * JOULES_PER_KWH
        if joules is not None:
            fuel_kwh = float(joules) / JOULES_PER_KWH

    elec_kwh = None
    if has_res:
        joules = flat.get(f"{prefix}.res.state.energy_out_electrical_joules")
        if joules is not None:
            elec_kwh = float(joules) / JOULES_PER_KWH

    soc_start = float(soc[0]) if soc is not None and soc.size else None
    soc_end = flat.get(f"{prefix}.res.state.soc")
    if soc_end is None and soc is not None and soc.size:
        soc_end = float(soc[-1])

    tractive_j = flat.get("veh.state.energy_tractive_joules")
    energy_to_wheels = abs(float(tractive_j)) / JOULES_PER_KWH if tractive_j is not None else None

    regen_kwh = None
    if batt_pwr is not None and batt_pwr.size:
        # pwr_out_electrical > 0 discharges, < 0 charges (regen) -> report a positive number
        regen_kwh = float(-(np.clip(batt_pwr, None, 0.0) * dt).sum()) / JOULES_PER_KWH

    duration_s = float(time_s[-1] - time_s[0]) if time_s.size > 1 else 0.0
    avg_speed_mph = (dist_m / duration_s) * MPS_TO_MPH if duration_s > 0 else None
    max_speed_mph = float(np.nanmax(speed_ach)) * MPS_TO_MPH if speed_ach.size else None

    candidates: dict[str, object] = {
        "distance_mi": distance_mi,
        "fuel_consumed_kwh": fuel_kwh,
        "electricity_consumed_kwh": elec_kwh,
        "fuel_economy_mpgge": (
            distance_mi / (fuel_kwh / KWH_PER_GGE)
            if fuel_kwh is not None and fuel_kwh > 0 and distance_mi > 0
            else None
        ),
        "electric_economy_wh_per_mi": (
            elec_kwh * 1000.0 / distance_mi
            if elec_kwh is not None and distance_mi > 0
            else None
        ),
        "soc_start_pct": soc_start * 100.0 if soc_start is not None else None,
        "soc_end_pct": float(soc_end) * 100.0 if soc_end is not None else None,
        "energy_to_wheels_kwh": energy_to_wheels,
        "regen_recovered_kwh": regen_kwh,
        "avg_speed_mph": avg_speed_mph,
        "max_speed_mph": max_speed_mph,
        "trace_met_pct": _overall_trace_match(speed_ach, speed_target),
    }

    kpis: dict[str, dict] = {}
    for key, _unit, _en, _zh in KPI_DEFINITIONS:
        entry = _kpi(key, candidates[key]) if candidates[key] is not None else None
        if entry is not None:
            kpis[key] = entry

    # ---------------- time series (CONTRACT.md 1.2) ----------------
    timeseries: dict[str, list] = {
        "time_s": _series(time_s, 4),
        "speed_target_mps": _series(speed_target, 4),
        "speed_achieved_mps": _series(speed_ach, 4),
    }
    if grade is not None:
        timeseries["grade_pct"] = _series(grade * 100.0, 4)
    if eng_pwr is not None:
        timeseries["engine_power_kw"] = _series(eng_pwr / 1000.0, 4)
    if mot_pwr is not None:
        timeseries["motor_power_kw"] = _series(mot_pwr / 1000.0, 4)
    if batt_pwr is not None:
        timeseries["battery_power_kw"] = _series(batt_pwr / 1000.0, 4)
    if soc is not None:
        timeseries["soc_pct"] = _series(soc * 100.0, 4)
    if fuel_pwr is not None:
        timeseries["fuel_power_kw"] = _series(fuel_pwr / 1000.0, 4)
    if cum_fuel_kwh is not None:
        timeseries["cum_fuel_kwh"] = _series(cum_fuel_kwh, 6)
    if cum_energy_kwh is not None:
        timeseries["cum_energy_kwh"] = _series(cum_energy_kwh, 6)
    if cum_dist is not None:
        timeseries["cum_distance_mi"] = _series(cum_dist / METERS_PER_MILE, 6)

    payload = {"kpis": kpis, "timeseries": timeseries, "lang": lang_code}

    if render_matplotlib:
        try:
            import plots  # imported lazily: matplotlib costs ~250 ms on first use

            figures = plots.render_figures({
                "time_s": time_s,
                "speed_target": speed_target,
                "speed_achieved": speed_ach,
                "engine_kw": eng_pwr / 1000.0 if eng_pwr is not None else None,
                "motor_kw": mot_pwr / 1000.0 if mot_pwr is not None else None,
                "battery_kw": batt_pwr / 1000.0 if batt_pwr is not None else None,
                "soc_pct": soc * 100.0 if soc is not None else None,
                "cum_fuel_kwh": cum_fuel_kwh,
                "cum_energy_kwh": cum_energy_kwh,
                "cum_distance_mi": (cum_dist / METERS_PER_MILE) if cum_dist is not None else None,
                "title": f"{veh_dict.get('name', preset_id)} \u00b7 {cycle_id}",
                "lang": lang_code,
            })
        except Exception as exc:  # plotting must never break a simulation
            import sys
            import traceback

            print(f"[fastsim-gui] matplotlib render failed: {exc}", file=sys.stderr)
            traceback.print_exc()
            figures = None
        if figures:
            payload["matplotlib_png_base64"] = figures["png_base64"]
            if figures.get("svg"):
                payload["matplotlib_svg"] = figures["svg"]

    return payload
