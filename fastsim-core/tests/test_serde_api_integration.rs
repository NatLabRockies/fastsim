//! Integration tests for #[serde_api] macro demonstrating multi-unit deserialization.
//!
//! Each SI field generates a helper struct with `Option<f64>` fields for each supported unit
//! variant. Any unit variant is accepted on input; values are converted to the SI base unit
//! internally. The serialized unit per quantity is configured in `serde_utils.rs`.

use fastsim_core::si;
use fastsim_core::utils::tracked_state::TrackedState;
use fastsim_proc_macros::{serde_api, HistoryVec};
use serde::{Deserialize, Serialize};

/// State captured each time step for history tracking
#[serde_api]
#[derive(Clone, Debug, Default, Serialize, Deserialize, PartialEq, HistoryVec)]
#[history_vec(no_pyo3)]
#[serde(default)]
struct TestDeviceState {
    /// Instantaneous power output
    pwr_out: si::Power,
    /// Operating efficiency
    #[si_unit(unitless)]
    eff: si::Ratio,
}

/// Struct for testing multi-unit deserialization with required, optional, and vector SI fields
#[serde_api]
#[derive(Debug, Serialize, Deserialize, Clone, PartialEq)]
struct TestDevice {
    /// Mass in kilograms - supports kilogram (primary)
    mass: si::Mass,

    /// Power in watts - supports watt and kilowatt
    power: si::Power,

    /// Velocity in m/s - supports meter_per_second, kilometer_per_hour, and mile_per_hour
    speed: si::Velocity,

    /// Time in seconds - supports second and hour
    duration: si::Time,

    /// Area in square meters
    area: si::Area,

    /// Temperature in Kelvin - supports kelvin, degree_celsius, and degree_fahrenheit
    temperature: si::Temperature,

    /// Energy in joules - supports joule and kilowatt_hour
    energy: si::Energy,

    /// Optional secondary temperature - supports kelvin, degree_celsius, and degree_fahrenheit
    secondary_temperature: Option<si::Temperature>,

    /// Vector of temperatures for multiple readings - supports kelvin, degree_celsius, and degree_fahrenheit
    temperature_readings: Vec<si::Temperature>,

    /// Efficiency ratio - supports ratio and percent
    #[si_unit(unitless)]
    efficiency: si::Ratio,

    /// Tracked power state, wrapped in TrackedState - accepts watts or kilowatts
    tracked_power: TrackedState<si::Power>,

    /// Tracked efficiency state, wrapped in TrackedState - accepts ratio or percent
    #[si_unit(unitless)]
    tracked_efficiency: TrackedState<si::Ratio>,

    /// Renamed from "old_power" - accepts old_power, old_power_watts, old_power_kilowatts, old_power_horsepower,
    /// as well as renamed_power, renamed_power_watts, renamed_power_kilowatts, renamed_power_horsepower
    #[serde(alias = "old_power")]
    renamed_power: si::Power,

    /// Battery / fuel specific energy.  Optional so that existing test fixtures that omit it
    /// continue to work.  Supports four unit variants:
    ///   - joules_per_kilogram (SI base, primary)
    ///   - kilojoules_per_kilogram
    ///   - watt_hours_per_kilogram   (custom uom unit defined in fastsim_core::si)
    ///   - kilowatt_hours_per_kilogram (custom uom unit defined in fastsim_core::si)
    #[serde(default)]
    specific_energy: Option<si::SpecificEnergy>,

    /// Current operational state
    #[serde(default)]
    state: TestDeviceState,
    /// History of operational state over time
    #[serde(default)]
    history: TestDeviceStateHistoryVec,
}

