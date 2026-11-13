"""
introspection.py -- turn a FASTSim Vehicle into the schema described by
CONTRACT.md section 1.1.

The parameter tree is rebuilt from the *live* ``Vehicle.to_dict()`` of the
selected preset, because the tree shape depends on the powertrain type
(Conv has ``fs``/``fc``, BEV has ``em``/``res``, HEV/PHEV have all four) and
on whether thermal models are present.
"""

from __future__ import annotations

import math
import threading

import fastsim

import metadata

# --------------------------------------------------------------------------
# Fixed preset / cycle lists (CONTRACT.md section 2)
# --------------------------------------------------------------------------
PRESET_ORDER: tuple[str, ...] = (
    "2012_Ford_Fusion.yaml",
    "2016_TOYOTA_Prius_Two.yaml",
    "2016 Nissan Leaf 30 kWh thrml.yaml",
    "2022 Tesla Model 3 RWD thrml.yaml",
    "2022_Renault_Zoe_ZE50_R135.yaml",
    "2020 Chevrolet Bolt EV thrml.yaml",
    "2021_Hyundai_Sonata_Hybrid_Blue_thrml.yaml",
    "2026_Chrysler_Pacifica_Select.yaml",
    "2026_Chrysler_Pacifica_Select_thrml.yaml",
)

CYCLE_ORDER: tuple[dict[str, str], ...] = (
    {"id": "udds.csv", "name": "UDDS (City)", "name_zh": "UDDS\uff08\u57ce\u5e02\uff09"},
    {"id": "hwfet.csv", "name": "HWFET (Highway)", "name_zh": "HWFET\uff08\u9ad8\u901f\uff09"},
)

# Documented step counts (CONTRACT.md section 2).  ``Cycle.len()`` counts the
# serialised rows (which includes the initial state for some files), so the
# documented values are authoritative here.
CYCLE_STEPS: dict[str, int] = {"udds.csv": 1369, "hwfet.csv": 766}

# Keys that are never user inputs (CONTRACT.md section 1.1, rule 3).
EXCLUDED_KEYS: frozenset[str] = frozenset({
    "state",
    "history",
    "save_interval",
    "doc",
    "min_fastsim_version",
    "db_path",
    "soc_bal_iter_history",
    "soc_bal_iters",
})

# fastsim serialises an absent optional sub-model as the *string* "None"
# (e.g. ``cabin``, ``hvac``, ``fc.thrml``).  Those are not parameters.
NONE_VARIANT = "None"

PT_TYPE_KEY = "pt_type"

_cache_lock = threading.Lock()
_vehicle_dict_cache: dict[str, dict] = {}
_presets_cache: list[dict] | None = None
_cycles_cache: list[dict] | None = None


# --------------------------------------------------------------------------
# Presets / cycles
# --------------------------------------------------------------------------
def load_vehicle_dict(preset_id: str) -> dict:
    """Load ``Vehicle.to_dict()`` for a preset, cached.  Caller must not mutate."""
    with _cache_lock:
        cached = _vehicle_dict_cache.get(preset_id)
    if cached is not None:
        return cached
    veh = fastsim.Vehicle.from_resource(preset_id)
    veh_dict = veh.to_dict()
    with _cache_lock:
        _vehicle_dict_cache[preset_id] = veh_dict
    return veh_dict


def powertrain_key(veh_dict: dict) -> str | None:
    """Return the concrete powertrain key (``Conv`` / ``HEV`` / ``PHEV`` / ``BEV``)."""
    pt = veh_dict.get(PT_TYPE_KEY)
    if isinstance(pt, dict) and len(pt) == 1:
        return next(iter(pt))
    return None


