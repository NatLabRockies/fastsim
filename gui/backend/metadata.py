"""
metadata.py -- static knowledge tables for FASTSim parameter introspection.

Everything here is *declarative*: unit suffixes, enum variant lists, group /
subgroup classification and human descriptions (EN + ZH).  Nothing in this
module touches fastsim itself, so it can be imported cheaply.
"""

from __future__ import annotations

# --------------------------------------------------------------------------
# 1. Unit suffixes  (CONTRACT.md section 1.1, rule 5)
# --------------------------------------------------------------------------
# Longest suffix first so that "_meters_per_second_squared" wins over
# "_meters_per_second" and "_watts_per_square_meter_kelvin" over "_watts".
UNIT_SUFFIXES: tuple[tuple[str, str], ...] = (
    ("_watts_per_square_meter_kelvin", "W/(m\u00b2\u00b7K)"),
    ("_kilogram_square_meters", "kg\u00b7m\u00b2"),
    ("_kilograms_per_cubic_meter", "kg/m\u00b3"),
    ("_meters_per_second_squared", "m/s\u00b2"),
    ("_joules_per_kilogram", "J/kg"),
    ("_watts_per_kilogram", "W/kg"),
    ("_joules_per_kelvin", "J/K"),
    ("_watts_per_kelvin", "W/K"),
    ("_meters_per_second", "m/s"),
    ("_square_meters", "m\u00b2"),
    ("_cubic_meters", "m\u00b3"),
    ("_cubic_meter", "m\u00b3"),
    ("_kilograms", "kg"),
    ("_seconds", "s"),
    ("_meters", "m"),
    ("_joules", "J"),
    ("_watts", "W"),
    ("_kelvin", "K"),
)

NO_UNIT = "-"


def parse_unit(name: str) -> str:
    """Return the unit implied by a field-name suffix, or '-'."""
    for suffix, unit in UNIT_SUFFIXES:
        if name.endswith(suffix):
            return unit
    return NO_UNIT


def strip_unit_suffix(name: str) -> str:
    for suffix, _unit in UNIT_SUFFIXES:
        if name.endswith(suffix):
            return name[: -len(suffix)]
    return name


# --------------------------------------------------------------------------
# 2. Enum variant tables
# --------------------------------------------------------------------------
# Variant lists were read out of fastsim 3.1.0 itself: feeding an invalid
# string through Vehicle.from_dict() makes the Rust serde layer answer with
# the complete `expected one of ...` list.  Keys are tried against the
# "short path" (powertrain segment removed), then the bare field name.
ENUM_TABLES: dict[str, list[str]] = {
    # chassis
    "chassis.drive_type": ["RWD", "FWD", "AWD", "4WD", "FourWD"],
    # fuel storage
    "fs.fuel_type": ["Gasoline", "Diesel", "Hydrogen"],
    "fuel_type": ["Gasoline", "Diesel", "Hydrogen"],
    # controllers
    "pt_cntrl": ["Normal", "StartStop", "StopStart"],
    "aux_cntrl": ["AuxOnResPriority", "AuxOnFcPriority"],
    # HVAC
    "heat_source": ["FuelConverter", "ResistanceHeater", "HeatPump"],
    "cabin_heat_source": ["FuelConverter", "ResistanceHeater", "HeatPump"],
    "res_heat_source": ["ResistanceHeater", "HeatPump", "None"],
    "res_cooling_source": ["HVAC", "None"],
    # interpolator internals (read-only, shown for completeness)
    "strategy": ["Linear", "Step"],
    "extrapolate": ["Enable", "Fill", "Clamp", "Wrap", "Error"],
    # SimParams (not part of Vehicle.to_dict, kept for completeness)
    "trace_miss_opts": ["Error", "Allow", "AllowChecked", "Correct"],
    "dfco_cntrl.dfco_enabled": [],  # bool, not an enum -- deliberate placeholder
}

# Powertrain-specific overrides for `pt_cntrl` (the active controller variant).
PT_CNTRL_OPTIONS: dict[str, list[str]] = {
    "Conv": ["Normal", "StartStop", "StopStart"],
    "HEV": ["RGWDB", "Normal", "StartStop", "StopStart"],
    "PHEV": ["RGWDB", "Normal", "StartStop", "StopStart"],
    "BEV": ["Normal"],
}

# Enums that must never be offered for overriding: fastsim expects a *newtype*
# (struct) variant here, so a bare string round-trip through Vehicle.from_dict
# raises `invalid type: unit variant, expected newtype variant`.
READONLY_ENUMS = {"pt_cntrl", "strategy", "extrapolate"}


def enum_options(short_path: str, basename: str, pt_type: str | None = None) -> list[str] | None:
    """Best-effort enum variant list for a field, or None when not an enum."""
    if basename in ("pt_cntrl",) or short_path in ("pt_cntrl",):
        return list(PT_CNTRL_OPTIONS.get(pt_type or "", ENUM_TABLES["pt_cntrl"]))
    options = ENUM_TABLES.get(short_path)
    if options is None:
        options = ENUM_TABLES.get(basename)
    if not options:
        return None
    return list(options)


def is_readonly_enum(basename: str) -> bool:
    return basename in READONLY_ENUMS


# --------------------------------------------------------------------------
# 3. Group / subgroup classification (CONTRACT.md section 1.1, rule 6)
# --------------------------------------------------------------------------
GROUP_BODY = ("Vehicle", "\u6574\u8f66", "Body", "\u8f66\u8eab")
GROUP_IDENTITY = ("Vehicle", "\u6574\u8f66", "Identity", "\u8eab\u4efd\u4fe1\u606f")
GROUP_PT_OTHER = ("Powertrain", "\u52a8\u529b\u603b\u6210", "Other", "\u5176\u4ed6")

_CHASSIS_AERO = ("Chassis", "\u5e95\u76d8", "Aero", "\u7a7a\u6c14\u52a8\u529b\u5b66")
_CHASSIS_ROLL = ("Chassis", "\u5e95\u76d8", "Rolling & Traction", "\u6eda\u963b\u4e0e\u9644\u7740")
_CHASSIS_WHEELS = ("Chassis", "\u5e95\u76d8", "Wheels", "\u8f66\u8f6e")
_CHASSIS_DIM = ("Chassis", "\u5e95\u76d8", "Dimensions & Mass", "\u5c3a\u5bf8\u4e0e\u8d28\u91cf")
_CHASSIS_OTHER = ("Chassis", "\u5e95\u76d8", "Other", "\u5176\u4ed6")