#[test]
fn test_deserialize_all_primary_units() {
    let json = r#"{
        "mass_kilograms": 2000.0,
        "power_watts": 150.0,
        "speed_meters_per_second": 30.0,
        "duration_seconds": 120.0,
        "area_square_meters": 5.0,
        "temperature_kelvin": 300.0,
        "energy_joules": 1000.0,
        "efficiency_ratio": 0.85,
        "temperature_readings_kelvin": [290.0, 300.0, 310.0],
        "tracked_power_watts": 100.0,
        "tracked_efficiency_ratio": 0.90,
        "renamed_power_watts": 50.0
    }"#;

    let device: TestDevice =
        serde_json::from_str(json).expect("Failed to deserialize with primary units");

    assert_eq!(device.mass.get::<si::kilogram>(), 2000.0);
    assert_eq!(device.power.get::<si::watt>(), 150.0);
    assert_eq!(device.speed.get::<si::meter_per_second>(), 30.0);
    assert_eq!(device.duration.get::<si::second>(), 120.0);
    assert_eq!(device.area.get::<si::square_meter>(), 5.0);
    assert_eq!(device.temperature.get::<si::kelvin_abs>(), 300.0);
    assert_eq!(device.energy.get::<si::joule>(), 1000.0);
    assert_eq!(device.efficiency.get::<si::ratio>(), 0.85);
    assert_eq!(device.secondary_temperature, None);
    assert_eq!(device.temperature_readings.len(), 3);
    assert_eq!(
        device
            .tracked_power
            .get_fresh(|| "".into())
            .unwrap()
            .get::<si::watt>(),
        100.0
    );
    assert_eq!(
        device
            .tracked_efficiency
            .get_fresh(|| "".into())
            .unwrap()
            .get::<si::ratio>(),
        0.90
    );
}

#[test]
fn test_deserialize_fails_with_missing_required_fields() {
    let json = r#"{
        "mass_kilograms": 2000.0,
        "power_watts": 150.0,
        "speed_meters_per_second": 30.0,
        "duration_seconds": 120.0,
        "area_square_meters": 5.0,
        "energy_joules": 1000.0,
        "efficiency_ratio": 0.85,
        "tracked_power_watts": 100.0,
        "tracked_efficiency_ratio": 0.90,
        "renamed_power_watts": 50.0
    }"#;

    let result: Result<TestDevice, _> = serde_json::from_str(json);
    assert!(
        result.is_err(),
        "Should return Err when required temperature field is missing"
    );
}

#[test]
fn test_deserialize_with_alternate_units() {
    let json = r#"{
        "mass_kilograms": 2000.0,
        "power_kilowatts": 0.15,
        "speed_kilometers_per_hour": 108.0,
        "duration_hours": 2.0,
        "area_square_meters": 5.0,
        "temperature_kelvin": 300.15,
        "energy_kilowatt_hours": 1.0,
        "efficiency_percent": 85.0,
        "secondary_temperature_degrees_celsius": 25.0,
        "temperature_readings_degrees_fahrenheit": [32.0, 68.0, 104.0],
        "tracked_power_kilowatts": 0.5,
        "tracked_efficiency_percent": 80.0,
        "renamed_power_watts": 50.0
    }"#;

    let device: TestDevice =
        serde_json::from_str(json).expect("Failed to deserialize with alternate units");

    // Verify unit conversions
    assert_eq!(device.power.get::<si::watt>(), 150.0); // 0.15 kW
    let speed = device.speed.get::<si::meter_per_second>();
    assert!((speed - 30.0).abs() < 0.1); // 108 km/h ≈ 30 m/s
    assert_eq!(device.energy.get::<si::joule>(), 3_600_000.0); // 1 kWh
    assert_eq!(device.duration.get::<si::second>(), 7_200.0); // 2 hours
    assert_eq!(device.efficiency.get::<si::ratio>(), 0.85); // 85%
    assert!(
        (device
            .tracked_power
            .get_fresh(|| "".into())
            .unwrap()
            .get::<si::watt>()
            - 500.0)
            .abs()
            < 1e-9
    ); // 0.5 kW
    assert!(
        (device
            .tracked_efficiency
            .get_fresh(|| "".into())
            .unwrap()
            .get::<si::ratio>()
            - 0.80)
            .abs()
            < 1e-9
    ); // 80%

    // Verify Optional field with alternate unit (celsius -> kelvin conversion)
    assert!(device.secondary_temperature.is_some());
    let secondary_temp = device.secondary_temperature.unwrap();
    assert!((secondary_temp.get::<si::kelvin_abs>() - 298.15).abs() < 0.01); // 25°C ≈ 298.15K

    // Verify Vec field with alternate units (fahrenheit -> kelvin)
    assert_eq!(device.temperature_readings.len(), 3);
    // 32°F = 273.15K, 68°F ≈ 293.15K, 104°F ≈ 313.15K
    assert!((device.temperature_readings[0].get::<si::kelvin_abs>() - 273.15).abs() < 0.5);
    assert!((device.temperature_readings[1].get::<si::kelvin_abs>() - 293.15).abs() < 0.5);
    assert!((device.temperature_readings[2].get::<si::kelvin_abs>() - 313.15).abs() < 0.5);
}