def list_presets() -> list[dict]:
    """Static preset list with id / name / pt_type / mass_kg."""
    global _presets_cache
    with _cache_lock:
        if _presets_cache is not None:
            return _presets_cache
    available = {str(p).replace("\\", "/").split("/")[-1] for p in fastsim.Vehicle.list_resources()}
    out: list[dict] = []
    for preset_id in PRESET_ORDER:
        if preset_id not in available:
            continue
        try:
            veh_dict = load_vehicle_dict(preset_id)
        except Exception:  # pragma: no cover - a broken resource should not kill the server
            continue
        mass = veh_dict.get("mass_kilograms")
        out.append({
            "id": preset_id,
            "name": str(veh_dict.get("name") or preset_id),
            "pt_type": powertrain_key(veh_dict),
            "mass_kg": round(float(mass), 1) if isinstance(mass, (int, float)) else None,
        })
    with _cache_lock:
        _presets_cache = out
    return out


def list_cycles() -> list[dict]:
    """Cycle list with duration in steps."""
    global _cycles_cache
    with _cache_lock:
        if _cycles_cache is not None:
            return _cycles_cache
    available = {str(p).replace("\\", "/").split("/")[-1] for p in fastsim.Cycle.list_resources()}
    out: list[dict] = []
    for entry in CYCLE_ORDER:
        if entry["id"] not in available:
            continue
        duration: int | None = CYCLE_STEPS.get(entry["id"])
        if duration is None:
            try:
                duration = int(fastsim.Cycle.from_resource(entry["id"]).len())
            except Exception:  # pragma: no cover
                duration = None
        out.append({**entry, "duration_s": duration})
    with _cache_lock:
        _cycles_cache = out
    return out


def default_preset_id() -> str:
    presets = list_presets()
    if not presets:
        raise RuntimeError("no FASTSim vehicle resources are available")
    return presets[0]["id"]


def resolve_preset_id(preset_id: str | None) -> str:
    """Validate a preset id (CONTRACT.md section 2) and return it."""
    known = [p["id"] for p in list_presets()]
    if preset_id is None or preset_id == "":
        return known[0] if known else default_preset_id()
    if preset_id not in known:
        raise ValueError(f"unknown preset: {preset_id}")
    return preset_id


def resolve_cycle_id(cycle_id: str | None) -> str:
    known = [c["id"] for c in list_cycles()]
    if cycle_id is None or cycle_id == "":
        return known[0]
    if cycle_id not in known:
        raise ValueError(f"unknown cycle: {cycle_id}")
    return cycle_id


# --------------------------------------------------------------------------
# Parameter tree walking
# --------------------------------------------------------------------------
def _looks_like_interpolator(node: dict) -> bool:
    """True for fastsim interpolator serialisations (a 'curve' in the schema)."""
    if "data" not in node:
        return False
    return "strategy" in node or "extrapolate" in node


def _short_path(path: str, pt_type: str | None) -> str:
    if pt_type:
        prefix = f"{PT_TYPE_KEY}.{pt_type}."
        if path.startswith(prefix):
            return path[len(prefix):]
    return path


def _infer_scalar_type(field_name: str, unit: str, options: list[str] | None) -> str:
    if options:
        return "enum"
    if unit != metadata.NO_UNIT:
        return "float"
    return "str"


def _make_param(
    *,
    path: str,
    field_name: str,
    value,
    param_type: str,
    unit: str,
    pt_type: str | None,
    options: list[str] | None = None,
    editable: bool | None = None,
) -> dict:
    group, group_zh, subgroup, subgroup_zh = metadata.classify(path)
    short = _short_path(path, pt_type)
    desc_en, desc_zh = metadata.describe(path, short, field_name, unit)
    if editable is None:
        editable = param_type in ("float", "int", "bool", "enum") and not metadata.is_readonly_enum(field_name)
    if param_type == "enum" and metadata.is_readonly_enum(field_name):
        editable = False
    return {
        "path": path,
        "name": field_name,
        "group": group,
        "group_zh": group_zh,
        "subgroup": subgroup,
        "subgroup_zh": subgroup_zh,
        "type": param_type,
        "default": value,
        "unit": unit,
        "description": desc_en,
        "description_zh": desc_zh,
        "editable": editable,
        "enum_options": options,
    }


def _json_scalar(value):
    """JSON-safe scalar (NaN/Inf -> None)."""
    if isinstance(value, float) and not math.isfinite(value):
        return None
    return value