_PT = "\u52a8\u529b\u603b\u6210"  # 动力总成
_THERMAL = ("Thermal", "\u70ed\u7ba1\u7406", "Cabin & HVAC", "\u5ea7\u8231\u4e0e\u7a7a\u8c03")
_THERMAL_FC = ("Thermal", "\u70ed\u7ba1\u7406", "Fuel Converter Thermal", "\u53d1\u52a8\u673a\u70ed\u7ba1\u7406")
_THERMAL_RES = ("Thermal", "\u70ed\u7ba1\u7406", "Battery Thermal", "\u7535\u6c60\u70ed\u7ba1\u7406")


def classify(path: str) -> tuple[str, str, str, str]:
    """Return (group, group_zh, subgroup, subgroup_zh) for a dotted path."""
    if path in ("mass_kilograms", "pwr_aux_base_watts"):
        return GROUP_BODY
    if path in ("name", "year", "make", "model", "trim"):
        return GROUP_IDENTITY

    # --- Thermal -----------------------------------------------------
    if path.startswith(("cabin.", "hvac.")) or path in ("cabin", "hvac"):
        return _THERMAL
    if "fc.thrml." in path or ".fc.thrml." in path:
        return _THERMAL_FC
    if "res.thrml." in path or ".res.thrml." in path:
        return _THERMAL_RES
    if ".thrml." in path:
        return _THERMAL_RES

    # --- Chassis -----------------------------------------------------
    if path.startswith("chassis."):
        leaf = path[len("chassis."):]
        if leaf.startswith(("drag_coef", "frontal_area")):
            return _CHASSIS_AERO
        if leaf.startswith(("wheel_rr_coef", "wheel_fric_coef")):
            return _CHASSIS_ROLL
        if leaf.startswith(("wheel_radius", "wheel_inertia", "num_wheels", "tire_code")):
            return _CHASSIS_WHEELS
        if leaf.startswith((
            "wheel_base", "cg_height", "drive_", "glider_mass", "cargo_mass", "mass_kilograms",
        )):
            return _CHASSIS_DIM
        return _CHASSIS_OTHER

    # --- Powertrain --------------------------------------------------
    if path.startswith("pt_type."):
        parts = path.split(".")
        rest = parts[2:] if len(parts) > 2 else []
        head = rest[0] if rest else ""
        tail = ".".join(rest)
        if head == "fs":
            return ("Powertrain", _PT, "Fuel Storage", "\u71c3\u6cb9\u5b58\u50a8")
        if head == "fc":
            return ("Powertrain", _PT, "Fuel Converter", "\u53d1\u52a8\u673a")
        if head == "em":
            return ("Powertrain", _PT, "Electric Machine", "\u7535\u673a")
        if head == "res":
            return ("Powertrain", _PT, "Battery (ESS)", "\u7535\u6c60\uff08\u50a8\u80fd\uff09")
        if head == "transmission":
            return ("Powertrain", _PT, "Transmission", "\u4f20\u52a8\u7cfb\u7edf")
        if head in ("pt_cntrl", "dfco_cntrl", "alt_eff", "aux_cntrl", "sim_params"):
            return ("Powertrain", _PT, "Controls", "\u63a7\u5236\u4e0e\u7b56\u7565")
        if tail.startswith(("pt_cntrl", "dfco_cntrl", "sim_params")):
            return ("Powertrain", _PT, "Controls", "\u63a7\u5236\u4e0e\u7b56\u7565")
        return GROUP_PT_OTHER

    return GROUP_PT_OTHER