#[test]
fn test_deserialize_time_in_hours() {
    let json = r#"{
        "mass_kilograms": 2000.0,
        "power_watts": 150.0,
        "speed_meters_per_second": 30.0,
        "duration_hours": 2.0,
        "area_square_meters": 5.0,
        "temperature_kelvin": 300.0,
        "energy_joules": 1000.0,
        "efficiency_ratio": 0.85,
        "tracked_power_watts": 100.0,
        "tracked_efficiency_ratio": 0.90,
        "renamed_power_watts": 50.0
    }"#;

    let device: TestDevice =
        serde_json::from_str(json).expect("Failed to deserialize with primary units");

    assert_eq!(device.duration.get::<si::second>(), 7_200.0); // 2 hours
}

#[test]
fn test_deserialize_multiple_si_quantities() {
    let json = r#"{
        "mass_kilograms": 1500.0,
        "power_watts": 200.0,
        "speed_meters_per_second": 25.0,
        "duration_seconds": 300.0,
        "area_square_meters": 10.0,
        "temperature_kelvin": 298.15,
        "energy_joules": 5000.0,
        "efficiency_ratio": 0.90,
        "tracked_power_watts": 50.0,
        "tracked_efficiency_ratio": 0.88,
        "renamed_power_watts": 50.0
    }"#;

    let device: TestDevice =
        serde_json::from_str(json).expect("Failed to deserialize multiple SI quantities");

    assert_eq!(device.mass.get::<si::kilogram>(), 1500.0);
    assert_eq!(device.power.get::<si::watt>(), 200.0);
    assert_eq!(device.speed.get::<si::meter_per_second>(), 25.0);
    assert_eq!(device.duration.get::<si::second>(), 300.0);
    assert_eq!(device.area.get::<si::square_meter>(), 10.0);
    assert_eq!(device.temperature.get::<si::kelvin_abs>(), 298.15);
    assert_eq!(device.energy.get::<si::joule>(), 5000.0);
    assert_eq!(device.efficiency.get::<si::ratio>(), 0.90);
}

#[test]
fn test_serialize_uses_prescribed_units() {
    let json = r#"{
        "mass_kilograms": 2000.0,
        "power_watts": 150.0,
        "speed_meters_per_second": 30.0,
        "duration_seconds": 120.0,
        "area_square_meters": 5.0,
        "temperature_kelvin": 300.0,
        "energy_joules": 1000.0,
        "efficiency_ratio": 0.85,
        "temperature_readings_kelvin": [290.0, 300.0, 310.0],
        "tracked_power_watts": 100.0,
        "tracked_efficiency_ratio": 0.90,
        "renamed_power_watts": 50.0
    }"#;

    let device: TestDevice = serde_json::from_str(json).expect("Failed to deserialize");

    let serialized = serde_json::to_string(&device).expect("Failed to serialize");

    let value: serde_json::Value =
        serde_json::from_str(&serialized).expect("Failed to parse serialized JSON");

    assert!(value.get("mass_kilograms").is_some());
    assert!(value.get("power_kilowatts").is_some());
    assert!(value.get("speed_meters_per_second").is_some());
    assert!(value.get("duration_seconds").is_some());
    assert!(value.get("area_square_meters").is_some());
    assert!(value.get("temperature_degrees_celsius").is_some());
    assert!(value.get("energy_kilojoules").is_some());
    assert!(value.get("efficiency").is_some()); // Ratio: bare name
    assert!(value.get("tracked_power_kilowatts").is_some());
    assert!(value.get("tracked_efficiency").is_some()); // Ratio: bare name
    assert!(value.get("renamed_power_kilowatts").is_some());
}

#[test]
fn test_round_trip_conversion_preserves_values() {
    let json_input = r#"{
        "mass_kilograms": 2000.0,
        "power_watts": 150.0,
        "speed_meters_per_second": 30.0,
        "duration_seconds": 3600.0,
        "area_square_meters": 5.0,
        "temperature_kelvin": 300.0,
        "energy_joules": 1000.0,
        "efficiency_ratio": 0.85,
        "temperature_readings_kelvin": [290.0, 300.0, 310.0],
        "tracked_power_watts": 100.0,
        "tracked_efficiency_ratio": 0.90,
        "renamed_power_watts": 50.0
    }"#;

    let device: TestDevice = serde_json::from_str(json_input).expect("Failed to deserialize");

    let serialized = serde_json::to_string(&device).expect("Failed to serialize");

    let value: serde_json::Value =
        serde_json::from_str(&serialized).expect("Failed to parse serialized JSON");

    // Values are preserved through round-trip (in canonical serialized units)
    assert_eq!(value["mass_kilograms"], 2000.0);
    assert_eq!(value["power_kilowatts"], 0.15);
    assert_eq!(value["speed_meters_per_second"], 30.0);
    assert_eq!(value["duration_seconds"], 3600.0);
    assert_eq!(value["area_square_meters"], 5.0);
    let temp_c = value["temperature_degrees_celsius"].as_f64().unwrap();
    assert!((temp_c - 26.85).abs() < 1e-9);
    assert_eq!(value["energy_kilojoules"], 1.0);
    assert_eq!(value["efficiency"], 0.85); // Ratio: bare name
}