def build_parameters(veh_dict: dict) -> list[dict]:
    """Walk the vehicle dict and emit one entry per scalar leaf."""
    pt_type = powertrain_key(veh_dict)
    out: list[dict] = []
    _walk(veh_dict, "", pt_type, out)
    return out


def _walk(node, path: str, pt_type: str | None, out: list[dict]) -> None:
    if isinstance(node, dict):
        if _looks_like_interpolator(node):
            field_name = path.split(".")[-1] if path else ""
            out.append(_make_param(
                path=path,
                field_name=field_name,
                value=None,
                param_type="curve",
                unit=metadata.NO_UNIT,
                pt_type=pt_type,
                editable=False,
            ))
            # interpolator internals: only the two scalar-valued enum knobs are
            # surfaced (read-only) so the enum requirement is visible.
            for knob in ("strategy", "extrapolate"):
                knob_value = node.get(knob)
                if isinstance(knob_value, str) and knob_value != NONE_VARIANT:
                    knob_path = f"{path}.{knob}"
                    out.append(_make_param(
                        path=knob_path,
                        field_name=knob,
                        value=knob_value,
                        param_type="enum",
                        unit=metadata.NO_UNIT,
                        pt_type=pt_type,
                        options=metadata.enum_options(knob, knob, pt_type),
                        editable=False,
                    ))
            return

        # `pt_cntrl` is a union: either a plain enum string or a variant dict.
        # In the dict form show which controller variant is active (read-only).
        if path.split(".")[-1] == "pt_cntrl":
            variant = next(iter(node), None)
            if variant:
                out.append(_make_param(
                    path=path,
                    field_name="pt_cntrl",
                    value=variant,
                    param_type="object",
                    unit=metadata.NO_UNIT,
                    pt_type=pt_type,
                    options=None,
                    editable=False,
                ))

        for key, value in node.items():
            if key in EXCLUDED_KEYS:
                continue
            child_path = f"{path}.{key}" if path else key
            child_pt = pt_type
            if not path and key == PT_TYPE_KEY and isinstance(value, dict) and len(value) == 1:
                child_pt = next(iter(value))
            _walk(value, child_path, child_pt, out)
        return

    if isinstance(node, list):  # pragma: no cover - lists only live inside curves
        return

    field_name = path.split(".")[-1] if path else ""
    if field_name in EXCLUDED_KEYS or not field_name:
        return

    unit = metadata.parse_unit(field_name)

    if isinstance(node, bool):
        out.append(_make_param(path=path, field_name=field_name, value=node,
                               param_type="bool", unit=unit, pt_type=pt_type))
        return
    if isinstance(node, int):
        out.append(_make_param(path=path, field_name=field_name, value=node,
                               param_type="int", unit=unit, pt_type=pt_type))
        return
    if isinstance(node, float):
        out.append(_make_param(path=path, field_name=field_name, value=_json_scalar(node),
                               param_type="float", unit=unit, pt_type=pt_type))
        return
    if isinstance(node, str):
        if node == NONE_VARIANT:
            # absent optional sub-model (cabin / hvac / *.thrml) -> not a parameter
            return
        short = _short_path(path, pt_type)
        options = metadata.enum_options(short, field_name, pt_type)
        param_type = "enum" if options else "str"
        out.append(_make_param(path=path, field_name=field_name, value=node,
                               param_type=param_type, unit=unit, pt_type=pt_type,
                               options=options))
        return
    if node is None:
        short = _short_path(path, pt_type)
        options = metadata.enum_options(short, field_name, pt_type)
        param_type = _infer_scalar_type(field_name, unit, options)
        out.append(_make_param(path=path, field_name=field_name, value=None,
                               param_type=param_type, unit=unit, pt_type=pt_type,
                               options=options))
        return


# --------------------------------------------------------------------------
# Public schema entry point
# --------------------------------------------------------------------------
def build_schema(preset_id: str | None = None) -> dict:
    """Full ``GET /api/schema`` payload."""
    resolved = resolve_preset_id(preset_id)
    veh_dict = load_vehicle_dict(resolved)
    return {
        "preset": resolved,
        "presets": list_presets(),
        "cycles": list_cycles(),
        "parameters": build_parameters(veh_dict),
    }