# --------------------------------------------------------------------------
# 4. Descriptions (EN + ZH)
# --------------------------------------------------------------------------
# Keyed by full path, then "short path" (powertrain segment stripped), then
# bare field name.  Long tail falls back to a generated description.
DESCRIPTIONS: dict[str, tuple[str, str]] = {
    # ---- Vehicle body ------------------------------------------------
    "mass_kilograms": (
        "Total vehicle test mass: glider + cargo + powertrain.",
        "\u6574\u8f66\u8bd5\u9a8c\u8d28\u91cf\uff1a\u8f66\u8eab + \u8f7d\u8377 + \u52a8\u529b\u603b\u6210\u3002",
    ),
    "pwr_aux_base_watts": (
        "Constant base auxiliary power draw (accessories).",
        "\u57fa\u7840\u9644\u4ef6\u529f\u7387\uff08\u5e38\u9a7b\u8d1f\u8f7d\uff09\u3002",
    ),
    "name": ("Vehicle display name.", "\u8f66\u578b\u663e\u793a\u540d\u79f0\u3002"),
    "year": ("Model year.", "\u5e74\u6b3e\u3002"),
    "make": ("Manufacturer.", "\u5382\u5546\u3002"),
    "model": ("Model name.", "\u578b\u53f7\u3002"),
    "trim": ("Trim / variant.", "\u914d\u7f6e\u7248\u672c\u3002"),

    # ---- Chassis -----------------------------------------------------
    "chassis.drag_coef": (
        "Aerodynamic drag coefficient.",
        "\u7a7a\u6c14\u963b\u529b\u7cfb\u6570\u3002",
    ),
    "chassis.frontal_area_square_meters": (
        "Frontal reference area for aerodynamic drag.",
        "\u8fce\u98ce\u9762\u79ef\uff08\u7a7a\u6c14\u963b\u529b\u8ba1\u7b97\u7528\uff09\u3002",
    ),
    "chassis.wheel_rr_coef": (
        "Rolling resistance coefficient (independent of speed).",
        "\u6eda\u52a8\u963b\u529b\u7cfb\u6570\uff08\u4e0d\u968f\u8f66\u901f\u53d8\u5316\uff09\u3002",
    ),
    "chassis.wheel_fric_coef": (
        "Tire-road friction coefficient; caps achievable traction.",
        "\u8f6e\u80ce-\u8def\u9762\u9644\u7740\u7cfb\u6570\uff0c\u9650\u5236\u6700\u5927\u9a71\u52a8\u529b\u3002",
    ),
    "chassis.wheel_radius_meters": (
        "Loaded wheel radius used for wheel speed / force conversion.",
        "\u8f66\u8f6e\u534a\u5f84\uff08\u8f6e\u901f\u4e0e\u9a71\u52a8\u529b\u6362\u7b97\u7528\uff09\u3002",
    ),
    "chassis.wheel_inertia_kilogram_square_meters": (
        "Rotational inertia of one wheel.",
        "\u5355\u4e2a\u8f66\u8f6e\u7684\u8f6c\u52a8\u60ef\u91cf\u3002",
    ),
    "chassis.num_wheels": (
        "Number of wheels (total inertia scaling).",
        "\u8f66\u8f6e\u6570\u91cf\uff08\u7528\u4e8e\u8f6e\u7cfb\u60ef\u91cf\u7f29\u653e\uff09\u3002",
    ),
    "chassis.tire_code": (
        "Optional tire identifier (informational).",
        "\u53ef\u9009\u7684\u8f6e\u80ce\u578b\u53f7\uff08\u4ec5\u4f5c\u6807\u8bc6\uff09\u3002",
    ),
    "chassis.wheel_base_meters": (
        "Wheelbase; used for axle load transfer.",
        "\u8f74\u8ddd\uff0c\u7528\u4e8e\u8f74\u8377\u8f6c\u79fb\u8ba1\u7b97\u3002",
    ),
    "chassis.cg_height_meters": (
        "Center-of-gravity height above ground; drives load transfer under acceleration.",
        "\u8d28\u5fc3\u9ad8\u5ea6\uff0c\u51b3\u5b9a\u52a0\u901f\u65f6\u7684\u8f74\u8377\u8f6c\u79fb\u3002",
    ),
    "chassis.drive_type": (
        "Driven axle layout (FWD / RWD / AWD).",
        "\u9a71\u52a8\u5f62\u5f0f\uff08\u524d\u9a71 / \u540e\u9a71 / \u56db\u9a71\uff09\u3002",
    ),
    "chassis.drive_axle_weight_frac": (
        "Static fraction of vehicle weight on the driven axle.",
        "\u7a33\u6001\u65f6\u9a71\u52a8\u8f74\u6240\u627f\u62c5\u7684\u91cd\u91cf\u5360\u6bd4\u3002",
    ),
    "chassis.glider_mass_kilograms": (
        "Glider mass (body, running gear) excluding the powertrain.",
        "\u6ed1\u884c\u4f53\u8d28\u91cf\uff08\u8f66\u8eab\u4e0e\u884c\u8d70\u7cfb\uff0c\u4e0d\u542b\u52a8\u529b\u603b\u6210\uff09\u3002",
    ),
    "chassis.cargo_mass_kilograms": (
        "Cargo / payload mass.",
        "\u8f7d\u8d27\uff08\u4e58\u5458\u4e0e\u884c\u674e\uff09\u8d28\u91cf\u3002",
    ),
    "chassis.mass_kilograms": (
        "Optional direct chassis mass override.",
        "\u53ef\u9009\u7684\u5e95\u76d8\u8d28\u91cf\u76f4\u63a5\u8986\u76d6\u503c\u3002",
    ),

    # ---- Fuel storage ------------------------------------------------
    "fs.pwr_out_max_watts": (
        "Maximum power the fuel storage can deliver.",
        "\u71c3\u6cb9\u5b58\u50a8\u7684\u6700\u5927\u8f93\u51fa\u529f\u7387\u3002",
    ),
    "fs.pwr_ramp_lag_seconds": (
        "Fuel storage power ramp lag time constant.",
        "\u71c3\u6cb9\u5b58\u50a8\u529f\u7387\u722c\u5347\u6ede\u540e\u65f6\u95f4\u5e38\u6570\u3002",
    ),
    "fs.energy_capacity_joules": (
        "Usable energy capacity of the fuel tank.",
        "\u71c3\u6cb9\u7bb1\u7684\u53ef\u7528\u80fd\u91cf\u5bb9\u91cf\u3002",
    ),
    "fs.specific_energy_joules_per_kilogram": (
        "Fuel specific energy (lower heating value).",
        "\u71c3\u6599\u6bd4\u80fd\u91cf\uff08\u4f4e\u70ed\u503c\uff09\u3002",
    ),
    "fs.mass_kilograms": (
        "Mass of the full fuel tank content.",
        "\u6ee1\u7bb1\u71c3\u6599\u7684\u8d28\u91cf\u3002",
    ),
    "fs.fuel_type": (
        "Fuel type (affects energy content / emissions bookkeeping).",
        "\u71c3\u6599\u7c7b\u578b\uff08\u5f71\u54cd\u80fd\u91cf\u4e0e\u6392\u653e\u8ba1\u91cf\uff09\u3002",
    ),

    # ---- Fuel converter (engine) -------------------------------------
    "fc.pwr_out_max_watts": (
        "Peak mechanical output power of the fuel converter.",
        "\u53d1\u52a8\u673a\u5cf0\u503c\u673a\u68b0\u8f93\u51fa\u529f\u7387\u3002",
    ),
    "fc.pwr_out_max_init_watts": (
        "Output power available before the engine is fully warm.",
        "\u53d1\u52a8\u673a\u672a\u5b8c\u5168\u6696\u673a\u65f6\u53ef\u7528\u7684\u8f93\u51fa\u529f\u7387\u3002",
    ),
    "fc.pwr_ramp_lag_seconds": (
        "Engine power ramp lag time constant.",
        "\u53d1\u52a8\u673a\u529f\u7387\u722c\u5347\u6ede\u540e\u65f6\u95f4\u5e38\u6570\u3002",
    ),
    "fc.pwr_idle_fuel_watts": (
        "Fuel power consumed while idling (engine on, no output).",
        "\u6020\u901f\u71c3\u6cb9\u529f\u7387\uff08\u53d1\u52a8\u673a\u8fd0\u8f6c\u4f46\u4e0d\u8f93\u51fa\u529f\u7387\uff09\u3002",
    ),
    "fc.eff_interp_from_pwr_out": (
        "Engine efficiency curve vs. fraction of peak output power (read-only curve).",
        "\u53d1\u52a8\u673a\u6548\u7387\u968f\u529f\u7387\u5360\u6bd4\u53d8\u5316\u7684\u63d2\u503c\u66f2\u7ebf\uff08\u53ea\u8bfb\uff09\u3002",
    ),
    "fc.mass_kilograms": (
        "Fuel converter mass.",
        "\u53d1\u52a8\u673a\u8d28\u91cf\u3002",
    ),
    "fc.specific_pwr_watts_per_kilogram": (
        "Fuel converter specific power.",
        "\u53d1\u52a8\u673a\u529f\u7387\u5bc6\u5ea6\u3002",
    ),
    "fc.thrml": (
        "Engine thermal model (absent when the preset has no engine thermal network).",
        "\u53d1\u52a8\u673a\u70ed\u6a21\u578b\uff08\u9884\u8bbe\u65e0\u70ed\u7f51\u7edc\u65f6\u4e0d\u5b58\u5728\uff09\u3002",
    ),

    # ---- Electric machine --------------------------------------------
    "em.pwr_out_max_watts": (
        "Peak mechanical output power of the electric machine.",
        "\u7535\u673a\u5cf0\u503c\u673a\u68b0\u8f93\u51fa\u529f\u7387\u3002",
    ),
    "em.eff_interp_achieved": (
        "Motor efficiency map as achieved (read-only curve).",
        "\u7535\u673a\u5b9e\u9645\u6548\u7387\u56fe\uff08\u53ea\u8bfb\u66f2\u7ebf\uff09\u3002",
    ),
    "em.eff_interp_at_max_input": (
        "Motor efficiency map at maximum input power (read-only curve).",
        "\u7535\u673a\u6ee1\u8f93\u5165\u529f\u7387\u4e0b\u7684\u6548\u7387\u56fe\uff08\u53ea\u8bfb\u66f2\u7ebf\uff09\u3002",
    ),
    "em.mass_kilograms": (
        "Electric machine mass.",
        "\u7535\u673a\u8d28\u91cf\u3002",
    ),
    "em.specific_pwr_watts_per_kilogram": (
        "Electric machine specific power.",
        "\u7535\u673a\u529f\u7387\u5bc6\u5ea6\u3002",
    ),

    # ---- Battery -----------------------------------------------------
    "res.energy_capacity_joules": (
        "Usable battery energy capacity.",
        "\u7535\u6c60\u53ef\u7528\u80fd\u91cf\u5bb9\u91cf\u3002",
    ),
    "res.pwr_out_max_watts": (
        "Battery maximum electrical output power.",
        "\u7535\u6c60\u6700\u5927\u8f93\u51fa\uff08\u653e\u7535\uff09\u529f\u7387\u3002",
    ),
    "res.min_soc": (
        "Minimum allowed state of charge (fraction of capacity).",
        "\u5141\u8bb8\u7684\u6700\u4f4e SOC\uff08\u5360\u5bb9\u91cf\u6bd4\u4f8b\uff09\u3002",
    ),
    "res.max_soc": (
        "Maximum allowed state of charge (fraction of capacity).",
        "\u5141\u8bb8\u7684\u6700\u9ad8 SOC\uff08\u5360\u5bb9\u91cf\u6bd4\u4f8b\uff09\u3002",
    ),
    "res.eff_interp": (
        "Battery efficiency model: a constant, or an efficiency map (read-only curve).",
        "\u7535\u6c60\u6548\u7387\u6a21\u578b\uff1a\u5e38\u6570\u6216\u6548\u7387\u56fe\uff08\u53ea\u8bfb\u66f2\u7ebf\uff09\u3002",
    ),
    "res.mass_kilograms": (
        "Battery pack mass.",
        "\u7535\u6c60\u5305\u8d28\u91cf\u3002",
    ),
    "res.specific_energy_joules_per_kilogram": (
        "Battery specific energy.",
        "\u7535\u6c60\u6bd4\u80fd\u91cf\u3002",
    ),
    "res.thrml": (
        "Battery thermal model (absent when the preset has no battery thermal network).",
        "\u7535\u6c60\u70ed\u6a21\u578b\uff08\u9884\u8bbe\u65e0\u70ed\u7f51\u7edc\u65f6\u4e0d\u5b58\u5728\uff09\u3002",
    ),

    # ---- Transmission -------------------------------------------------
    "transmission.eff_interp": (
        "Transmission (gearbox) efficiency.",
        "\u53d8\u901f\u5668\uff08\u4f20\u52a8\u7cfb\u7edf\uff09\u6548\u7387\u3002",
    ),
    "transmission.mass_kilograms": (
        "Transmission mass.",
        "\u4f20\u52a8\u7cfb\u7edf\u8d28\u91cf\u3002",
    ),

    # ---- Controls -----------------------------------------------------
    "pt_cntrl": (
        "Active powertrain controller variant (read-only: switching variants needs the variant's own parameters).",
        "\u5f53\u524d\u542f\u7528\u7684\u52a8\u529b\u603b\u6210\u63a7\u5236\u5668\uff08\u53ea\u8bfb\uff1a\u5207\u6362\u53d8\u4f53\u9700\u8981\u5176\u5bf9\u5e94\u53c2\u6570\uff09\u3002",
    ),
    "alt_eff": (
        "Alternator efficiency for conventional powertrains.",
        "\u4f20\u7edf\u8f66\u7528\u53d1\u7535\u673a\u6548\u7387\u3002",
    ),
    "aux_cntrl": (
        "Auxiliary load priority controller for hybrid powertrains.",
        "\u6df7\u52a8\u52a8\u529b\u603b\u6210\u7684\u9644\u4ef6\u8d1f\u8f7d\u4f18\u5148\u7b56\u7565\u3002",
    ),
    "dfco_cntrl.dfco_enabled": (
        "Enable deceleration fuel cut-off.",
        "\u662f\u5426\u542f\u7528\u51cf\u901f\u65ad\u6cb9\uff08DFCO\uff09\u3002",
    ),
    "dfco_cntrl.minimum_dfco_speed_meters_per_second": (
        "Minimum vehicle speed at which DFCO may engage.",
        "\u65ad\u6cb9\u751f\u6548\u7684\u6700\u4f4e\u8f66\u901f\u3002",
    ),
    "dfco_cntrl.minimum_dfco_deceleration_meters_per_second_squared": (
        "Minimum (most negative) deceleration required to keep DFCO engaged.",
        "\u7ef4\u6301\u65ad\u6cb9\u6240\u9700\u7684\u6700\u5c0f\uff08\u6700\u8d1f\uff09\u51cf\u901f\u5ea6\u3002",
    ),
    "dfco_cntrl.stopped_speed_threshold_meters_per_second": (
        "Speed below which the vehicle is treated as stopped.",
        "\u5224\u5b9a\u8f66\u8f86\u4e3a\u9759\u6b62\u7684\u8f66\u901f\u9608\u503c\u3002",
    ),
    "pt_cntrl.StartStop.fc_min_time_on_seconds": (
        "Minimum time the engine must stay on once started.",
        "\u53d1\u52a8\u673a\u542f\u52a8\u540e\u5fc5\u987b\u4fdd\u6301\u8fd0\u8f6c\u7684\u6700\u77ed\u65f6\u95f4\u3002",
    ),
    "pt_cntrl.StartStop.temp_fc_allowed_off_kelvin": (
        "Coolant temperature above which the engine may be shut off.",
        "\u51b7\u5374\u6db2\u6e29\u5ea6\u9ad8\u4e8e\u6b64\u503c\u65f6\u53d1\u52a8\u673a\u5141\u8bb8\u505c\u673a\u3002",
    ),
    "pt_cntrl.StartStop.temp_fc_forced_on_kelvin": (
        "Coolant temperature below which the engine is forced on.",
        "\u51b7\u5374\u6db2\u6e29\u5ea6\u4f4e\u4e8e\u6b64\u503c\u65f6\u5f3a\u5236\u542f\u52a8\u53d1\u52a8\u673a\u3002",
    ),
    "pt_cntrl.StartStop.time_delay_after_stop_until_fc_can_turn_off_seconds": (
        "Delay after the vehicle stops before the engine may turn off.",
        "\u8f66\u8f86\u505c\u4e0b\u540e\u9700\u5ef6\u8fdf\u591a\u4e45\u53d1\u52a8\u673a\u624d\u80fd\u505c\u673a\u3002",
    ),
    "pt_cntrl.StartStop.stopped_speed_threshold_meters_per_second": (
        "Speed below which the vehicle counts as stopped.",
        "\u8ba1\u5165\u201c\u505c\u8f66\u201d\u72b6\u6001\u7684\u8f66\u901f\u9608\u503c\u3002",
    ),
    "pt_cntrl.RGWDB.frac_of_most_eff_pwr_to_run_fc": (
        "Fraction of the engine's best-efficiency power at which it is run (hybrid).",
        "\u6df7\u52a8\u5f15\u64ce\u5de5\u4f5c\u70b9\u53d6\u5176\u6700\u4f73\u6548\u7387\u529f\u7387\u7684\u6bd4\u4f8b\u3002",
    ),
    "pt_cntrl.RGWDB.frac_pwr_demand_fc_forced_on": (
        "Power demand fraction above which the engine is forced on.",
        "\u9700\u6c42\u529f\u7387\u8d85\u6b64\u6bd4\u4f8b\u65f6\u5f3a\u5236\u542f\u52a8\u53d1\u52a8\u673a\u3002",
    ),
    "pt_cntrl.RGWDB.fc_min_time_on_seconds": (
        "Minimum engine on time for the hybrid controller.",
        "\u6df7\u52a8\u63a7\u5236\u5668\u4e0b\u53d1\u52a8\u673a\u6700\u77ed\u8fd0\u8f6c\u65f6\u95f4\u3002",
    ),
    "sim_params.balance_soc": (
        "Balance the battery SOC between start and end of the cycle (hybrid).",
        "\u5728\u5de5\u51b5\u8d77\u6b62\u4e4b\u95f4\u5e73\u8861\u7535\u6c60 SOC\uff08\u6df7\u52a8\uff09\u3002",
    ),
    "sim_params.soc_balance_iter_err": (
        "Allowed SOC imbalance error for the balance iterations (% or points, per preset).",
        "SOC \u5e73\u8861\u8fed\u4ee3\u7684\u5141\u8bb8\u8bef\u5dee\u3002",
    ),
    "sim_params.res_per_fuel_lim": (
        "Battery-energy-per-fuel-energy limit used while balancing SOC.",
        "\u5e73\u8861 SOC \u65f6\u4f7f\u7528\u7684\u201c\u7535\u80fd/\u71c3\u6cb9\u80fd\u201d\u6bd4\u4f8b\u9650\u503c\u3002",
    ),
    "sim_params.save_soc_bal_iters": (
        "Record the SOC-balance iteration history.",
        "\u662f\u5426\u8bb0\u5f55 SOC \u5e73\u8861\u8fed\u4ee3\u5386\u53f2\u3002",
    ),
    "pt_cntrl.RGWDB.speed_fc_forced_on_meters_per_second": (
        "Speed above which the engine is forced on (hybrid).",
        "\u8f66\u901f\u9ad8\u4e8e\u6b64\u503c\u65f6\u5f3a\u5236\u542f\u52a8\u53d1\u52a8\u673a\uff08\u6df7\u52a8\uff09\u3002",
    ),
    "pt_cntrl.RGWDB.speed_soc_fc_on_buffer_meters_per_second": (
        "SOC-buffer speed threshold that turns the engine on.",
        "\u89e6\u53d1\u53d1\u52a8\u673a\u542f\u52a8\u7684 SOC \u7f13\u51b2\u8f66\u901f\u9608\u503c\u3002",
    ),
    "pt_cntrl.RGWDB.speed_soc_disch_buffer_meters_per_second": (
        "SOC-buffer speed threshold that allows more discharge.",
        "\u5141\u8bb8\u66f4\u591a\u653e\u7535\u7684 SOC \u7f13\u51b2\u8f66\u901f\u9608\u503c\u3002",
    ),
    "pt_cntrl.RGWDB.speed_soc_regen_buffer_meters_per_second": (
        "SOC-buffer speed threshold that reserves regen headroom.",
        "\u4e3a\u56de\u6536\u9884\u7559\u4f59\u91cf\u7684 SOC \u7f13\u51b2\u8f66\u901f\u9608\u503c\u3002",
    ),

    # ---- Thermal: cabin / HVAC ---------------------------------------
    "cabin.heat_capacitance_joules_per_kelvin": (
        "Lumped thermal capacitance of the cabin air + interior.",
        "\u5ea7\u8231\uff08\u7a7a\u6c14+\u5185\u9970\uff09\u7684\u96c6\u4e2d\u70ed\u5bb9\u3002",
    ),
    "cabin.cab_shell_htc_to_amb_watts_per_square_meter_kelvin": (
        "Cabin shell heat-transfer coefficient to ambient (driving).",
        "\u884c\u9a76\u65f6\u5ea7\u8231\u58f3\u4f53\u5230\u73af\u5883\u7684\u4f20\u70ed\u7cfb\u6570\u3002",
    ),
    "cabin.cab_htc_to_amb_stop_watts_per_square_meter_kelvin": (
        "Cabin shell heat-transfer coefficient to ambient when stopped.",
        "\u505c\u8f66\u65f6\u5ea7\u8231\u58f3\u4f53\u5230\u73af\u5883\u7684\u4f20\u70ed\u7cfb\u6570\u3002",
    ),
    "cabin.length_meters": (
        "Cabin length used for the shell surface area.",
        "\u5ea7\u8231\u957f\u5ea6\uff08\u7528\u4e8e\u8ba1\u7b97\u58f3\u4f53\u8868\u9762\u79ef\uff09\u3002",
    ),
    "cabin.width_meters": (
        "Cabin width used for the shell surface area.",
        "\u5ea7\u8231\u5bbd\u5ea6\uff08\u7528\u4e8e\u8ba1\u7b97\u58f3\u4f53\u8868\u9762\u79ef\uff09\u3002",
    ),
    "hvac.te_set_kelvin": (
        "Cabin HVAC temperature setpoint.",
        "\u5ea7\u8231\u7a7a\u8c03\u6e29\u5ea6\u8bbe\u5b9a\u503c\u3002",
    ),
    "hvac.te_set_cab_kelvin": (
        "Cabin HVAC temperature setpoint.",
        "\u5ea7\u8231\u7a7a\u8c03\u6e29\u5ea6\u8bbe\u5b9a\u503c\u3002",
    ),
    "hvac.te_set_res_kelvin": (
        "Battery HVAC temperature setpoint.",
        "\u7535\u6c60\u70ed\u7ba1\u7406\u6e29\u5ea6\u8bbe\u5b9a\u503c\u3002",
    ),
    "hvac.te_deadband_kelvin": (
        "Temperature deadband around the setpoint before HVAC reacts.",
        "\u7a7a\u8c03\u54cd\u5e94\u524d\u7684\u8bbe\u5b9a\u503c\u6b7b\u533a\u5e26\u3002",
    ),
    "hvac.te_deadband_cab_kelvin": (
        "Cabin temperature deadband around the setpoint.",
        "\u5ea7\u8231\u6e29\u5ea6\u8bbe\u5b9a\u503c\u6b7b\u533a\u5e26\u3002",
    ),
    "hvac.te_deadband_res_kelvin": (
        "Battery temperature deadband around the setpoint.",
        "\u7535\u6c60\u6e29\u5ea6\u8bbe\u5b9a\u503c\u6b7b\u533a\u5e26\u3002",
    ),
    "hvac.p_watts_per_kelvin": (
        "Proportional gain of the HVAC controller (W/K).",
        "\u7a7a\u8c03\u63a7\u5236\u5668\u7684\u6bd4\u4f8b\u589e\u76ca\uff08W/K\uff09\u3002",
    ),
    "hvac.p_cabin_watts_per_kelvin": (
        "Cabin-loop proportional gain of the HVAC controller.",
        "\u5ea7\u8231\u56de\u8def\u7a7a\u8c03\u6bd4\u4f8b\u589e\u76ca\u3002",
    ),
    "hvac.p_res_watts_per_kelvin": (
        "Battery-loop proportional gain of the HVAC controller.",
        "\u7535\u6c60\u56de\u8def\u7a7a\u8c03\u6bd4\u4f8b\u589e\u76ca\u3002",
    ),
    "hvac.pwr_thrml_max_watts": (
        "Maximum thermal power the HVAC unit can deliver.",
        "\u7a7a\u8c03\u53ef\u63d0\u4f9b\u7684\u6700\u5927\u70ed\u529f\u7387\u3002",
    ),
    "hvac.pwr_aux_for_hvac_max_watts": (
        "Maximum electrical auxiliary power the HVAC may draw.",
        "\u7a7a\u8c03\u53ef\u6d88\u8017\u7684\u6700\u5927\u7535\u529f\u7387\u3002",
    ),
    "hvac.frac_of_ideal_cop": (
        "Fraction of the ideal (Carnot) COP achieved by the heat pump.",
        "\u70ed\u6cf5\u5b9e\u9645 COP \u5360\u7406\u60f3\uff08\u5361\u8bfa\uff09COP \u7684\u6bd4\u4f8b\u3002",
    ),
    "hvac.heat_source": (
        "Heat source used by the cabin HVAC.",
        "\u5ea7\u8231\u7a7a\u8c03\u4f7f\u7528\u7684\u70ed\u6e90\u3002",
    ),
    "hvac.cabin_heat_source": (
        "Heat source used to warm the cabin.",
        "\u5ea7\u8231\u52a0\u70ed\u6240\u7528\u7684\u70ed\u6e90\u3002",
    ),
    "hvac.res_heat_source": (
        "Heat source used to warm the battery.",
        "\u7535\u6c60\u52a0\u70ed\u6240\u7528\u7684\u70ed\u6e90\u3002",
    ),
    "hvac.res_cooling_source": (
        "Cooling source used for the battery.",
        "\u7535\u6c60\u964d\u6e29\u6240\u7528\u7684\u51b7\u6e90\u3002",
    ),

    # ---- Thermal: engine / battery -----------------------------------
    "thrml.heat_capacitance_joules_per_kelvin": (
        "Lumped thermal capacitance of the component.",
        "\u90e8\u4ef6\u7684\u96c6\u4e2d\u70ed\u5bb9\u3002",
    ),
    "thrml.conductance_to_amb_watts_per_kelvin": (
        "Thermal conductance from the component to ambient.",
        "\u90e8\u4ef6\u5230\u73af\u5883\u7684\u70ed\u5bfc\u3002",
    ),
    "thrml.conductance_to_cab_watts_per_kelvin": (
        "Thermal conductance between the battery and the cabin.",
        "\u7535\u6c60\u4e0e\u5ea7\u8231\u4e4b\u95f4\u7684\u70ed\u5bfc\u3002",
    ),
    "thrml.conductance_from_comb_watts_per_kelvin": (
        "Thermal conductance from combustion into the engine block.",
        "\u71c3\u70e7\u70ed\u5230\u53d1\u52a8\u673a\u672c\u4f53\u7684\u70ed\u5bfc\u3002",
    ),
    "thrml.max_frac_from_comb": (
        "Maximum fraction of combustion heat that can enter the block.",
        "\u53ef\u8fdb\u5165\u53d1\u52a8\u673a\u672c\u4f53\u7684\u71c3\u70e7\u70ed\u6700\u5927\u5360\u6bd4\u3002",
    ),
    "thrml.tstat_te_sto_kelvin": (
        "Thermostat setpoint temperature.",
        "\u8282\u6e29\u5668\u8bbe\u5b9a\u6e29\u5ea6\u3002",
    ),
    "thrml.tstat_te_delta_kelvin": (
        "Thermostat deadband (half-width) around the setpoint.",
        "\u8282\u6e29\u5668\u6e29\u5ea6\u6b7b\u533a\uff08\u534a\u5bbd\uff09\u3002",
    ),
    "thrml.radiator_effectiveness": (
        "Radiator heat-rejection effectiveness.",
        "\u6563\u70ed\u5668\u6362\u70ed\u6548\u7387\u3002",
    ),
    "thrml.length_for_convection_meters": (
        "Characteristic length for the convection correlation.",
        "\u5bf9\u6d41\u6362\u70ed\u5173\u8054\u5f0f\u7684\u7279\u5f81\u957f\u5ea6\u3002",
    ),
    "thrml.htc_to_amb_stop_watts_per_square_meter_kelvin": (
        "Heat-transfer coefficient to ambient when the vehicle is stopped.",
        "\u505c\u8f66\u65f6\u5230\u73af\u5883\u7684\u4f20\u70ed\u7cfb\u6570\u3002",
    ),
    "thrml.fc_eff_model": (
        "Cold-start engine efficiency penalty model.",
        "\u51b7\u542f\u52a8\u65f6\u53d1\u52a8\u673a\u6548\u7387\u60e9\u7f5a\u6a21\u578b\u3002",
    ),
    "Exponential.offset": (
        "Temperature offset of the cold-start efficiency penalty.",
        "\u51b7\u542f\u52a8\u6548\u7387\u60e9\u7f5a\u7684\u6e29\u5ea6\u504f\u79fb\u91cf\u3002",
    ),
    "Exponential.lag": (
        "Time/temperature lag of the cold-start efficiency penalty.",
        "\u51b7\u542f\u52a8\u6548\u7387\u60e9\u7f5a\u7684\u6ede\u540e\u5e38\u6570\u3002",
    ),
    "Exponential.minimum": (
        "Floor of the cold-start efficiency multiplier.",
        "\u51b7\u542f\u52a8\u6548\u7387\u4e58\u5b50\u7684\u4e0b\u9650\u3002",
    ),

    # ---- Curves / interpolator variants ------------------------------
    "Constant": (
        "Constant battery efficiency (no map: a single round-trip efficiency).",
        "\u5e38\u6570\u7535\u6c60\u6548\u7387\uff08\u65e0\u6548\u7387\u56fe\uff0c\u53ea\u7528\u4e00\u4e2a\u5f80\u8fd4\u6548\u7387\uff09\u3002",
    ),
    "eff_interp": (
        "Battery efficiency model: a constant or a C-rate / temperature map (read-only curve).",
        "\u7535\u6c60\u6548\u7387\u6a21\u578b\uff1a\u5e38\u6570\u6216\u968f C-rate/\u6e29\u5ea6\u53d8\u5316\u7684\u56fe\u8868\uff08\u53ea\u8bfb\uff09\u3002",
    ),
    "CRateTemperature": (
        "Battery efficiency map over C-rate and temperature (read-only curve).",
        "\u7535\u6c60\u6548\u7387\u968f C-rate \u4e0e\u6e29\u5ea6\u53d8\u5316\u7684\u56fe\u8868\uff08\u53ea\u8bfb\uff09\u3002",
    ),
    "tstat_interp": (
        "Thermostat opening curve vs. temperature (read-only curve).",
        "\u8282\u6e29\u5668\u5f00\u5ea6\u968f\u6e29\u5ea6\u53d8\u5316\u7684\u66f2\u7ebf\uff08\u53ea\u8bfb\uff09\u3002",
    ),
    "eff_interp_from_pwr_out": (
        "Efficiency curve vs. fraction of peak output power (read-only).",
        "\u6548\u7387\u968f\u529f\u7387\u5360\u6bd4\u53d8\u5316\u7684\u66f2\u7ebf\uff08\u53ea\u8bfb\uff09\u3002",
    ),
    "eff_interp_achieved": (
        "Efficiency map as achieved at the current operating point (read-only curve).",
        "\u5f53\u524d\u5de5\u4f5c\u70b9\u4e0b\u7684\u5b9e\u9645\u6548\u7387\u56fe\uff08\u53ea\u8bfb\uff09\u3002",
    ),
    "eff_interp_at_max_input": (
        "Efficiency map at maximum input power (read-only curve).",
        "\u6ee1\u8f93\u5165\u529f\u7387\u4e0b\u7684\u6548\u7387\u56fe\uff08\u53ea\u8bfb\uff09\u3002",
    ),

    # ---- HVAC controller gains ---------------------------------------
    "i": (
        "Integral gain of the HVAC controller.",
        "\u7a7a\u8c03\u63a7\u5236\u5668\u7684\u79ef\u5206\u589e\u76ca\u3002",
    ),
    "d": (
        "Derivative gain of the HVAC controller.",
        "\u7a7a\u8c03\u63a7\u5236\u5668\u7684\u5fae\u5206\u589e\u76ca\u3002",
    ),
    "i_cabin": (
        "Cabin-loop integral gain of the HVAC controller.",
        "\u5ea7\u8231\u56de\u8def\u7a7a\u8c03\u79ef\u5206\u589e\u76ca\u3002",
    ),
    "d_cabin": (
        "Cabin-loop derivative gain of the HVAC controller.",
        "\u5ea7\u8231\u56de\u8def\u7a7a\u8c03\u5fae\u5206\u589e\u76ca\u3002",
    ),
    "i_res": (
        "Battery-loop integral gain of the HVAC controller.",
        "\u7535\u6c60\u56de\u8def\u7a7a\u8c03\u79ef\u5206\u589e\u76ca\u3002",
    ),
    "d_res": (
        "Battery-loop derivative gain of the HVAC controller.",
        "\u7535\u6c60\u56de\u8def\u7a7a\u8c03\u5fae\u5206\u589e\u76ca\u3002",
    ),
    "pwr_i_max_watts": (
        "Upper limit on the HVAC integral term output.",
        "\u7a7a\u8c03\u79ef\u5206\u9879\u8f93\u51fa\u7684\u4e0a\u9650\u529f\u7387\u3002",
    ),
    "pwr_i_max_cabin_watts": (
        "Upper limit on the cabin HVAC integral term output.",
        "\u5ea7\u8231\u7a7a\u8c03\u79ef\u5206\u9879\u8f93\u51fa\u7684\u4e0a\u9650\u529f\u7387\u3002",
    ),
    "pwr_i_max_res_watts": (
        "Upper limit on the battery HVAC integral term output.",
        "\u7535\u6c60\u7a7a\u8c03\u79ef\u5206\u9879\u8f93\u51fa\u7684\u4e0a\u9650\u529f\u7387\u3002",
    ),
    "pwr_aux_for_hvac_cab_max_watts": (
        "Maximum electrical power the cabin HVAC may draw.",
        "\u5ea7\u8231\u7a7a\u8c03\u53ef\u6d88\u8017\u7684\u6700\u5927\u7535\u529f\u7387\u3002",
    ),
    "pwr_aux_for_hvac_res_max_watts": (
        "Maximum electrical power the battery HVAC may draw.",
        "\u7535\u6c60\u70ed\u7ba1\u7406\u53ef\u6d88\u8017\u7684\u6700\u5927\u7535\u529f\u7387\u3002",
    ),
    "te_set_cab_kelvin": (
        "Cabin temperature setpoint.",
        "\u5ea7\u8231\u6e29\u5ea6\u8bbe\u5b9a\u503c\u3002",
    ),
    "te_set_res_kelvin": (
        "Battery temperature setpoint.",
        "\u7535\u6c60\u6e29\u5ea6\u8bbe\u5b9a\u503c\u3002",
    ),
}