#[test]
fn test_deserialize_with_different_values() {
    let json = r#"{
        "mass_kilograms": 5000.0,
        "power_watts": 500.0,
        "speed_meters_per_second": 50.0,
        "duration_seconds": 600.0,
        "area_square_meters": 20.0,
        "temperature_kelvin": 400.0,
        "energy_joules": 10000.0,
        "efficiency_ratio": 0.95,
        "temperature_readings_kelvin": [380.0, 400.0, 420.0],
        "tracked_power_watts": 200.0,
        "tracked_efficiency_ratio": 0.75,
        "renamed_power_watts": 50.0
    }"#;

    let device: TestDevice =
        serde_json::from_str(json).expect("Failed to deserialize with different values");

    assert_eq!(device.mass.get::<si::kilogram>(), 5000.0);
    assert_eq!(device.power.get::<si::watt>(), 500.0);
    assert_eq!(device.speed.get::<si::meter_per_second>(), 50.0);
    assert_eq!(device.duration.get::<si::second>(), 600.0);
    assert_eq!(device.area.get::<si::square_meter>(), 20.0);
    assert_eq!(device.temperature.get::<si::kelvin_abs>(), 400.0);
    assert_eq!(device.energy.get::<si::joule>(), 10000.0);
}

// ============================================================================
// TESTS: TrackedState fields
// ============================================================================

#[test]
fn test_tracked_state_deserialize_primary_units() {
    let json = r#"{
        "mass_kilograms": 1000.0,
        "power_watts": 100.0,
        "speed_meters_per_second": 10.0,
        "duration_seconds": 60.0,
        "area_square_meters": 2.0,
        "temperature_kelvin": 293.0,
        "energy_joules": 500.0,
        "efficiency_ratio": 0.80,
        "tracked_power_watts": 250.0,
        "tracked_efficiency_ratio": 0.92,
        "renamed_power_watts": 50.0
    }"#;

    let device: TestDevice =
        serde_json::from_str(json).expect("Failed to deserialize TestDevice primary units");

    assert_eq!(
        device
            .tracked_power
            .get_fresh(|| "".into())
            .unwrap()
            .get::<si::watt>(),
        250.0
    );
    assert_eq!(
        device
            .tracked_efficiency
            .get_fresh(|| "".into())
            .unwrap()
            .get::<si::ratio>(),
        0.92
    );
}

#[test]
fn test_tracked_state_deserialize_alternate_units() {
    let json = r#"{
        "mass_kilograms": 1000.0,
        "power_watts": 100.0,
        "speed_meters_per_second": 10.0,
        "duration_seconds": 60.0,
        "area_square_meters": 2.0,
        "temperature_kelvin": 293.0,
        "energy_joules": 500.0,
        "efficiency_ratio": 0.80,
        "tracked_power_kilowatts": 0.5,
        "tracked_efficiency_percent": 85.0,
        "renamed_power_watts": 50.0
    }"#;

    let device: TestDevice =
        serde_json::from_str(json).expect("Failed to deserialize TestDevice alternate units");

    assert!(
        (device
            .tracked_power
            .get_fresh(|| "".into())
            .unwrap()
            .get::<si::watt>()
            - 500.0)
            .abs()
            < 1e-9
    );
    assert!(
        (device
            .tracked_efficiency
            .get_fresh(|| "".into())
            .unwrap()
            .get::<si::ratio>()
            - 0.85)
            .abs()
            < 1e-9
    );
}

