"""
export_code.py -- generate the flat, dependency-free Python script of
CONTRACT.md section 1.3.

The generated source is exactly what the API ran: same preset, same cycle,
only the user's overrides applied through ``Vehicle.to_dict()`` ->
``Vehicle.from_dict()``.
"""

from __future__ import annotations

import copy
import json

import introspection
import simulate


def _literal(value) -> str:
    """Python source literal for an override value."""
    if isinstance(value, bool):
        return "True" if value else "False"
    if isinstance(value, int):
        return str(value)
    if isinstance(value, float):
        return repr(float(value))
    if value is None:
        return "None"
    return json.dumps(str(value))


def _validate(preset_id: str, cycle_id: str, overrides: dict) -> dict:
    """Re-apply the overrides on a throw-away copy so export fails like simulate."""
    veh_dict = copy.deepcopy(introspection.load_vehicle_dict(preset_id))
    simulate.apply_overrides(veh_dict, overrides)
    return veh_dict


def generate(preset: str | None, cycle: str | None, overrides: dict | None = None) -> str:
    """Return the exported Python source for this configuration."""
    preset_id = introspection.resolve_preset_id(preset)
    cycle_id = introspection.resolve_cycle_id(cycle)
    overrides = overrides or {}
    if not isinstance(overrides, dict):
        raise simulate.InvalidOverrideError("`overrides` must be an object of path -> value")

    veh_dict = _validate(preset_id, cycle_id, overrides)
    pt_type = introspection.powertrain_key(veh_dict) or ""
    sub = (veh_dict.get("pt_type") or {}).get(pt_type) or {}
    has_fc = isinstance(sub.get("fc"), dict)
    has_res = isinstance(sub.get("res"), dict)
    vehicle_name = str(veh_dict.get("name") or preset_id)

    lines: list[str] = [
        "import fastsim",
        "",
        f"veh = fastsim.Vehicle.from_resource({json.dumps(preset_id)})",
        f"cyc = fastsim.Cycle.from_resource({json.dumps(cycle_id)})",
    ]

    if overrides:
        items = ", ".join(f"{_literal(k)}: {_literal(v)}" for k, v in overrides.items())
        lines += [
            "d = veh.to_dict()",
            f"for path, val in {{{items}}}.items():",
            '    node, keys = d, path.split(".")',
            "    for k in keys[:-1]:",
            "        node = node[k]",
            "    node[keys[-1]] = val",
            "veh = fastsim.Vehicle.from_dict(d)",
        ]

    lines.append("veh.set_save_interval(1)")
    lines += [
        "sim = fastsim.SimDrive(veh, cyc)",
        "sim.run()",
        "r = sim.to_dict(flatten=True)",
        f'P = "veh.pt_type.{pt_type}"',
        # same constant as simulate.py -- single source of truth
        f'mi = r["veh.state.dist_meters"] / {simulate.METERS_PER_MILE!r}',
    ]
    if has_res:
        # The post-init start SOC only exists on the SimDrive history, not on
        # the standalone Vehicle dict -- so read it from the first recorded step.
        lines.append(f'soc0 = sim.to_dataframe()[f"{{P}}.res.history.soc"].iloc[0]')
    lines.append(
        f'rows = [("Preset", {json.dumps(vehicle_name)}), ("Cycle", {json.dumps(cycle_id)}),'
        ' ("Distance", f"{mi:.3f} mi")]'
    )
    if has_fc:
        lines += [
            'kwh = r[f"{P}.fc.state.energy_fuel_joules"] / 3.6e6',
            'rows += [("Fuel energy", f"{kwh:.4f} kWh"),'
            ' ("Fuel economy", f"{mi / (kwh / 33.7):.2f} mpgge")]',
        ]
    if has_res:
        lines += [
            'ekwh = r[f"{P}.res.state.energy_out_electrical_joules"] / 3.6e6',
            'rows += [("Electricity", f"{ekwh:.4f} kWh"),'
            ' ("Electricity economy", f"{ekwh * 1000 / mi:.2f} Wh/mi")]',
            'soc1 = r[f"{P}.res.state.soc"]',
            'rows += [("SOC start", f"{soc0 * 100:.2f} %"),'
            ' ("SOC end", f"{soc1 * 100:.2f} %")]',
        ]
    lines += [
        "for key, val in rows:",
        '    print(f"{key:<22}{val}")',
    ]
    return "\n".join(lines) + "\n"