# --------------------------------------------------------------------------
# 5. Fallback description generation
# --------------------------------------------------------------------------
_TOKEN_EN = {
    "pwr": "power",
    "eff": "efficiency",
    "thrml": "thermal",
    "temp": "temperature",
    "te": "temperature",
    "amb": "ambient",
    "cab": "cabin",
    "res": "battery",
    "fc": "fuel converter",
    "em": "electric machine",
    "fs": "fuel storage",
    "htc": "heat transfer coefficient",
    "dist": "distance",
    "grad": "grade",
    "coef": "coefficient",
    "out": "output",
    "in": "input",
    "prop": "propulsive",
    "mech": "mechanical",
    "elec": "electrical",
    "aux": "auxiliary",
    "tstat": "thermostat",
    "frac": "fraction",
    "comb": "combustion",
    "init": "initial",
    "bal": "balance",
    "iter": "iteration",
    "err": "error",
    "lim": "limit",
    "soc": "state of charge",
    "fwd": "forward",
    "bwd": "backward",
    "dyn": "dynamic",
    "brake": "brake",
    "regen": "regenerative",
    "chrg": "charge",
    "disch": "discharge",
    "max": "maximum",
    "min": "minimum",
    "num": "number of",
    "rr": "rolling resistance",
    "fric": "friction",
    "cg": "center of gravity",
    "ce": "capacitance",
    "capacitance": "capacitance",
    "conductance": "conductance",
    "specific": "specific",
    "energy": "energy",
    "power": "power",
    "speed": "speed",
    "interp": "interpolated",
    "model": "model",
    "source": "source",
    "shell": "shell",
    "stop": "stopped",
    "sto": "setpoint",
    "deadband": "deadband",
    "crate": "C-rate",
    "temperature": "temperature",
    "buffer": "buffer",
    "coeff": "coefficient",
    "axle": "axle",
    "weight": "weight",
    "mass": "mass",
    "radius": "radius",
    "inertia": "inertia",
    "area": "area",
    "base": "base",
    "drag": "drag",
    "test": "test",
    "on": "on",
    "off": "off",
    "to": "to",
    "from": "from",
    "at": "at",
    "of": "of",
    "and": "and",
    "i": "integral gain",
    "d": "derivative gain",
    "p": "proportional gain",
    "set": "setpoint",
    "tire": "tire",
    "code": "code",
}