#[test]
fn test_tracked_state_deserialize_bare_name() {
    // Bare field names (without unit suffix) are accepted as aliases for the primary unit
    let json = r#"{
        "mass_kilograms": 1000.0,
        "power_watts": 100.0,
        "speed_meters_per_second": 10.0,
        "duration_seconds": 60.0,
        "area_square_meters": 2.0,
        "temperature_kelvin": 293.0,
        "energy_joules": 500.0,
        "efficiency_ratio": 0.80,
        "tracked_power": 300.0,
        "tracked_efficiency": 0.78,
        "renamed_power_watts": 50.0
    }"#;

    let device: TestDevice =
        serde_json::from_str(json).expect("Failed to deserialize TestDevice with bare names");

    assert_eq!(
        device
            .tracked_power
            .get_fresh(|| "".into())
            .unwrap()
            .get::<si::watt>(),
        300.0
    );
    assert_eq!(
        device
            .tracked_efficiency
            .get_fresh(|| "".into())
            .unwrap()
            .get::<si::ratio>(),
        0.78
    );
}

// ============================================================================
// TEST: Drive cycle CSV loading with bare "grade" column
// ============================================================================

#[test]
#[cfg(feature = "csv")]
fn test_cycle_csv_loads_bare_grade_column() {
    use fastsim_core::drive_cycle::CycleElement;

    // CSV with bare "grade" column (no unit suffix)
    let csv = "time_seconds,speed_meters_per_second,grade\n\
               0,0,0\n\
               1,2.5,0.01\n\
               2,5.0,-0.005\n";

    let mut rdr = csv::Reader::from_reader(csv.as_bytes());
    let elements: Vec<CycleElement> = rdr
        .deserialize()
        .map(|r| r.expect("Failed to deserialize CycleElement row"))
        .collect();

    assert_eq!(elements.len(), 3);
    assert_eq!(elements[0].time.get::<si::second>(), 0.0);
    assert_eq!(elements[1].speed.get::<si::meter_per_second>(), 2.5);
    // grade in ratio units (0.01 = 1% grade)
    assert!((elements[1].grade.unwrap().get::<si::ratio>() - 0.01).abs() < 1e-10);
    assert!((elements[2].grade.unwrap().get::<si::ratio>() - (-0.005)).abs() < 1e-10);
}

#[test]
#[cfg(feature = "csv")]
fn test_cycle_csv_loads_grade_ratio_column() {
    use fastsim_core::drive_cycle::CycleElement;

    // CSV with explicit "grade_ratio" column
    let csv = "time_seconds,speed_meters_per_second,grade_ratio\n\
               0,0,0\n\
               1,10.0,0.02\n";

    let mut rdr = csv::Reader::from_reader(csv.as_bytes());
    let elements: Vec<CycleElement> = rdr
        .deserialize()
        .map(|r| r.expect("Failed to deserialize CycleElement row"))
        .collect();

    assert_eq!(elements.len(), 2);
    assert!((elements[1].grade.unwrap().get::<si::ratio>() - 0.02).abs() < 1e-10);
}

#[test]
#[cfg(feature = "csv")]
fn test_cycle_csv_legacy_aliases() {
    use fastsim_core::drive_cycle::CycleElement;

    // CSV with legacy fastsim2 column names: cycSecs, cycMps, cycGrade
    let csv = "cycSecs,cycMps,cycGrade\n\
               0,0,0\n\
               1,5.0,0.05\n\
               2,10.0,-0.02\n";

    let mut rdr = csv::Reader::from_reader(csv.as_bytes());
    let elements: Vec<CycleElement> = rdr
        .deserialize()
        .map(|r| r.expect("Failed to deserialize CycleElement row with legacy aliases"))
        .collect();

    assert_eq!(elements.len(), 3);
    assert_eq!(elements[0].time.get::<si::second>(), 0.0);
    assert_eq!(elements[1].speed.get::<si::meter_per_second>(), 5.0);
    assert!((elements[1].grade.unwrap().get::<si::ratio>() - 0.05).abs() < 1e-10);
    assert!((elements[2].grade.unwrap().get::<si::ratio>() - (-0.02)).abs() < 1e-10);
}

// ============================================================================
// TEST: alias = name prefix expands to all unit variants (backward compat rename)
// ============================================================================

