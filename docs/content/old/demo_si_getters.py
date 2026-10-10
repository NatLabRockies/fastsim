"""FASTSim 3 stores physical quantities using the `uom` Rust crate, which
enforces dimensional correctness at compile time.  Previously, reading a
field value from Python required a serde round-trip (`to_pydict` → Python
dict → value).  The `serde_api` macro now auto-generates a `#[getter]`
property for every (field, unit) combination declared in the type system,
so values are available directly as Python properties.

The properties follow the naming pattern `{field_name}_{unit_plural}`,
e.g. `pwr_out_max_kilowatts`, `mass_kilograms`, `speed_meters_per_second`.
"""

# %%
import fastsim as fsim

# ── 1. Vehicle-level scalars ─────────────────────────────────────────────────

veh = fsim.Vehicle.from_resource("2016_TOYOTA_Prius_Two.yaml")

# `mass` is an `si::Mass`; two unit properties are auto-generated.
print("Vehicle mass:")
print(f"  {veh.mass_kilograms:.1f} kg")
print(f"  {veh.mass_pounds:.1f} lb")

# `pwr_aux_base` is an `si::Power`; watts, kilowatts, and horsepower variants.
print("\nAuxiliary base power:")
print(f"  {veh.pwr_aux_base_watts:.0f} W")
print(f"  {veh.pwr_aux_base_kilowatts:.3f} kW")
print(f"  {veh.pwr_aux_base_horsepower:.3f} hp")

# ── 2. Component scalars ──────────────────────────────────────────────────────

fc = veh.fc  # FuelConverter (present on HEV / CONV)

print("\nFuel converter:")
print(f"  max power  : {fc.pwr_out_max_kilowatts:.1f} kW  / {fc.pwr_out_max_horsepower:.1f} hp")
print(f"  idle fuel  : {fc.pwr_idle_fuel_watts:.1f} W  / {fc.pwr_idle_fuel_kilowatts:.3f} kW")
print(f"  ramp lag   : {fc.pwr_ramp_lag_seconds:.2f} s")
if fc.mass_kilograms is not None:
    print(f"  mass       : {fc.mass_kilograms:.1f} kg")

res = veh.res  # ReversibleEnergyStorage (RES / battery)

print("\nBattery:")
print(
    f"  capacity (usable) : {res.energy_capacity_usable_joules / 3600:.2f} Wh  "
    f"({res.energy_capacity_joules / 3600:.2f} Wh total)"
)
print(
    f"  max power         : {res.pwr_out_max_kilowatts:.1f} kW  "
    f"/ {res.pwr_out_max_horsepower:.1f} hp"
)
print(
    f"  SOC window        : "
    f"{res.min_soc_percent:.0f}% – {res.max_soc_percent:.0f}%  "
    f"({res.min_soc_ratio:.2f} – {res.max_soc_ratio:.2f} ratio)"
)

em = veh.em  # ElectricMachine

if em is not None:
    print("\nElectric motor:")
    print(f"  max power : {em.pwr_out_max_kilowatts:.1f} kW  / {em.pwr_out_max_horsepower:.1f} hp")

# ── 3. Cycle arrays ───────────────────────────────────────────────────────────

cyc = fsim.Cycle.from_resource("udds.csv")

# Cycle SI fields that are Vec<si::T> return Vec[float] — no manual conversion.
time_s = cyc.time_seconds
speed_mps = cyc.speed_meters_per_second
speed_kmh = cyc.speed_kilometers_per_hour

print(
    f"\nUDDS cycle: {len(time_s)} time steps, "
    f"duration {time_s[-1]:.0f} s, "
    f"peak {max(speed_kmh):.1f} km/h  "
    f"({max(speed_mps):.2f} m/s)"
)

# ── 4. Simulation history arrays ──────────────────────────────────────────────

sd = fsim.SimDrive(veh, cyc)
sd.walk()

# Retrieve post-simulation history via pydict (history is serialised with the
# vehicle).  HistoryVec types also have SI getter properties — they return
# Vec[float] just like cycle arrays.
veh_dict = sd.to_pydict()["veh"]
pt_hev = veh_dict["pt_type"]["HEV"]

fc_hist = fsim.FuelConverterStateHistoryVec.from_pydict(pt_hev["fc"]["history"])
res_hist = fsim.ReversibleEnergyStorageStateHistoryVec.from_pydict(pt_hev["res"]["history"])

# FC fuel and propulsion power traces — available in W, kW, and hp.
fuel_kw = fc_hist.pwr_fuel_kilowatts
prop_kw = fc_hist.pwr_prop_kilowatts

print(f"\nFC fuel power  : mean {sum(fuel_kw) / len(fuel_kw):.2f} kW, peak {max(fuel_kw):.2f} kW")
print(f"FC prop power  : mean {sum(prop_kw) / len(prop_kw):.2f} kW, peak {max(prop_kw):.2f} kW")

# RES state-of-charge history — both `ratio` and `percent` variants exist.
soc_pct = res_hist.soc_percent
soc_rat = res_hist.soc_ratio

print(
    f"\nRES SOC range  : {min(soc_pct):.2f}% – {max(soc_pct):.2f}%  "
    f"({min(soc_rat):.4f} – {max(soc_rat):.4f} ratio)"
)

# FC cumulative energy — also available in Wh for convenience.
e_fuel_j = fc_hist.energy_fuel_joules
e_prop_j = fc_hist.energy_prop_joules
total_fuel_wh = e_fuel_j[-1] / 3600
total_prop_wh = e_prop_j[-1] / 3600

print(f"\nCumulative FC fuel energy : {total_fuel_wh:.1f} Wh")
print(f"Cumulative FC prop energy : {total_prop_wh:.1f} Wh")
print(f"Drivetrain efficiency     : {100 * total_prop_wh / total_fuel_wh:.1f}%  (prop / fuel)")

# ── 5. Unit safety: the old way vs the new way ────────────────────────────────

print("\n--- Unit safety comparison ---")

# Old approach: serde round-trip, unit implicit in the YAML key name
pwr_old = veh_dict["pt_type"]["HEV"]["fc"]["pwr_out_max_watts"]
print(f"Old (pydict, key carries unit) : {pwr_old:.0f}  ← must remember key name encodes 'watts'")

# New approach: explicit unit in the property name, no dict traversal
pwr_kw = fc.pwr_out_max_kilowatts
pwr_hp = fc.pwr_out_max_horsepower
print(f"New (property getter, kW)      : {pwr_kw:.2f} kW")
print(f"New (property getter, hp)      : {pwr_hp:.2f} hp")