_TOKEN_ZH = {
    "pwr": "\u529f\u7387",
    "eff": "\u6548\u7387",
    "thrml": "\u70ed",
    "temp": "\u6e29\u5ea6",
    "te": "\u6e29\u5ea6",
    "amb": "\u73af\u5883",
    "cab": "\u5ea7\u8231",
    "res": "\u7535\u6c60",
    "fc": "\u53d1\u52a8\u673a",
    "em": "\u7535\u673a",
    "fs": "\u71c3\u6cb9\u5b58\u50a8",
    "dist": "\u8ddd\u79bb",
    "coef": "\u7cfb\u6570",
    "out": "\u8f93\u51fa",
    "in": "\u8f93\u5165",
    "prop": "\u9a71\u52a8",
    "mech": "\u673a\u68b0",
    "elec": "\u7535\u6c14",
    "aux": "\u9644\u4ef6",
    "frac": "\u5360\u6bd4",
    "soc": "SOC",
    "max": "\u6700\u5927",
    "min": "\u6700\u5c0f",
    "speed": "\u8f66\u901f",
    "energy": "\u80fd\u91cf",
    "power": "\u529f\u7387",
    "mass": "\u8d28\u91cf",
    "area": "\u9762\u79ef",
    "radius": "\u534a\u5f84",
    "inertia": "\u60ef\u91cf",
    "source": "\u6e90",
    "model": "\u6a21\u578b",
    "specific": "\u7279\u5b9a",
    "num": "\u6570\u91cf",
    "weight": "\u91cd\u91cf",
    "axle": "\u8f74",
}