#[test]
fn test_alias_prefix_expands_to_all_unit_variants() {
    let base_json = r#"{
        "mass_kilograms": 1000.0,
        "power_watts": 100.0,
        "speed_meters_per_second": 10.0,
        "duration_seconds": 60.0,
        "area_square_meters": 2.0,
        "temperature_kelvin": 293.0,
        "energy_joules": 500.0,
        "efficiency_ratio": 0.80,
        "tracked_power_watts": 50.0,
        "tracked_efficiency_ratio": 0.90,
        "renamed_power_watts": 0.0
    }"#;

    // Old bare name routes to SI base unit (watts), regardless of the current serialization default
    let json = base_json.replace(r#""renamed_power_watts": 0.0"#, r#""old_power": 200.0"#);
    let d: TestDevice = serde_json::from_str(&json).expect("old_power (bare) should deserialize");
    assert_eq!(d.renamed_power.get::<si::watt>(), 200.0); // 200 W

    // Old name + _watts suffix
    let json = base_json.replace(
        r#""renamed_power_watts": 0.0"#,
        r#""old_power_watts": 300.0"#,
    );
    let d: TestDevice = serde_json::from_str(&json).expect("old_power_watts should deserialize");
    assert_eq!(d.renamed_power.get::<si::watt>(), 300.0);

    // Old name + _kilowatts suffix — converts correctly
    let json = base_json.replace(
        r#""renamed_power_watts": 0.0"#,
        r#""old_power_kilowatts": 0.5"#,
    );
    let d: TestDevice =
        serde_json::from_str(&json).expect("old_power_kilowatts should deserialize");
    assert!((d.renamed_power.get::<si::watt>() - 500.0).abs() < 1e-9); // 0.5 kW = 500 W

    // Old name + _horsepower suffix — converts correctly
    let json = base_json.replace(
        r#""renamed_power_watts": 0.0"#,
        r#""old_power_horsepower": 1.0"#,
    );
    let d: TestDevice =
        serde_json::from_str(&json).expect("old_power_horsepower should deserialize");
    assert!((d.renamed_power.get::<si::watt>() - 745.7).abs() < 0.1); // 1 hp ≈ 745.7 W

    // New canonical name still works
    let json = base_json.replace(
        r#""renamed_power_watts": 0.0"#,
        r#""renamed_power_kilowatts": 0.15"#,
    );
    let d: TestDevice =
        serde_json::from_str(&json).expect("renamed_power_kilowatts should deserialize");
    assert!((d.renamed_power.get::<si::watt>() - 150.0).abs() < 1e-9);
}

#[test]
fn test_multi_unit_conflict_is_rejected() {
    // Supplying two unit variants for the same field must fail with a descriptive error.
    let json = r#"{
        "mass_kilograms": 1000.0,
        "power_watts": 100.0,
        "power_kilowatts": 0.1,
        "speed_meters_per_second": 10.0,
        "duration_seconds": 60.0,
        "area_square_meters": 2.0,
        "temperature_kelvin": 293.0,
        "energy_joules": 500.0,
        "efficiency_ratio": 0.80,
        "tracked_power_watts": 50.0,
        "tracked_efficiency_ratio": 0.90,
        "renamed_power_watts": 200.0
    }"#;

    let result: Result<TestDevice, _> = serde_json::from_str(json);
    assert!(
        result.is_err(),
        "deserializing with two unit variants for `power` must return an error"
    );
    let err_msg = result.unwrap_err().to_string();
    assert!(
        err_msg.contains("power"),
        "error message should mention the conflicting field; got: {err_msg}"
    );
}

#[test]
fn test_state_history_push_and_round_trip() {
    let json = r#"{
        "mass_kilograms": 1000.0,
        "power_watts": 100.0,
        "speed_meters_per_second": 10.0,
        "duration_seconds": 60.0,
        "area_square_meters": 2.0,
        "temperature_kelvin": 293.0,
        "energy_joules": 500.0,
        "efficiency_ratio": 0.80,
        "tracked_power_watts": 50.0,
        "tracked_efficiency_ratio": 0.90,
        "renamed_power_watts": 200.0
    }"#;

    let mut device: TestDevice = serde_json::from_str(json).unwrap();
    // Initial state defaults to zero
    assert_eq!(device.history.len(), 0);

    // Push two states
    device.state.pwr_out = si::Power::new::<si::watt>(500.0);
    device.state.eff = si::Ratio::new::<si::ratio>(0.9);
    device.history.push(device.state.clone());

    device.state.pwr_out = si::Power::new::<si::watt>(750.0);
    device.state.eff = si::Ratio::new::<si::ratio>(0.85);
    device.history.push(device.state.clone());

    assert_eq!(device.history.len(), 2);

    // Round-trip through JSON
    let json = serde_json::to_string(&device).unwrap();
    let rt: TestDevice = serde_json::from_str(&json).expect("round-trip failed");

    assert_eq!(rt.history.len(), 2);
    assert!((rt.history.pwr_out[0].get::<si::watt>() - 500.0).abs() < 1e-9);
    assert!((rt.history.pwr_out[1].get::<si::watt>() - 750.0).abs() < 1e-9);
    assert!((rt.history.eff[0].get::<si::ratio>() - 0.9).abs() < 1e-9);
    assert!((rt.history.eff[1].get::<si::ratio>() - 0.85).abs() < 1e-9);
}

#[test]
fn test_state_history_deserializes_alternate_units() {
    // History vec should accept alternate unit keys on input
    // (pwr_out_kilowatts instead of pwr_out_watts)
    let json = r#"{
        "mass_kilograms": 1000.0,
        "power_watts": 100.0,
        "speed_meters_per_second": 10.0,
        "duration_seconds": 60.0,
        "area_square_meters": 2.0,
        "temperature_kelvin": 293.0,
        "energy_joules": 500.0,
        "efficiency_ratio": 0.80,
        "tracked_power_watts": 50.0,
        "tracked_efficiency_ratio": 0.90,
        "renamed_power_watts": 200.0,
        "history": {
            "pwr_out_kilowatts": [0.5, 0.75],
            "eff_percent": [90.0, 85.0]
        }
    }"#;

    let device: TestDevice = serde_json::from_str(json).expect("alternate unit in history failed");
    assert_eq!(device.history.len(), 2);
    assert!((device.history.pwr_out[0].get::<si::watt>() - 500.0).abs() < 1e-9);
    assert!((device.history.pwr_out[1].get::<si::watt>() - 750.0).abs() < 1e-9);
    assert!((device.history.eff[0].get::<si::ratio>() - 0.9).abs() < 1e-9);
    assert!((device.history.eff[1].get::<si::ratio>() - 0.85).abs() < 1e-9);
}

// ============================================================================
// TEST: #[si_unit(...)] field-level serialization unit override
// ============================================================================

/// Device that overrides the serialization unit for one field via #[si_unit(...)].
/// `pwr_out` serializes as kilowatts; all other fields use the global default.
#[serde_api]
#[derive(Debug, Default, Serialize, Deserialize, Clone, PartialEq)]
#[serde(default)]
struct OverrideDevice {
    #[si_unit(kilowatts)]
    pwr_out: si::Power,
    speed: si::Velocity,
}

#[serde_api]
#[derive(Debug, Default, Serialize, Deserialize, Clone, PartialEq)]
#[serde(default)]
struct UnitlessRatioDevice {
    #[si_unit(unitless)]
    alt_eff: si::Ratio,
    #[si_unit(unitless)]
    cop: si::Ratio,
    grade: si::Ratio,
}

#[test]
fn test_si_unit_override_changes_serialization_key() {
    // Build a device with 500 W output and 10 m/s speed
    let device = OverrideDevice {
        pwr_out: si::Power::new::<si::watt>(500.0),
        speed: si::Velocity::new::<si::meter_per_second>(10.0),
    };

    let json = serde_json::to_string(&device).unwrap();
    let value: serde_json::Value = serde_json::from_str(&json).unwrap();

    // pwr_out should be serialized under the kilowatts key
    assert!(
        value.get("pwr_out_kilowatts").is_some(),
        "expected pwr_out_kilowatts key; got: {json}"
    );
    assert!(
        value.get("pwr_out_watts").is_none(),
        "unexpected pwr_out_watts key when kilowatts override is set"
    );

    // speed uses the global default (meters_per_second)
    assert!(value.get("speed_meters_per_second").is_some());
}

// ============================================================================
// TESTS: SpecificEnergy — custom wh/kg and kWh/kg units
// ============================================================================

/// Base JSON for SpecificEnergy tests — all required TestDevice fields, specific_energy omitted.
const SE_BASE: &str = r#"{
    "mass_kilograms": 1000.0,
    "power_watts": 100.0,
    "speed_meters_per_second": 10.0,
    "duration_seconds": 60.0,
    "area_square_meters": 2.0,
    "temperature_kelvin": 293.15,
    "energy_joules": 500.0,
    "efficiency_ratio": 0.85,
    "tracked_power_watts": 50.0,
    "tracked_efficiency_ratio": 0.90,
    "renamed_power_watts": 200.0
}"#;

#[test]
fn test_specific_energy_serializes_as_kilowatt_hours_per_kilogram() {
    // Primary serialization key for SpecificEnergy is kilowatt_hours_per_kilogram.
    let json = SE_BASE.replace(
        "}",
        r#", "specific_energy_watt_hours_per_kilogram": 100.0}"#,
    );
    let device: TestDevice = serde_json::from_str(&json).unwrap();
    let serialized = serde_json::to_string(&device).unwrap();
    let value: serde_json::Value = serde_json::from_str(&serialized).unwrap();
    assert!(
        value.get("specific_energy_kilowatt_hours_per_kilogram").is_some(),
        "expected specific_energy_kilowatt_hours_per_kilogram in serialized output; got: {serialized}"
    );
    // 100 Wh/kg = 0.1 kWh/kg
    let stored = value["specific_energy_kilowatt_hours_per_kilogram"]
        .as_f64()
        .unwrap();
    assert!((stored - 0.1).abs() < 1e-12);
}