_SKIP_WORDS = {"watts", "joules", "kelvin", "meters", "seconds", "kilograms", "square", "cubic", "per", "w", "j", "m", "s", "kg", "k"}


def generated_description(field_name: str, unit: str) -> tuple[str, str]:
    """Mechanical, but readable, description for long-tail fields."""
    stem = strip_unit_suffix(field_name)
    tokens = [t for t in stem.split("_") if t and t not in _SKIP_WORDS]
    en_words = [_TOKEN_EN.get(t, t) for t in tokens]
    en = " ".join(en_words) if en_words else field_name
    en = f"{en[0].upper()}{en[1:]}." if en else f"{field_name}."
    if unit != NO_UNIT:
        en = en.rstrip(".") + f"  Unit: {unit}."
    zh_tokens = [_TOKEN_ZH[t] for t in tokens if t in _TOKEN_ZH]
    zh = f"{''.join(zh_tokens)}\u3002" if zh_tokens else f"\u5b57\u6bb5 {field_name}\u3002"
    if unit != NO_UNIT:
        zh = zh.rstrip("\u3002") + f"\uff08{unit}\uff09\u3002"
    return en, zh


def describe(path: str, short_path: str, field_name: str, unit: str) -> tuple[str, str]:
    """Look up a hand-written description, else generate one."""
    for key in (path, short_path, field_name):
        if key in DESCRIPTIONS:
            return DESCRIPTIONS[key]
    return generated_description(field_name, unit)