#[test]
fn test_specific_energy_multi_unit_round_trip() {
    // All four unit keys should round-trip to the same internal value.
    let expected_j_per_kg = 3_600_000.0_f64; // = 1 kWh/kg = 1000 Wh/kg = 3600 kJ/kg

    let cases: &[(&str, f64)] = &[
        ("specific_energy_joules_per_kilogram", expected_j_per_kg),
        (
            "specific_energy_kilojoules_per_kilogram",
            expected_j_per_kg / 1_000.0,
        ),
        (
            "specific_energy_watt_hours_per_kilogram",
            expected_j_per_kg / 3_600.0,
        ),
        (
            "specific_energy_kilowatt_hours_per_kilogram",
            expected_j_per_kg / 3_600_000.0,
        ),
    ];

    for (key, value) in cases {
        let json = SE_BASE.replace("}", &format!(", \"{key}\": {value}}}"));
        let device: TestDevice = serde_json::from_str(&json)
            .unwrap_or_else(|e| panic!("failed to deserialize {key}: {e}"));
        let se = device
            .specific_energy
            .unwrap_or_else(|| panic!("{key} gave None"));
        assert!(
            (se.get::<si::joule_per_kilogram>() - expected_j_per_kg).abs() < 1.0,
            "{key}={value} → {:.0} J/kg (expected {expected_j_per_kg:.0})",
            se.get::<si::joule_per_kilogram>()
        );
    }
    // Also verify the custom unit types themselves read back correctly.
    let wh_json = SE_BASE.replace(
        "}",
        ", \"specific_energy_watt_hours_per_kilogram\": 1000.0}",
    );
    let se = serde_json::from_str::<TestDevice>(&wh_json)
        .unwrap()
        .specific_energy
        .unwrap();
    assert!((se.get::<si::watt_hour_per_kilogram>() - 1000.0).abs() < 1e-6);

    let kwh_json = SE_BASE.replace(
        "}",
        ", \"specific_energy_kilowatt_hours_per_kilogram\": 0.5}",
    );
    let se = serde_json::from_str::<TestDevice>(&kwh_json)
        .unwrap()
        .specific_energy
        .unwrap();
    assert!((se.get::<si::kilowatt_hour_per_kilogram>() - 0.5).abs() < 1e-9);
}

#[test]
fn test_si_unit_override_round_trips() {
    // Deserialize from any supported unit key; serialization uses the override unit
    let json = r#"{ "pwr_out_watts": 500.0, "speed_meters_per_second": 10.0 }"#;
    let device: OverrideDevice = serde_json::from_str(json).unwrap();

    // Internal representation is always SI base units
    assert!((device.pwr_out.get::<si::watt>() - 500.0).abs() < 1e-9);

    // Round-trip: serializes as kilowatts, deserializes back correctly
    let out = serde_json::to_string(&device).unwrap();
    let rt: OverrideDevice = serde_json::from_str(&out).unwrap();
    assert!((rt.pwr_out.get::<si::watt>() - 500.0).abs() < 1e-9);
}

#[test]
fn test_si_unit_unitless_override_changes_ratio_canonical_name() {
    let device = UnitlessRatioDevice {
        alt_eff: si::Ratio::new::<si::ratio>(0.9),
        cop: si::Ratio::new::<si::ratio>(2.5),
        grade: si::Ratio::new::<si::ratio>(0.03),
    };

    let json = serde_json::to_string(&device).unwrap();
    let value: serde_json::Value = serde_json::from_str(&json).unwrap();

    // unitless override forms should serialize bare
    assert!(
        value.get("alt_eff").is_some(),
        "expected bare alt_eff in {json}"
    );
    assert!(value.get("cop").is_some(), "expected bare cop in {json}");
    assert!(value.get("alt_eff_ratio").is_none());
    assert!(value.get("cop_ratio").is_none());

    // With Ratio escape hatch disabled, unannotated ratio fields serialize with suffix.
    assert!(
        value.get("grade_ratio").is_some(),
        "expected grade_ratio in {json}"
    );
    assert!(value.get("grade").is_none());
}
