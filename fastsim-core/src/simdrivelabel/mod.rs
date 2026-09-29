//! Module containing classes and methods for calculating label fuel economy.

use std::collections::HashMap;

// crate local
use crate::drive_cycle::{Cycle, CYC_ACCEL};
use crate::imports::*;
use crate::simdrive::{params::SimParams, SimDrive};
use crate::vehicle::{PowertrainType, Vehicle};

/// Return first index of `arr` greater than `cut`
fn first_grtr(arr: &[f64], cut: f64) -> Option<usize> {
    let len = arr.len();
    if len == 0 {
        return None;
    }
    Some(arr.iter().position(|&x| x > cut).unwrap_or(len - 1)) // unwrap_or allows for default if not found
}

fn parse_model_year_or_2017(veh_year: Option<&str>) -> u32 {
    if let Some(parsed_year) = veh_year.and_then(|year| year.parse::<u32>().ok()) {
        parsed_year
    } else {
        eprintln!("Model year could not be parsed; using 2017 adjustment coefficients.");
        2017
    }
}

/// Get the 0 to 60 mph accelaration time from the given times and speeds.
pub fn get_0_to_60_time_from_accel_data(accel_data: &AccelData) -> anyhow::Result<f64> {
    // Check if vehicle reaches 60 mph
    let first_ind_after_60_mph =
        first_grtr(&accel_data.speed_mph, 60.).with_context(|| format_dbg!())?;

    if accel_data.speed_mph.iter().any(|&x| x >= 60.0) {
        // Create interpolator from speed to time
        let interp = Interp1DView::new(
            ArrayView::from(&accel_data.speed_mph[..first_ind_after_60_mph + 1]),
            ArrayView::from(&accel_data.time_s[..first_ind_after_60_mph + 1]),
            strategy::Linear,
            Extrapolate::Clamp,
        )
        .map_err(|e| {
            anyhow!(
                "Failed to create interpolator at line {} with originating error [{}]",
                format_dbg!(),
                e
            )
        })?;

        // Interpolate time at 60 mph
        let accel_time = interp.interpolate(&[60.0]).map_err(|e| {
            anyhow!(
                "Failed to interpolate acceleration time at line {} with originating error [{}]",
                format_dbg!(),
                e
            )
        })?;
        Ok(accel_time)
    } else {
        bail!("Vehicle does not reach 60 mph")
    }
}

/// Run the acceleration test and return the time/speed trace.
pub fn run_accel(
    veh: &Vehicle,
    sim_params: &HashMap<&'static str, SimParams>,
) -> anyhow::Result<AccelData> {
    let mut sd_accel = SimDrive::new(
        veh.clone(),
        CYC_ACCEL.clone(),
        sim_params.get("accel").cloned(),
    );
    sd_accel.sim_params.trace_miss_opts = TraceMissOptions::Allow;
    sd_accel.run_once().map_err(|e| {
        anyhow!(
            "Acceleration simdrive run_once failed at line {} with originating error [{}]",
            format_dbg!(),
            e
        )
    })?;
    // Extract speed values in mph
    let mut speed_mph: Vec<f64> = vec![];
    for s in sd_accel.veh.history.speed_ach.clone() {
        speed_mph.push(s.get_fresh(|| format_dbg!())?.get::<si::mile_per_hour>())
    }
    // Extract time values in seconds
    let time_s: Vec<f64> = sd_accel
        .cyc
        .time
        .iter()
        .map(|t| t.get::<si::second>())
        .collect();
    Ok(AccelData { time_s, speed_mph })
}

/// Returns time [s] for 0-60 mph acceleration at max power
pub fn get_0_to_60_time(sd_accel: &mut SimDrive) -> anyhow::Result<f64> {
    sd_accel.sim_params.trace_miss_opts = TraceMissOptions::Allow;
    sd_accel.run_once().map_err(|e| {
        anyhow!(
            "Acceleration simdrive run_once failed at line {} with originating error [{}]",
            format_dbg!(),
            e
        )
    })?;

    // Extract speed values in mph
    let mut speed_mph: Vec<f64> = vec![];
    for s in sd_accel.veh.history.speed_ach.clone() {
        speed_mph.push(s.get_fresh(|| format_dbg!())?.get::<si::mile_per_hour>())
    }

    // Extract time values in seconds
    let time_s: Vec<f64> = sd_accel
        .cyc
        .time
        .iter()
        .map(|t| t.get::<si::second>())
        .collect();

    let accel_data = AccelData { time_s, speed_mph };
    let accel_time = get_0_to_60_time_from_accel_data(&accel_data).map_err(|err| {
        anyhow!(
            "Vehicle {} doesn't reach 60 mph in the acceleration test at line {} with originating error [{}]",
            sd_accel.veh.name,
            format_dbg!(),
            err
        )
    })?;
    Ok(accel_time)
}

// const MPH_PER_MPS: f64 = 2.2369362921;
const DEFAULT_CHG_EFF: f64 = 0.86;

#[serde_api]
#[derive(Clone, Debug, Deserialize, Serialize, PartialEq)]
#[cfg_attr(feature = "pyo3", pyclass(module = "fastsim", subclass, eq))]
pub struct FuelProperties {
    // TODO: make a way to serialize/deserialize with "J/m^3"
    // fuel energy density
    /// fuel energy density (i.e. energy per unit mass, which has the same base
    /// units as pressure)
    pub energy_density: si::Pressure,
    /// fuel density
    pub density: si::MassDensity,
}

impl Init for FuelProperties {}
impl SerdeAPI for FuelProperties {}

#[pyo3_api]
impl FuelProperties {}

impl Default for FuelProperties {
    /// Default values for gasoline
    fn default() -> Self {
        Self {
            energy_density: 33.7 * uc::KWH / uc::GALLON,
            density: 0.75 * uc::KG / uc::L,
        }
    }
}

const J_PER_KWH: f64 = 3_600_000.0;
lazy_static! {
    static ref CUBIC_METER_PER_GAL: f64 = 3.79e-3;
}

impl FuelProperties {
    fn kwh_per_gge(&self) -> f64 {
        self.energy_density.get::<si::joule_per_cubic_meter>() / J_PER_KWH * *CUBIC_METER_PER_GAL
    }
}

trait VehicleEfficiency {
    fn mpg(&self, energy_density: si::Pressure) -> anyhow::Result<f64>;

    fn kwh_per_mi(&self) -> anyhow::Result<f64>;
}

impl VehicleEfficiency for Vehicle {
    fn mpg(&self, energy_density: si::Pressure) -> anyhow::Result<f64> {
        if let Some(fc) = self.fc() {
            Ok(self
                .state
                .dist
                .get_fresh(|| format_dbg!())?
                .get::<si::mile>()
                / (*fc.state.energy_fuel.get_fresh(|| format_dbg!())? / energy_density)
                    .get::<si::gallon>())
        } else {
            Ok(f64::NAN)
        }
    }

    fn kwh_per_mi(&self) -> anyhow::Result<f64> {
        if let Some(res) = self.res() {
            Ok(res
                .state
                .energy_out_chemical
                .get_fresh(|| format_dbg!())?
                .get::<si::kilowatt_hour>()
                / self
                    .state
                    .dist
                    .get_fresh(|| format_dbg!())?
                    .get::<si::mile>())
        } else {
            Ok(f64::NAN)
        }
    }
}

#[serde_api]
#[derive(Clone, Default, Debug, Deserialize, Serialize, PartialEq)]
#[cfg_attr(feature = "pyo3", pyclass(module = "fastsim", subclass, eq))]
pub struct LabelFe {
    pub veh: Option<Vehicle>,
    pub adj_params: AdjCoef,
    pub lab_udds_mpgge: f64,
    pub lab_hwy_mpgge: f64,
    pub lab_comb_mpgge: f64,
    pub lab_udds_kwh_per_mi: f64,
    pub lab_hwy_kwh_per_mi: f64,
    pub lab_comb_kwh_per_mi: f64,
    pub adj_udds_mpgge: f64,
    pub adj_hwy_mpgge: f64,
    pub adj_comb_mpgge: f64,
    pub adj_udds_kwh_per_mi: f64,
    pub adj_hwy_kwh_per_mi: f64,
    pub adj_comb_kwh_per_mi: f64,
    pub adj_udds_ess_kwh_per_mi: f64,
    pub adj_hwy_ess_kwh_per_mi: f64,
    pub adj_comb_ess_kwh_per_mi: f64,
    pub net_range_miles: f64,
    pub uf: Option<f64>,
    pub net_accel: f64,
    pub res_found: String,
    pub phev_calcs: Option<LabelFePHEV>,
    pub adj_cs_comb_mpgge: Option<f64>,
    pub adj_cd_comb_mpgge: Option<f64>,
    pub net_phev_cd_miles: Option<f64>,
}

#[pyo3_api]
impl LabelFe {}

impl Init for LabelFe {}
impl SerdeAPI for LabelFe {}

#[serde_api]
#[derive(Default, Clone, Debug, Deserialize, Serialize, PartialEq)]
#[cfg_attr(feature = "pyo3", pyclass(module = "fastsim", subclass, eq))]
/// Label fuel economy values for a PHEV vehicle
pub struct LabelFePHEV {
    pub regen_soc_buffer: si::Ratio,
    pub udds: PHEVCycleCalc,
    pub hwy: PHEVCycleCalc,
}

#[pyo3_api]
impl LabelFePHEV {}

impl Init for LabelFePHEV {}
impl SerdeAPI for LabelFePHEV {}

#[serde_api]
#[derive(Default, Clone, Debug, Deserialize, Serialize, PartialEq)]
#[cfg_attr(feature = "pyo3", pyclass(module = "fastsim", subclass, eq))]
/// Label fuel economy calculations for a specific cycle of a PHEV vehicle
pub struct PHEVCycleCalc {
    /// Charge depletion battery kW-hr
    pub cd_ess_kwh: f64,
    pub cd_ess_kwh_per_mi: f64,
    /// Charge depletion fuel gallons
    pub cd_fs_gal: f64,
    pub cd_fs_kwh: f64,
    pub cd_mpg: f64,
    /// Number of cycles in charge depletion mode, up to transition
    pub cd_cycs: f64,
    pub cd_miles: f64,
    pub cd_lab_mpg: f64,
    pub cd_adj_mpg: f64,
    /// Fraction of transition cycles spent in charge depletion
    pub cd_frac_in_trans: f64,
    /// SOC change during 1 cycle
    pub trans_init_soc: si::Ratio,
    /// charge depletion battery kW-hr
    pub trans_ess_kwh: f64,
    pub trans_ess_kwh_per_mi: f64,
    pub trans_fs_gal: f64,
    pub trans_fs_kwh: f64,
    /// charge sustaining battery kW-hr
    pub cs_ess_kwh: f64,
    pub cs_ess_kwh_per_mi: f64,
    /// charge sustaining fuel gallons
    pub cs_fs_gal: f64,
    pub cs_fs_kwh: f64,
    pub cs_mpg: f64,
    pub lab_mpgge: f64,
    pub lab_kwh_per_mi: f64,
    pub lab_uf: f64,
    pub lab_uf_gpm: Vec<f64>,
    pub lab_iter_uf: Vec<f64>,
    pub lab_iter_uf_kwh_per_mi: Vec<f64>,
    pub lab_iter_kwh_per_mi: Vec<f64>,
    pub adj_iter_mpgge: Vec<f64>,
    pub adj_iter_kwh_per_mi: Vec<f64>,
    pub adj_iter_cd_miles: Vec<f64>,
    pub adj_iter_uf: Vec<f64>,
    pub adj_iter_uf_gpm: Vec<f64>,
    pub adj_iter_uf_kwh_per_mi: Vec<f64>,
    pub adj_cd_miles: f64,
    pub adj_cd_mpgge: f64,
    pub adj_cs_mpgge: f64,
    pub adj_uf: f64,
    pub adj_mpgge: f64,
    pub adj_kwh_per_mi: f64,
    pub adj_ess_kwh_per_mi: f64,
    pub delta_soc: si::Ratio,
    /// Total number of miles in charge depletion mode, assuming constant kWh_per_mi
    pub total_cd_miles: f64,
}

impl Init for PHEVCycleCalc {}
impl SerdeAPI for PHEVCycleCalc {}

#[pyo3_api]
impl PHEVCycleCalc {}

#[derive(Clone, Serialize, Deserialize, Debug, PartialEq)]
#[cfg_attr(feature = "pyo3", pyclass(module = "fastsim", subclass, eq))]
pub struct AdjCoef {
    pub city_intercept: f64,
    pub city_slope: f64,
    pub hwy_intercept: f64,
    pub hwy_slope: f64,
}

#[pyo3_api]
impl AdjCoef {}

impl Init for AdjCoef {}
impl SerdeAPI for AdjCoef {}

impl Default for AdjCoef {
    fn default() -> Self {
        Self {
            city_intercept: 0.003259,
            city_slope: 1.1805,
            hwy_intercept: 0.001376,
            hwy_slope: 1.3466,
        }
    }
}

#[serde_api]
#[derive(Clone, Serialize, Deserialize, Debug, PartialEq)]
#[cfg_attr(feature = "pyo3", pyclass(module = "fastsim", subclass, eq))]
pub struct PhevUtilizationParams {
    pub adj_coef_map: HashMap<String, AdjCoef>,
    /// Frequency of recharge events
    pub rechg_freq_miles: Vec<f64>,
    /// Array of utility factor
    pub uf_array: Vec<f64>,
}

impl Init for PhevUtilizationParams {}
impl SerdeAPI for PhevUtilizationParams {}

impl Default for PhevUtilizationParams {
    fn default() -> Self {
        Self::from_json(&*PHEV_UTIL_PARAMS, false).unwrap()
    }
}

lazy_static! {
    static ref PHEV_UTIL_PARAMS: String = include_str!("longparams.json").to_string();
}

pub struct PhevVehicleInfo {
    pub max_soc: si::Ratio,
    pub min_soc: si::Ratio,
    pub phev_max_regen: si::Ratio,
    pub veh_mass: si::Mass,
    pub em_peak_eff: si::Ratio,
    pub energy_capacity: si::Energy,
    pub chg_eff: f64,
    pub fuel_storage_capacity: si::Energy,
}

pub struct PhevSimulationDataForLabel {
    pub cd_fuel_consumed_kwh: f64,
    pub cd_soc_start: f64,
    pub cd_soc_end: f64,
    pub cyc_dist_mi: f64,
    pub cd_kwh_per_mi: f64,
    // sd.veh.mpg(fuel_props.energy_density)?
    pub cd_mpg: f64,
    pub cs_fuel_consumed_kwh: f64,
    // res.state.energy_out_chemical
    pub cs_ess_energy_kwh: f64,
    // sd.veh.kwh_per_mi()
    pub cs_kwh_per_mi: f64,
    // sd.veh.mpg()
    // sd.veh.mpg(fuel_props.energy_density)
    pub cs_mpg: f64,
    // min of sd.veh.res().history.soc
    pub cs_min_soc: f64,
    // phev.fs.energy_capacity.get::<si::kilowatt_hour>()
    pub cs_fs_energy_capacity_kwh: f64,
}

pub enum SimulationDataForLabel {
    ConvOrHev {
        veh_year: u32,
        udds_mpgge: f64,
        hwy_mpgge: f64,
        /// Fuel storage usable energy in kWh
        fs_energy_capacity_kwh: f64,
    },
    Bev {
        veh_year: u32,
        udds_kwh_per_mi: f64,
        hwy_kwh_per_mi: f64,
        bev_energy_capacity_kwh: f64,
    },
    Phev {
        veh_year: u32,
        info: PhevVehicleInfo,
        udds: PhevSimulationDataForLabel,
        hwy: PhevSimulationDataForLabel,
    },
}

pub struct AccelData {
    pub time_s: Vec<f64>,
    pub speed_mph: Vec<f64>,
}

/// Calculate the transient cycle's init SOC.
/// This is used for PHEV label fuel economy calculation.
/// Returns the calculated SOC.
pub fn calculate_transient_soc_helper(
    max_soc: f64,
    min_soc: f64,
    energy_capacity_kwh: f64,
    cyc_kwh_per_mi: f64,
    soc_start: f64,
    soc_end: f64,
    dist_mi: f64,
) -> f64 {
    let total_cd_miles = ((max_soc - min_soc) * energy_capacity_kwh) / cyc_kwh_per_mi;
    let cd_cycs = total_cd_miles / dist_mi;
    let delta_soc = soc_start - soc_end;
    max_soc - cd_cycs.floor() * delta_soc
}

/// A helper function to calculate label fuel economy for PHEVs.
pub fn calculate_phev_label_helper(
    info: &PhevVehicleInfo,
    data: &PhevSimulationDataForLabel,
    fuel_props: &FuelProperties,
    max_epa_adj: f64,
    phev_utilization_params: &PhevUtilizationParams,
    adj_params: &AdjCoef,
    label_fe_phev: &LabelFePHEV,
    is_city: bool,
) -> anyhow::Result<PHEVCycleCalc> {
    let mut phev_calc = PHEVCycleCalc::default();
    // charge depletion cycle has already been simulated
    // charge depletion battery kW-hr
    phev_calc.cd_ess_kwh =
        ((info.max_soc - info.min_soc) * info.energy_capacity).get::<si::kilowatt_hour>();
    let soc_start = data.cd_soc_start;
    let soc_end = data.cd_soc_end;
    let dist_mi = data.cyc_dist_mi;

    // SOC change during 1 cycle
    phev_calc.delta_soc = (soc_start - soc_end) * uc::R;
    // total number of miles in charge depletion mode, assuming constant kWh_per_mi
    phev_calc.total_cd_miles = ((info.max_soc - info.min_soc) * info.energy_capacity)
        .get::<si::kilowatt_hour>()
        / data.cd_kwh_per_mi;
    // number of cycles in charge depletion mode, up to transition
    phev_calc.cd_cycs = phev_calc.total_cd_miles / dist_mi;
    // fraction of transition cycle spent in charge depletion
    phev_calc.cd_frac_in_trans = phev_calc.cd_cycs % phev_calc.cd_cycs.floor();

    // charge depletion fuel gallons - get from fuel converter
    let fuel_energy_kwh = data.cd_fuel_consumed_kwh;
    phev_calc.cd_fs_gal = fuel_energy_kwh / fuel_props.kwh_per_gge();
    phev_calc.cd_fs_kwh = fuel_energy_kwh;
    phev_calc.cd_ess_kwh_per_mi = data.cd_kwh_per_mi;
    phev_calc.cd_mpg = data.cd_mpg;

    // utility factor calculation for last charge depletion iteration and transition iteration
    // ported from excel
    let interp_x_vals: Vec<f64> = (0..((phev_calc.cd_cycs.ceil() + 1.0) as usize))
        .map(|i| i as f64 * dist_mi)
        .collect();

    phev_calc.lab_iter_uf = vec![];
    for x in interp_x_vals {
        phev_calc.lab_iter_uf.push(
            phev_utilization_params.uf_array[first_grtr(
                &phev_utilization_params.rechg_freq_miles,
                x,
            )
            .with_context(|| format_dbg!())?
                - 1],
        );
    }

    // transition cycle
    phev_calc.trans_init_soc = info.max_soc - phev_calc.cd_cycs.floor() * phev_calc.delta_soc;

    // charge depletion battery kW-hr
    phev_calc.trans_ess_kwh = phev_calc.cd_ess_kwh_per_mi * dist_mi * phev_calc.cd_frac_in_trans;
    phev_calc.trans_ess_kwh_per_mi = phev_calc.cd_ess_kwh_per_mi * phev_calc.cd_frac_in_trans;

    // charge sustaining fuel gallons
    let cs_fuel_energy_kwh = data.cs_fuel_consumed_kwh;
    phev_calc.cs_fs_gal = cs_fuel_energy_kwh / fuel_props.kwh_per_gge();
    // charge depletion fuel gallons, dependent on phev_calc.trans_fs_gal
    phev_calc.trans_fs_gal = phev_calc.cs_fs_gal * (1.0 - phev_calc.cd_frac_in_trans);
    phev_calc.cs_fs_kwh = cs_fuel_energy_kwh;
    phev_calc.trans_fs_kwh = phev_calc.cs_fs_kwh * (1.0 - phev_calc.cd_frac_in_trans);
    // charge sustaining battery kW-hr
    let cs_ess_energy_kwh = data.cs_ess_energy_kwh;
    phev_calc.cs_ess_kwh = cs_ess_energy_kwh;
    phev_calc.cs_ess_kwh_per_mi = data.cs_kwh_per_mi;

    let lab_iter_uf_diff = phev_calc.lab_iter_uf.diff();
    phev_calc.lab_uf_gpm = [
        phev_calc.trans_fs_gal * lab_iter_uf_diff.last().with_context(|| format_dbg!())?,
        phev_calc.cs_fs_gal
            * (1.0
                - phev_calc
                    .lab_iter_uf
                    .last()
                    .with_context(|| format_dbg!())?),
    ]
    .iter()
    .map(|x| *x / dist_mi)
    .collect();

    // TODO: investigate. This does not seem correct but also appears in FASTSim 2
    // shouldn't this be setting cs_mpg?
    // Disabling for now. cd_mpg was already set above and cs_mpg is set below.
    // phev_calc.cd_mpg = data.cd_mpg;

    // city and highway cycle ranges
    let min_soc_in_cycle = phev_calc.delta_soc.abs(); // Use delta_soc as proxy for min SOC change
    phev_calc.cd_miles =
        if (info.max_soc - label_fe_phev.regen_soc_buffer - min_soc_in_cycle) < 0.01 * uc::R {
            1000.0
        } else {
            phev_calc.cd_cycs.ceil() * dist_mi
        };
    phev_calc.cd_lab_mpg = phev_calc
        .lab_iter_uf
        .last()
        .with_context(|| format_dbg!())?
        / (phev_calc.trans_fs_gal / dist_mi);

    // charge sustaining
    phev_calc.cs_mpg = dist_mi / phev_calc.cs_fs_gal;

    phev_calc.lab_uf = phev_utilization_params.uf_array[first_grtr(
        &phev_utilization_params.rechg_freq_miles,
        phev_calc.cd_miles,
    )
    .with_context(|| format_dbg!())?
        - 1];

    // labCombMpgge
    phev_calc.cd_adj_mpg =
        phev_calc.lab_iter_uf.max()? / phev_calc.lab_uf_gpm[phev_calc.lab_uf_gpm.len() - 2];

    phev_calc.lab_mpgge = 1.0
        / (phev_calc.lab_uf / phev_calc.cd_adj_mpg + (1.0 - phev_calc.lab_uf) / phev_calc.cs_mpg);

    let mut lab_iter_kwh_per_mi_vals = Vec::new();
    lab_iter_kwh_per_mi_vals.push(0.0);
    lab_iter_kwh_per_mi_vals
        .extend(vec![phev_calc.cd_ess_kwh_per_mi; phev_calc.cd_cycs.floor() as usize].iter());
    lab_iter_kwh_per_mi_vals.push(phev_calc.trans_ess_kwh_per_mi);
    lab_iter_kwh_per_mi_vals.push(0.0);
    phev_calc.lab_iter_kwh_per_mi = lab_iter_kwh_per_mi_vals;

    let uf_diff = phev_calc.lab_iter_uf.diff();
    let mut vals = Vec::new();
    vals.push(0.0);
    for i in 1..phev_calc.lab_iter_kwh_per_mi.len() - 1 {
        // if i - 1 < uf_diff.len() {
        if i < uf_diff.len() {
            // vals.push(phev_calc.lab_iter_kwh_per_mi[i] * uf_diff[i - 1]);
            vals.push(phev_calc.lab_iter_kwh_per_mi[i] * uf_diff[i]);
        }
    }
    vals.push(0.0);
    phev_calc.lab_iter_uf_kwh_per_mi = vals;

    phev_calc.lab_kwh_per_mi = phev_calc
        .lab_iter_uf_kwh_per_mi
        .iter()
        .fold(0.0, |acc, x| acc + x)
        / phev_calc
            .lab_iter_uf
            .iter()
            .fold(0.0f64, |acc, x| acc.max(*x));

    let mut adj_iter_mpgge_vals = vec![0.0; phev_calc.cd_cycs.floor() as usize];
    let mut adj_iter_kwh_per_mi_vals = vec![0.0; phev_calc.lab_iter_kwh_per_mi.len()];
    if is_city {
        adj_iter_mpgge_vals.push(f64::max(
            1.0 / (adj_params.city_intercept
                + (adj_params.city_slope
                    / (data.cyc_dist_mi / (phev_calc.trans_fs_kwh / fuel_props.kwh_per_gge())))),
            data.cyc_dist_mi / (phev_calc.trans_fs_kwh / fuel_props.kwh_per_gge())
                * (1.0 - max_epa_adj),
        ));
        adj_iter_mpgge_vals.push(f64::max(
            1.0 / (adj_params.city_intercept
                + (adj_params.city_slope
                    / (data.cyc_dist_mi / (phev_calc.cs_fs_kwh / fuel_props.kwh_per_gge())))),
            data.cyc_dist_mi / (phev_calc.cs_fs_kwh / fuel_props.kwh_per_gge())
                * (1.0 - max_epa_adj),
        ));

        for (c, _) in phev_calc.lab_iter_kwh_per_mi.iter().enumerate() {
            if phev_calc.lab_iter_kwh_per_mi[c] == 0.0 {
                adj_iter_kwh_per_mi_vals[c] = 0.0;
            } else {
                adj_iter_kwh_per_mi_vals[c] =
                    (1.0 / f64::max(
                        1.0 / (adj_params.city_intercept
                            + (adj_params.city_slope
                                / ((1.0 / phev_calc.lab_iter_kwh_per_mi[c])
                                    * fuel_props.kwh_per_gge()))),
                        (1.0 - max_epa_adj)
                            * ((1.0 / phev_calc.lab_iter_kwh_per_mi[c]) * fuel_props.kwh_per_gge()),
                    )) * fuel_props.kwh_per_gge();
            }
        }
    } else {
        adj_iter_mpgge_vals.push(f64::max(
            1.0 / (adj_params.hwy_intercept
                + (adj_params.hwy_slope
                    / (data.cyc_dist_mi / (phev_calc.trans_fs_kwh / fuel_props.kwh_per_gge())))),
            data.cyc_dist_mi / (phev_calc.trans_fs_kwh / fuel_props.kwh_per_gge())
                * (1.0 - max_epa_adj),
        ));
        adj_iter_mpgge_vals.push(f64::max(
            1.0 / (adj_params.hwy_intercept
                + (adj_params.hwy_slope
                    / (data.cyc_dist_mi / (phev_calc.cs_fs_kwh / fuel_props.kwh_per_gge())))),
            data.cyc_dist_mi / (phev_calc.cs_fs_kwh / fuel_props.kwh_per_gge())
                * (1.0 - max_epa_adj),
        ));

        for (c, _) in phev_calc.lab_iter_kwh_per_mi.iter().enumerate() {
            if phev_calc.lab_iter_kwh_per_mi[c] == 0.0 {
                adj_iter_kwh_per_mi_vals[c] = 0.0;
            } else {
                adj_iter_kwh_per_mi_vals[c] =
                    (1.0 / f64::max(
                        1.0 / (adj_params.hwy_intercept
                            + (adj_params.hwy_slope
                                / ((1.0 / phev_calc.lab_iter_kwh_per_mi[c])
                                    * fuel_props.kwh_per_gge()))),
                        (1.0 - max_epa_adj)
                            * ((1.0 / phev_calc.lab_iter_kwh_per_mi[c]) * fuel_props.kwh_per_gge()),
                    )) * fuel_props.kwh_per_gge();
            }
        }
    }
    phev_calc.adj_iter_mpgge = adj_iter_mpgge_vals;
    phev_calc.adj_iter_kwh_per_mi = adj_iter_kwh_per_mi_vals;

    phev_calc.adj_iter_cd_miles = vec![0.0; phev_calc.cd_cycs.ceil() as usize + 2];
    for c in 0..phev_calc.adj_iter_cd_miles.len() {
        if c == 0 {
            phev_calc.adj_iter_cd_miles[c] = 0.0;
        } else if c <= phev_calc.cd_cycs.floor() as usize {
            phev_calc.adj_iter_cd_miles[c] = phev_calc.adj_iter_cd_miles[c - 1]
                + phev_calc.cd_ess_kwh_per_mi * data.cyc_dist_mi / phev_calc.adj_iter_kwh_per_mi[c];
        } else if c == phev_calc.cd_cycs.floor() as usize + 1 {
            phev_calc.adj_iter_cd_miles[c] = phev_calc.adj_iter_cd_miles[c - 1]
                + phev_calc.trans_ess_kwh_per_mi * data.cyc_dist_mi
                    / phev_calc.adj_iter_kwh_per_mi[c];
        } else {
            phev_calc.adj_iter_cd_miles[c] = 0.0;
        }
    }

    phev_calc.adj_cd_miles =
        if info.max_soc - label_fe_phev.regen_soc_buffer - (data.cs_min_soc * uc::R) < 0.01 * uc::R
        {
            1000.0
        } else {
            *phev_calc.adj_iter_cd_miles.max()?
        };

    // utility factor calculation for last charge depletion iteration and transition iteration
    // ported from excel

    phev_calc.adj_iter_uf = vec![];
    for x in phev_calc.adj_iter_cd_miles.clone() {
        phev_calc.adj_iter_uf.push(
            phev_utilization_params.uf_array[first_grtr(
                &phev_utilization_params.rechg_freq_miles,
                x,
            )
            .with_context(|| format_dbg!())?
                - 1],
        )
    }

    let adj_iter_uf_diff = phev_calc.adj_iter_uf.diff();
    phev_calc.adj_iter_uf_gpm = vec![0.0; phev_calc.cd_cycs.floor() as usize];
    phev_calc.adj_iter_uf_gpm.push(
        (1.0 / phev_calc.adj_iter_mpgge[phev_calc.adj_iter_mpgge.len() - 2])
            * adj_iter_uf_diff[adj_iter_uf_diff.len() - 2],
    );
    phev_calc.adj_iter_uf_gpm.push(
        (1.0 / phev_calc
            .adj_iter_mpgge
            .last()
            .with_context(|| format_dbg!())?)
            * (1.0 - phev_calc.adj_iter_uf[phev_calc.adj_iter_uf.len() - 2]),
    );

    let adj_uf_diff = phev_calc.adj_iter_uf.diff();
    phev_calc.adj_iter_uf_kwh_per_mi = phev_calc
        .adj_iter_kwh_per_mi
        .iter()
        .zip(adj_uf_diff.iter())
        .map(|(kwh, uf)| kwh * uf)
        .collect();

    let max_uf: f64 = phev_calc
        .adj_iter_uf
        .iter()
        .fold(0.0f64, |acc, x| acc.max(*x));
    phev_calc.adj_cd_mpgge =
        1.0 / phev_calc.adj_iter_uf_gpm[phev_calc.adj_iter_uf_gpm.len() - 2] * max_uf;
    phev_calc.adj_cs_mpgge = 1.0
        / phev_calc
            .adj_iter_uf_gpm
            .last()
            .with_context(|| format_dbg!())?
        * (1.0 - max_uf);

    phev_calc.adj_uf = phev_utilization_params.uf_array[first_grtr(
        &phev_utilization_params.rechg_freq_miles,
        phev_calc.adj_cd_miles,
    )
    .with_context(|| format_dbg!())?
        - 1];

    phev_calc.adj_mpgge = 1.0
        / (phev_calc.adj_uf / phev_calc.adj_cd_mpgge
            + (1.0 - phev_calc.adj_uf) / phev_calc.adj_cs_mpgge);

    let uf_kwh_sum: f64 = phev_calc
        .adj_iter_uf_kwh_per_mi
        .iter()
        .fold(0.0, |acc, x| acc + x);
    phev_calc.adj_kwh_per_mi = uf_kwh_sum / max_uf / info.chg_eff;

    phev_calc.adj_ess_kwh_per_mi = uf_kwh_sum / max_uf;

    Ok(phev_calc)
}

/// This is a pure function that calculates the label fuel economy given
/// simulation results.
pub fn calculate_label_fuel_economy(
    fuel_props: &FuelProperties,
    phev_utilization_params: &PhevUtilizationParams,
    max_epa_adj: f64,
    sim_data: &SimulationDataForLabel,
    accel_data: &AccelData,
) -> anyhow::Result<LabelFe> {
    let mut label_fe = LabelFe::default();
    let veh_year = match sim_data {
        SimulationDataForLabel::ConvOrHev { veh_year, .. }
        | SimulationDataForLabel::Phev { veh_year, .. }
        | SimulationDataForLabel::Bev { veh_year, .. } => *veh_year,
    };
    // find year-based adjustment parameters
    let adj_params = if veh_year < 2017 {
        &phev_utilization_params.adj_coef_map["2008"]
    } else {
        // assume 2017 coefficients are valid
        &phev_utilization_params.adj_coef_map["2017"]
    };
    label_fe.adj_params = adj_params.clone();
    match sim_data {
        SimulationDataForLabel::ConvOrHev {
            udds_mpgge,
            hwy_mpgge,
            fs_energy_capacity_kwh: fuel_storage_capacity_kwh,
            ..
        } => {
            // compare to Excel 'VehicleIO'!C203 or 'VehicleIO'!labUddsMpgge
            label_fe.lab_udds_mpgge = *udds_mpgge;
            label_fe.lab_hwy_mpgge = *hwy_mpgge;
            label_fe.lab_comb_mpgge = 1.0 / (0.55 / *udds_mpgge + 0.45 / *hwy_mpgge);
            label_fe.lab_udds_kwh_per_mi = 0.0;
            label_fe.lab_hwy_kwh_per_mi = 0.0;
            label_fe.lab_comb_kwh_per_mi = 0.0;
            // non-EV case
            // CV or HEV case (not PHEV)
            // HEV SOC iteration is handled in simdrive.SimDriveClassic
            label_fe.adj_udds_mpgge =
                1. / (adj_params.city_intercept + adj_params.city_slope / udds_mpgge);
            // compare to Excel 'VehicleIO'!C203 or 'VehicleIO'!adjHwyMpgge
            label_fe.adj_hwy_mpgge =
                1. / (adj_params.hwy_intercept + adj_params.hwy_slope / hwy_mpgge);
            label_fe.adj_comb_mpgge =
                1. / (0.55 / label_fe.adj_udds_mpgge + 0.45 / label_fe.adj_hwy_mpgge);
            let fuel_energy_gge = fuel_storage_capacity_kwh / fuel_props.kwh_per_gge();
            label_fe.net_range_miles = fuel_energy_gge * label_fe.adj_comb_mpgge;
        }
        SimulationDataForLabel::Phev {
            info, udds, hwy, ..
        } => {
            let mut phev_calcs = LabelFePHEV {
                regen_soc_buffer: ((0.5 * info.veh_mass * ((60. * uc::MPH).powi(P2::new())))
                    * info.phev_max_regen
                    * info.em_peak_eff
                    / info.energy_capacity)
                    .min((info.max_soc - info.min_soc) / 2.0),
                ..Default::default()
            };
            // UDDS
            phev_calcs.udds = calculate_phev_label_helper(
                info,
                udds,
                &fuel_props,
                max_epa_adj,
                &phev_utilization_params,
                &adj_params,
                &phev_calcs,
                true,
            )?;
            // HWY
            phev_calcs.hwy = calculate_phev_label_helper(
                info,
                hwy,
                &fuel_props,
                max_epa_adj,
                &phev_utilization_params,
                &adj_params,
                &phev_calcs,
                false,
            )?;
            // efficiency-related calculations
            // lab
            label_fe.lab_udds_mpgge = phev_calcs.udds.lab_mpgge;
            label_fe.lab_hwy_mpgge = phev_calcs.hwy.lab_mpgge;
            label_fe.lab_comb_mpgge =
                1.0 / (0.55 / phev_calcs.udds.lab_mpgge + 0.45 / phev_calcs.hwy.lab_mpgge);

            label_fe.lab_udds_kwh_per_mi = phev_calcs.udds.lab_kwh_per_mi;
            label_fe.lab_hwy_kwh_per_mi = phev_calcs.hwy.lab_kwh_per_mi;
            label_fe.lab_comb_kwh_per_mi =
                0.55 * phev_calcs.udds.lab_kwh_per_mi + 0.45 * phev_calcs.hwy.lab_kwh_per_mi;

            // adjusted
            label_fe.adj_udds_mpgge = phev_calcs.udds.adj_mpgge;
            label_fe.adj_hwy_mpgge = phev_calcs.hwy.adj_mpgge;
            label_fe.adj_comb_mpgge =
                1.0 / (0.55 / phev_calcs.udds.adj_mpgge + 0.45 / phev_calcs.hwy.adj_mpgge);

            label_fe.adj_cs_comb_mpgge = Some(
                1.0 / (0.55 / phev_calcs.udds.adj_cs_mpgge + 0.45 / phev_calcs.hwy.adj_cs_mpgge),
            );
            label_fe.adj_cd_comb_mpgge = Some(
                1.0 / (0.55 / phev_calcs.udds.adj_cd_mpgge + 0.45 / phev_calcs.hwy.adj_cd_mpgge),
            );

            label_fe.adj_udds_kwh_per_mi = phev_calcs.udds.adj_kwh_per_mi;
            label_fe.adj_hwy_kwh_per_mi = phev_calcs.hwy.adj_kwh_per_mi;
            label_fe.adj_comb_kwh_per_mi =
                0.55 * phev_calcs.udds.adj_kwh_per_mi + 0.45 * phev_calcs.hwy.adj_kwh_per_mi;

            label_fe.adj_udds_ess_kwh_per_mi = phev_calcs.udds.adj_ess_kwh_per_mi;
            label_fe.adj_hwy_ess_kwh_per_mi = phev_calcs.hwy.adj_ess_kwh_per_mi;
            label_fe.adj_comb_ess_kwh_per_mi = 0.55 * phev_calcs.udds.adj_ess_kwh_per_mi
                + 0.45 * phev_calcs.hwy.adj_ess_kwh_per_mi;

            // range for combined city/highway
            // utility factor (percent driving in charge depletion mode)
            label_fe.uf = Some(
                phev_utilization_params.uf_array[first_grtr(
                    &phev_utilization_params.rechg_freq_miles,
                    0.55 * phev_calcs.udds.adj_cd_miles + 0.45 * phev_calcs.hwy.adj_cd_miles,
                )
                .with_context(|| format_dbg!())?
                    - 1],
            );

            label_fe.net_phev_cd_miles =
                Some(0.55 * phev_calcs.udds.adj_cd_miles + 0.45 * phev_calcs.hwy.adj_cd_miles);

            // For PHEVs, calculate net range as the sum of CD range and CS range
            // Get CS range by determining how much fuel energy remains after depleting the battery
            let fuel_energy_kwh = info.fuel_storage_capacity.get::<si::kilowatt_hour>();
            let fuel_energy_gge = fuel_energy_kwh / fuel_props.kwh_per_gge();

            label_fe.net_range_miles = (fuel_energy_gge
                - label_fe.net_phev_cd_miles.with_context(|| format_dbg!())?
                    / label_fe.adj_cd_comb_mpgge.with_context(|| format_dbg!())?)
                * label_fe.adj_cs_comb_mpgge.with_context(|| format_dbg!())?
                + label_fe.net_phev_cd_miles.with_context(|| format_dbg!())?;

            label_fe.phev_calcs = Some(phev_calcs);
        }
        SimulationDataForLabel::Bev {
            udds_kwh_per_mi,
            hwy_kwh_per_mi,
            bev_energy_capacity_kwh,
            ..
        } => {
            label_fe.lab_udds_mpgge = 0.0;
            label_fe.lab_hwy_mpgge = 0.0;
            label_fe.lab_comb_mpgge = 0.0;
            label_fe.lab_udds_kwh_per_mi = *udds_kwh_per_mi;
            label_fe.lab_hwy_kwh_per_mi = *hwy_kwh_per_mi;
            label_fe.lab_comb_kwh_per_mi = 0.55 * *udds_kwh_per_mi + 0.45 * *hwy_kwh_per_mi;
            // EV case
            // Mpgge is all zero for EV
            label_fe.adj_udds_mpgge = 0.;
            label_fe.adj_hwy_mpgge = 0.;
            label_fe.adj_comb_mpgge = 0.;
            // EV Case
            label_fe.adj_udds_kwh_per_mi =
                (1. / f64::max(
                    1. / (adj_params.city_intercept
                        + (adj_params.city_slope
                            / ((1. / label_fe.lab_udds_kwh_per_mi) * fuel_props.kwh_per_gge()))),
                    (1. / label_fe.lab_udds_kwh_per_mi)
                        * fuel_props.kwh_per_gge()
                        * (1. - max_epa_adj),
                )) * fuel_props.kwh_per_gge()
                    / DEFAULT_CHG_EFF;
            label_fe.adj_hwy_kwh_per_mi =
                (1. / f64::max(
                    1. / (adj_params.hwy_intercept
                        + (adj_params.hwy_slope
                            / ((1. / label_fe.lab_hwy_kwh_per_mi) * fuel_props.kwh_per_gge()))),
                    (1. / label_fe.lab_hwy_kwh_per_mi)
                        * fuel_props.kwh_per_gge()
                        * (1. - max_epa_adj),
                )) * fuel_props.kwh_per_gge()
                    / DEFAULT_CHG_EFF;
            label_fe.adj_comb_kwh_per_mi =
                0.55 * label_fe.adj_udds_kwh_per_mi + 0.45 * label_fe.adj_hwy_kwh_per_mi;

            label_fe.adj_udds_ess_kwh_per_mi = label_fe.adj_udds_kwh_per_mi * DEFAULT_CHG_EFF;
            label_fe.adj_hwy_ess_kwh_per_mi = label_fe.adj_hwy_kwh_per_mi * DEFAULT_CHG_EFF;
            label_fe.adj_comb_ess_kwh_per_mi = label_fe.adj_comb_kwh_per_mi * DEFAULT_CHG_EFF;

            // range for combined city/highway
            // Get energy capacity from the proper powertrain
            label_fe.net_range_miles = bev_energy_capacity_kwh / label_fe.adj_comb_ess_kwh_per_mi;
        }
    }

    // process acceleration test data
    label_fe.net_accel = get_0_to_60_time_from_accel_data(accel_data).map_err(|e| {
        anyhow!(
            "get_0_to_60_time_from_accel_data failed at line {} with originating error [{}]",
            format_dbg!(),
            e
        )
    })?;

    // success Boolean -- did all of the tests work(e.g. met trace within ~2 mph)?
    label_fe.res_found = String::from("model needs to be implemented for this");

    Ok(label_fe)
}

fn run_simdrive_with_init_soc(
    veh: &Vehicle,
    cycle: &str,
    init_soc: si::Ratio,
    sim_params: Option<SimParams>,
) -> anyhow::Result<SimDrive> {
    let mut sd = SimDrive::new(veh.clone(), Cycle::from_resource(cycle, false)?, sim_params);
    let res_mut = sd.veh.res_mut().with_context(|| format_dbg!())?;
    res_mut.state.soc.mark_stale();
    res_mut.state.soc.update(init_soc, || format_dbg!())?;
    sd.reset_cumulative(|| format_dbg!())?;
    sd.reset_step(|| format_dbg!())?;
    sd.clear();
    sd.run_once().map_err(|e| {
        anyhow!(
            "run_simdrive_with_init_soc failed at line {} with originating error [{}]",
            format_dbg!(),
            e
        )
    })?;
    Ok(sd)
}

/// Runs the appropriate simulations required for calculating
/// the label fuel economy for the given vehicle.
/// NOTE: does not run the acceleration test.
pub fn run_label_simulations(
    veh: &mut Vehicle,
    // max_epa_adj: Option<f64>,
    fuel_props: Option<FuelProperties>,
    phev_utilization_params: Option<PhevUtilizationParams>,
    sim_params: &HashMap<&'static str, SimParams>,
) -> anyhow::Result<(SimulationDataForLabel, HashMap<&'static str, SimDrive>)> {
    // let max_epa_adj = max_epa_adj.unwrap_or(0.3);
    let phev_utilization_params = &phev_utilization_params.unwrap_or_default();
    let fuel_props = fuel_props.unwrap_or_default();

    let mut cyc: HashMap<&str, Cycle> = HashMap::new();
    let mut sd = HashMap::new();
    let mut label_fe = LabelFe::default();

    label_fe.veh = Some(veh.clone());

    // load the cycles and instantiate simdrive objects
    cyc.insert("accel", CYC_ACCEL.clone());
    cyc.insert("udds", Cycle::from_resource("udds.csv", false)?);
    cyc.insert("hwy", Cycle::from_resource("hwfet.csv", false)?);

    if veh.pt_type.is_plug_in_hybrid_electric_vehicle() {
        let rm = veh.res_mut().unwrap();
        rm.state.soc.check_and_reset(|| format_dbg!()).unwrap();
        rm.state.soc.update(rm.max_soc, || format_dbg!()).unwrap();
    }

    // run simdrive for non-phev powertrains
    sd.insert(
        "udds",
        SimDrive::new(
            veh.clone(),
            cyc["udds"].clone(),
            sim_params.get("udds").cloned(),
        ),
    );
    sd.insert(
        "hwy",
        SimDrive::new(
            veh.clone(),
            cyc["hwy"].clone(),
            sim_params.get("hwy").cloned(),
        ),
    );

    for (k, val) in sd.iter_mut() {
        val.run().map_err(|e| {
            anyhow!(
                "run_label_simulations failed for key {} at line {} with originating error [{}]",
                k,
                format_dbg!(),
                e
            )
        })?;
    }

    let veh_year = parse_model_year_or_2017(veh.year.as_deref());

    // find year-based adjustment parameters
    let adj_params = if veh_year < 2017 {
        &phev_utilization_params.adj_coef_map["2008"]
    } else {
        // assume 2017 coefficients are valid
        &phev_utilization_params.adj_coef_map["2017"]
    };
    label_fe.adj_params = adj_params.clone();

    // Check powertrain type
    let is_conv = matches!(veh.pt_type, PowertrainType::ConventionalVehicle(_));
    let is_hev = matches!(veh.pt_type, PowertrainType::HybridElectricVehicle(_));
    let is_phev = matches!(veh.pt_type, PowertrainType::PlugInHybridElectricVehicle(_));
    let is_bev = matches!(veh.pt_type, PowertrainType::BatteryElectricVehicle(_));

    if is_hev || is_conv {
        Ok((
            SimulationDataForLabel::ConvOrHev {
                veh_year,
                udds_mpgge: sd["udds"].veh.mpg(fuel_props.energy_density)?,
                hwy_mpgge: sd["hwy"].veh.mpg(fuel_props.energy_density)?,
                fs_energy_capacity_kwh: veh
                    .pt_type
                    .fs()
                    .map(|fs| fs.energy_capacity.get::<si::kilowatt_hour>())
                    .unwrap_or(0.0),
            },
            sd,
        ))
    } else if is_bev {
        if let PowertrainType::BatteryElectricVehicle(bev) = &veh.pt_type {
            let res_energy_capacity_kwh =
                bev.res.energy_capacity_usable().get::<si::kilowatt_hour>();
            Ok((
                SimulationDataForLabel::Bev {
                    veh_year,
                    udds_kwh_per_mi: sd["udds"].veh.kwh_per_mi()?,
                    hwy_kwh_per_mi: sd["hwy"].veh.kwh_per_mi()?,
                    bev_energy_capacity_kwh: res_energy_capacity_kwh,
                },
                sd,
            ))
        } else {
            bail!("is_bev but powertrain not BEV")
        }
    } else if is_phev {
        // Get access to the PHEV powertrain
        let max_soc: si::Ratio;
        let min_soc: si::Ratio;
        let phev_max_regen: si::Ratio;
        let veh_mass: si::Mass;
        // equivalent to fastsim-2 `mc_peak_eff`
        let em_peak_eff: si::Ratio;
        // battery total energy capacity from soc of 1.0 to 0.0
        let energy_capacity: si::Energy;
        let chg_eff: f64;
        let fuel_storage_capacity: si::Energy;
        if let PowertrainType::PlugInHybridElectricVehicle(phev) = &veh.pt_type {
            max_soc = phev.res.max_soc;
            min_soc = phev.res.min_soc;
            phev_max_regen = 0.98 * uc::R;
            veh_mass = *veh.state.mass.get_fresh(|| format_dbg!())?;
            em_peak_eff = *phev
                .em
                .eff_interp_achieved
                .max()
                .with_context(|| format_dbg!())?
                * uc::R;
            energy_capacity = phev.res.energy_capacity;
            chg_eff = DEFAULT_CHG_EFF;
            fuel_storage_capacity = phev.fs.energy_capacity;
        } else {
            bail!("Vehicle is not a PHEV");
        }

        // Create SimDrive objects for Charge Sustaining PHEV calculations
        let init_soc = min_soc + 0.01 * uc::R;
        let cs_udds_sd =
            run_simdrive_with_init_soc(veh, "udds.csv", init_soc, sim_params.get("udds").cloned())?;
        let cs_hwy_sd =
            run_simdrive_with_init_soc(veh, "hwfet.csv", init_soc, sim_params.get("hwy").cloned())?;
        sd.insert("udds-cs", cs_udds_sd.clone());
        sd.insert("hwy-cs", cs_hwy_sd.clone());
        Ok((
            SimulationDataForLabel::Phev {
                veh_year,
                info: PhevVehicleInfo {
                    max_soc,
                    min_soc,
                    phev_max_regen,
                    veh_mass,
                    em_peak_eff,
                    energy_capacity,
                    chg_eff,
                    fuel_storage_capacity,
                },
                udds: PhevSimulationDataForLabel {
                    cd_fuel_consumed_kwh: {
                        if let Some(fc) = sd["udds"].veh.fc() {
                            fc.state
                                .energy_fuel
                                .get_fresh(|| format_dbg!())?
                                .get::<si::kilowatt_hour>()
                        } else {
                            0.0
                        }
                    },
                    cd_soc_start: {
                        if let Some(res) = sd["udds"].veh.res() {
                            res.history
                                .soc
                                .first()
                                .unwrap()
                                .get_fresh(|| format_dbg!())?
                                .get::<si::ratio>()
                        } else {
                            1.0
                        }
                    },
                    cd_soc_end: {
                        if let Some(res) = sd["udds"].veh.res() {
                            res.history
                                .soc
                                .last()
                                .unwrap()
                                .get_fresh(|| format_dbg!())?
                                .get::<si::ratio>()
                        } else {
                            0.0
                        }
                    },
                    cyc_dist_mi: {
                        sd["udds"]
                            .veh
                            .state
                            .dist
                            .get_fresh(|| format_dbg!())?
                            .get::<si::mile>()
                    },
                    cd_kwh_per_mi: sd["udds"].veh.kwh_per_mi()?,
                    cd_mpg: sd["udds"].veh.mpg(fuel_props.energy_density)?,
                    cs_fuel_consumed_kwh: {
                        if let Some(fc) = cs_udds_sd.veh.fc() {
                            fc.state
                                .energy_fuel
                                .get_fresh(|| format_dbg!())?
                                .get::<si::kilowatt_hour>()
                        } else {
                            0.0
                        }
                    },
                    cs_ess_energy_kwh: {
                        if let Some(res) = cs_udds_sd.veh.res() {
                            res.state
                                .energy_out_chemical
                                .get_fresh(|| format_dbg!())?
                                .get::<si::kilowatt_hour>()
                        } else {
                            0.0
                        }
                    },
                    cs_kwh_per_mi: cs_udds_sd.veh.kwh_per_mi()?,
                    cs_mpg: cs_udds_sd.veh.mpg(fuel_props.energy_density)?,
                    cs_min_soc: min_soc.get::<si::ratio>(),
                    cs_fs_energy_capacity_kwh: {
                        if let Some(fs) = veh.pt_type.fs() {
                            fs.energy_capacity.get::<si::kilowatt_hour>()
                        } else {
                            0.0
                        }
                    },
                },
                hwy: PhevSimulationDataForLabel {
                    cd_fuel_consumed_kwh: {
                        if let Some(fc) = sd["hwy"].veh.fc() {
                            fc.state
                                .energy_fuel
                                .get_fresh(|| format_dbg!())?
                                .get::<si::kilowatt_hour>()
                        } else {
                            0.0
                        }
                    },
                    cd_soc_start: {
                        if let Some(res) = sd["hwy"].veh.res() {
                            res.history
                                .soc
                                .first()
                                .unwrap()
                                .get_fresh(|| format_dbg!())?
                                .get::<si::ratio>()
                        } else {
                            1.0
                        }
                    },
                    cd_soc_end: {
                        if let Some(res) = sd["hwy"].veh.res() {
                            res.history
                                .soc
                                .last()
                                .unwrap()
                                .get_fresh(|| format_dbg!())?
                                .get::<si::ratio>()
                        } else {
                            0.0
                        }
                    },
                    cyc_dist_mi: {
                        sd["hwy"]
                            .veh
                            .state
                            .dist
                            .get_fresh(|| format_dbg!())?
                            .get::<si::mile>()
                    },
                    cd_kwh_per_mi: sd["hwy"].veh.kwh_per_mi()?,
                    cd_mpg: sd["hwy"].veh.mpg(fuel_props.energy_density)?,
                    cs_fuel_consumed_kwh: {
                        if let Some(fc) = cs_hwy_sd.veh.fc() {
                            fc.state
                                .energy_fuel
                                .get_fresh(|| format_dbg!())?
                                .get::<si::kilowatt_hour>()
                        } else {
                            0.0
                        }
                    },
                    cs_ess_energy_kwh: {
                        if let Some(res) = cs_hwy_sd.veh.res() {
                            res.state
                                .energy_out_chemical
                                .get_fresh(|| format_dbg!())?
                                .get::<si::kilowatt_hour>()
                        } else {
                            0.0
                        }
                    },
                    cs_kwh_per_mi: cs_hwy_sd.veh.kwh_per_mi()?,
                    cs_mpg: cs_hwy_sd.veh.mpg(fuel_props.energy_density)?,
                    cs_min_soc: min_soc.get::<si::ratio>(),
                    cs_fs_energy_capacity_kwh: {
                        if let Some(fs) = veh.pt_type.fs() {
                            fs.energy_capacity.get::<si::kilowatt_hour>()
                        } else {
                            0.0
                        }
                    },
                },
            },
            sd,
        ))
    } else {
        bail!("Unhandled powertrain type")
    }
}

/// Generates label fuel economy (FE) values for a provided vehicle.
///
/// # Arguments
///
/// - `veh`: vehicle::Vehicle
/// - `full_detail`: boolean, default False
///   If True, sim_drive objects for each cycle are also returned.
/// - `verbose`: boolean, default false
///   If true, print out key results
///
/// Returns label fuel economy values as a struct and (optionally)
/// simdrive::SimDrive objects.
pub fn get_label_fe(
    veh: &mut Vehicle,
    max_epa_adj: Option<f64>,
    full_detail: bool,
    fuel_props: Option<FuelProperties>,
    phev_utilization_params: Option<PhevUtilizationParams>,
    sim_params: Option<HashMap<&'static str, SimParams>>,
    verbose: bool,
) -> anyhow::Result<(LabelFe, Option<HashMap<&'static str, SimDrive>>)> {
    let max_epa_adj = max_epa_adj.unwrap_or(0.3);
    let phev_utilization_params = &phev_utilization_params.unwrap_or_default();
    let fuel_props = fuel_props.unwrap_or_default();
    let veh_copy = veh.clone();

    let sim_params = sim_params.unwrap_or_default();

    let (sim_data, sd) = run_label_simulations(
        veh,
        Some(fuel_props.clone()),
        Some(phev_utilization_params.clone()),
        &sim_params,
    )?;
    let accel_data = run_accel(&veh_copy, &sim_params)?;
    let mut label_fe = calculate_label_fuel_economy(
        &fuel_props,
        phev_utilization_params,
        max_epa_adj,
        &sim_data,
        &accel_data,
    )?;
    label_fe.veh = Some(veh_copy);

    if full_detail && verbose {
        println!("{label_fe:#?}");
        Ok((label_fe, Some(sd)))
    } else if full_detail {
        Ok((label_fe, Some(sd)))
    } else if verbose {
        println!("{label_fe:#?}");
        Ok((label_fe, None))
    } else {
        Ok((label_fe, None))
    }
}

#[cfg(feature = "pyo3")]
#[pyfunction(name = "get_label_fe")]
#[cfg_attr(
    feature = "pyo3",
    pyo3(signature = (
        veh, max_epa_adj=None, full_detail=None, fuel_props=None, phev_utilization_params=None, sim_params=None, verbose=None))
)]
/// pyo3 version of [get_label_fe]
pub fn get_label_fe_py(
    veh: &mut Vehicle,
    max_epa_adj: Option<f64>,
    full_detail: Option<bool>,
    fuel_props: Option<FuelProperties>,
    phev_utilization_params: Option<PhevUtilizationParams>,
    sim_params: Option<HashMap<String, SimParams>>,
    verbose: Option<bool>,
) -> anyhow::Result<LabelFe> {
    let (label_fe, _) = get_label_fe(
        veh,
        max_epa_adj,
        full_detail.unwrap_or_default(),
        fuel_props,
        phev_utilization_params,
        sim_params
            .map(|m| {
                m.into_iter()
                    .map(|(k, v)| -> anyhow::Result<(&'static str, SimParams)> {
                        let key = match k.as_str() {
                            "udds" => "udds",
                            "hwy" => "hwy",
                            "accel" => "accel",
                            other => bail!("Unknown sim_params key: {other:?}"),
                        };
                        Ok((key, v))
                    })
                    .collect()
            })
            .transpose()?,
        verbose.unwrap_or_default(),
    )?;
    Ok(label_fe)
}

/// PHEV-specific function for label fe.
///
/// # Arguments
/// - max_epa_adj: maximum EPA adjustment factor
///
/// # Returns
/// label fuel economy values for PHEV as a struct.
pub fn get_label_fe_phev(
    veh: &Vehicle,
    phev_utilization_params: &PhevUtilizationParams,
    adj_params: &AdjCoef,
    max_epa_adj: f64,
    fuel_props: &FuelProperties,
) -> anyhow::Result<LabelFePHEV> {
    // Get access to the PHEV powertrain
    let max_soc: si::Ratio;
    let min_soc: si::Ratio;
    let phev_max_regen: si::Ratio;
    let veh_mass: si::Mass;
    // equivalent to fastsim-2 `mc_peak_eff`
    let em_peak_eff: si::Ratio;
    // battery total energy capacity from soc of 1.0 to 0.0
    let energy_capacity: si::Energy;
    let chg_eff: f64;

    if let PowertrainType::PlugInHybridElectricVehicle(phev) = &veh.pt_type {
        max_soc = phev.res.max_soc;
        min_soc = phev.res.min_soc;
        phev_max_regen = 0.98 * uc::R;
        veh_mass = *veh.state.mass.get_fresh(|| format_dbg!())?;
        em_peak_eff = *phev
            .em
            .eff_interp_achieved
            .max()
            .with_context(|| format_dbg!())?
            * uc::R;
        energy_capacity = phev.res.energy_capacity;
        chg_eff = DEFAULT_CHG_EFF; // Use default charging efficiency
    } else {
        bail!("Vehicle is not a PHEV");
    }

    let mut label_fe_phev = LabelFePHEV {
        regen_soc_buffer: ((0.5 * veh_mass * ((60. * uc::MPH).powi(P2::new())))
            * phev_max_regen
            * em_peak_eff
            / energy_capacity)
            .min((max_soc - min_soc) / 2.0),
        ..Default::default()
    };

    // Create SimDrive objects for PHEV calculations
    let mut sd: HashMap<&str, SimDrive> = HashMap::new();
    sd.insert(
        "udds",
        SimDrive::new(veh.clone(), Cycle::from_resource("udds.csv", false)?, None),
    );
    sd.insert(
        "hwy",
        SimDrive::new(veh.clone(), Cycle::from_resource("hwfet.csv", false)?, None),
    );

    // charge sustaining behavior
    for (key, sd) in sd.iter_mut() {
        // do PHEV soc iteration
        // This runs 1 cycle starting at max SOC then runs 1 cycle starting at min SOC.
        // By assuming that the battery SOC depletion per mile is constant across cycles,
        // the first cycle can be extrapolated until charge sustaining kicks in.
        sd.run()?;
        let mut phev_calc = PHEVCycleCalc::default();

        // charge depletion cycle has already been simulated
        // charge depletion battery kW-hr
        phev_calc.cd_ess_kwh = ((max_soc - min_soc) * energy_capacity).get::<si::kilowatt_hour>();

        // Get SOC and distance values
        let res = sd.veh.res().with_context(|| format_dbg!())?;
        let soc_start = *res
            .history
            .soc
            .first()
            .with_context(|| format_dbg!())?
            .get_fresh(|| format_dbg!())?;
        let soc_end = *res
            .history
            .soc
            .last()
            .with_context(|| format_dbg!())?
            .get_fresh(|| format_dbg!())?;
        let dist_mi = sd
            .veh
            .state
            .dist
            .get_fresh(|| format_dbg!())?
            .get::<si::mile>();

        // SOC change during 1 cycle
        phev_calc.delta_soc = soc_start - soc_end;
        // total number of miles in charge depletion mode, assuming constant kWh_per_mi
        phev_calc.total_cd_miles = ((max_soc - min_soc) * energy_capacity)
            .get::<si::kilowatt_hour>()
            / sd.veh.kwh_per_mi()?;
        // number of cycles in charge depletion mode, up to transition
        phev_calc.cd_cycs = phev_calc.total_cd_miles / dist_mi;
        // fraction of transition cycle spent in charge depletion
        phev_calc.cd_frac_in_trans = phev_calc.cd_cycs % phev_calc.cd_cycs.floor();

        // charge depletion fuel gallons - get from fuel converter
        let fuel_energy_kwh = if let Some(fc) = sd.veh.fc() {
            fc.state
                .energy_fuel
                .get_fresh(|| format_dbg!())?
                .get::<si::kilowatt_hour>()
        } else {
            0.0
        };
        phev_calc.cd_fs_gal = fuel_energy_kwh / fuel_props.kwh_per_gge();
        phev_calc.cd_fs_kwh = fuel_energy_kwh;
        phev_calc.cd_ess_kwh_per_mi = sd.veh.kwh_per_mi()?;
        phev_calc.cd_mpg = sd.veh.mpg(fuel_props.energy_density)?;

        // utility factor calculation for last charge depletion iteration and transition iteration
        // ported from excel
        let interp_x_vals: Vec<f64> = (0..((phev_calc.cd_cycs.ceil() + 1.0) as usize))
            .map(|i| i as f64 * dist_mi)
            .collect();

        phev_calc.lab_iter_uf = vec![];
        for x in interp_x_vals {
            phev_calc.lab_iter_uf.push(
                phev_utilization_params.uf_array[first_grtr(
                    &phev_utilization_params.rechg_freq_miles,
                    x,
                )
                .with_context(|| format_dbg!())?
                    - 1],
            );
        }

        // transition cycle
        phev_calc.trans_init_soc = max_soc - phev_calc.cd_cycs.floor() * phev_calc.delta_soc;

        // run the transition cycle by setting initial SOC
        let res_mut = sd.veh.res_mut().with_context(|| format_dbg!())?;
        res_mut.state.soc.mark_stale();
        res_mut
            .state
            .soc
            .update(phev_calc.trans_init_soc, || format_dbg!())?;
        sd.reset_cumulative(|| format_dbg!())?;
        sd.reset_step(|| format_dbg!())?;
        sd.clear();
        sd.run_once().map_err(|err| {
            anyhow!(
                "run_once failed at line {} with originating error {}",
                format_dbg!(),
                err
            )
        })?;

        // charge depletion battery kW-hr
        phev_calc.trans_ess_kwh =
            phev_calc.cd_ess_kwh_per_mi * dist_mi * phev_calc.cd_frac_in_trans;
        phev_calc.trans_ess_kwh_per_mi = phev_calc.cd_ess_kwh_per_mi * phev_calc.cd_frac_in_trans;

        // charge sustaining
        // the 0.01 is here to be consistent with Excel
        let init_soc = min_soc + 0.01 * uc::R;
        let res_mut = sd.veh.res_mut().with_context(|| format_dbg!())?;
        res_mut.state.soc.mark_stale();
        res_mut.state.soc.update(init_soc, || format_dbg!())?;
        sd.reset_cumulative(|| format_dbg!())?;
        sd.reset_step(|| format_dbg!())?;
        sd.clear();
        sd.run_once().map_err(|err| {
            anyhow!(
                "run_once failed at line {} with originating error {}",
                format_dbg!(),
                err
            )
        })?;

        // charge sustaining fuel gallons
        let cs_fuel_energy_kwh = if let Some(fc) = sd.veh.fc() {
            fc.state
                .energy_fuel
                .get_fresh(|| format_dbg!())?
                .get::<si::kilowatt_hour>()
        } else {
            0.0
        };
        phev_calc.cs_fs_gal = cs_fuel_energy_kwh / fuel_props.kwh_per_gge();
        // charge depletion fuel gallons, dependent on phev_calc.trans_fs_gal
        phev_calc.trans_fs_gal = phev_calc.cs_fs_gal * (1.0 - phev_calc.cd_frac_in_trans);
        phev_calc.cs_fs_kwh = cs_fuel_energy_kwh;
        phev_calc.trans_fs_kwh = phev_calc.cs_fs_kwh * (1.0 - phev_calc.cd_frac_in_trans);
        // charge sustaining battery kW-hr
        let cs_ess_energy_kwh = if let Some(res) = sd.veh.res() {
            res.state
                .energy_out_chemical
                .get_fresh(|| format_dbg!())?
                .get::<si::kilowatt_hour>()
        } else {
            0.0
        };
        phev_calc.cs_ess_kwh = cs_ess_energy_kwh;
        phev_calc.cs_ess_kwh_per_mi = sd.veh.kwh_per_mi()?;

        let lab_iter_uf_diff = phev_calc.lab_iter_uf.diff();
        phev_calc.lab_uf_gpm = [
            phev_calc.trans_fs_gal * lab_iter_uf_diff.last().with_context(|| format_dbg!())?,
            phev_calc.cs_fs_gal
                * (1.0
                    - phev_calc
                        .lab_iter_uf
                        .last()
                        .with_context(|| format_dbg!())?),
        ]
        .iter()
        .map(|x| *x / dist_mi)
        .collect();

        // TODO: check that below is correct. cd_mpg was already set above and cs_mpg is set below...
        // phev_calc.cd_mpg = sd.veh.mpg(fuel_props.energy_density)?;

        // city and highway cycle ranges
        let min_soc_in_cycle = phev_calc.delta_soc.abs(); // Use delta_soc as proxy for min SOC change
        phev_calc.cd_miles =
            if (max_soc - label_fe_phev.regen_soc_buffer - min_soc_in_cycle) < 0.01 * uc::R {
                1000.0
            } else {
                phev_calc.cd_cycs.ceil() * dist_mi
            };
        phev_calc.cd_lab_mpg = phev_calc
            .lab_iter_uf
            .last()
            .with_context(|| format_dbg!())?
            / (phev_calc.trans_fs_gal / dist_mi);

        // charge sustaining
        phev_calc.cs_mpg = dist_mi / phev_calc.cs_fs_gal;

        phev_calc.lab_uf = phev_utilization_params.uf_array[first_grtr(
            &phev_utilization_params.rechg_freq_miles,
            phev_calc.cd_miles,
        )
        .with_context(|| format_dbg!())?
            - 1];

        // labCombMpgge
        phev_calc.cd_adj_mpg =
            phev_calc.lab_iter_uf.max()? / phev_calc.lab_uf_gpm[phev_calc.lab_uf_gpm.len() - 2];

        phev_calc.lab_mpgge = 1.0
            / (phev_calc.lab_uf / phev_calc.cd_adj_mpg
                + (1.0 - phev_calc.lab_uf) / phev_calc.cs_mpg);

        let mut lab_iter_kwh_per_mi_vals = Vec::new();
        lab_iter_kwh_per_mi_vals.push(0.0);
        lab_iter_kwh_per_mi_vals
            .extend(vec![phev_calc.cd_ess_kwh_per_mi; phev_calc.cd_cycs.floor() as usize].iter());
        lab_iter_kwh_per_mi_vals.push(phev_calc.trans_ess_kwh_per_mi);
        lab_iter_kwh_per_mi_vals.push(0.0);
        phev_calc.lab_iter_kwh_per_mi = lab_iter_kwh_per_mi_vals;

        let uf_diff = phev_calc.lab_iter_uf.diff();
        let mut vals = Vec::new();
        vals.push(0.0);
        for i in 1..phev_calc.lab_iter_kwh_per_mi.len() - 1 {
            if i - 1 < uf_diff.len() {
                vals.push(phev_calc.lab_iter_kwh_per_mi[i] * uf_diff[i - 1]);
            }
        }
        vals.push(0.0);
        phev_calc.lab_iter_uf_kwh_per_mi = vals;

        phev_calc.lab_kwh_per_mi = phev_calc
            .lab_iter_uf_kwh_per_mi
            .iter()
            .fold(0.0, |acc, x| acc + x)
            / phev_calc
                .lab_iter_uf
                .iter()
                .fold(0.0f64, |acc, x| acc.max(*x));

        let mut adj_iter_mpgge_vals = vec![0.0; phev_calc.cd_cycs.floor() as usize];
        let mut adj_iter_kwh_per_mi_vals = vec![0.0; phev_calc.lab_iter_kwh_per_mi.len()];
        if *key == "udds" {
            adj_iter_mpgge_vals.push(f64::max(
                1.0 / (adj_params.city_intercept
                    + (adj_params.city_slope
                        / (sd
                            .veh
                            .state
                            .dist
                            .get_fresh(|| format_dbg!())?
                            .get::<si::mile>()
                            / (phev_calc.trans_fs_kwh / fuel_props.kwh_per_gge())))),
                sd.veh
                    .state
                    .dist
                    .get_fresh(|| format_dbg!())?
                    .get::<si::mile>()
                    / (phev_calc.trans_fs_kwh / fuel_props.kwh_per_gge())
                    * (1.0 - max_epa_adj),
            ));
            adj_iter_mpgge_vals.push(f64::max(
                1.0 / (adj_params.city_intercept
                    + (adj_params.city_slope
                        / (sd
                            .veh
                            .state
                            .dist
                            .get_fresh(|| format_dbg!())?
                            .get::<si::mile>()
                            / (phev_calc.cs_fs_kwh / fuel_props.kwh_per_gge())))),
                sd.veh
                    .state
                    .dist
                    .get_fresh(|| format_dbg!())?
                    .get::<si::mile>()
                    / (phev_calc.cs_fs_kwh / fuel_props.kwh_per_gge())
                    * (1.0 - max_epa_adj),
            ));

            for (c, _) in phev_calc.lab_iter_kwh_per_mi.iter().enumerate() {
                if phev_calc.lab_iter_kwh_per_mi[c] == 0.0 {
                    adj_iter_kwh_per_mi_vals[c] = 0.0;
                } else {
                    adj_iter_kwh_per_mi_vals[c] =
                        (1.0 / f64::max(
                            1.0 / (adj_params.city_intercept
                                + (adj_params.city_slope
                                    / ((1.0 / phev_calc.lab_iter_kwh_per_mi[c])
                                        * fuel_props.kwh_per_gge()))),
                            (1.0 - max_epa_adj)
                                * ((1.0 / phev_calc.lab_iter_kwh_per_mi[c])
                                    * fuel_props.kwh_per_gge()),
                        )) * fuel_props.kwh_per_gge();
                }
            }
        } else {
            adj_iter_mpgge_vals.push(f64::max(
                1.0 / (adj_params.hwy_intercept
                    + (adj_params.hwy_slope
                        / (sd
                            .veh
                            .state
                            .dist
                            .get_fresh(|| format_dbg!())?
                            .get::<si::mile>()
                            / (phev_calc.trans_fs_kwh / fuel_props.kwh_per_gge())))),
                sd.veh
                    .state
                    .dist
                    .get_fresh(|| format_dbg!())?
                    .get::<si::mile>()
                    / (phev_calc.trans_fs_kwh / fuel_props.kwh_per_gge())
                    * (1.0 - max_epa_adj),
            ));
            adj_iter_mpgge_vals.push(f64::max(
                1.0 / (adj_params.hwy_intercept
                    + (adj_params.hwy_slope
                        / (sd
                            .veh
                            .state
                            .dist
                            .get_fresh(|| format_dbg!())?
                            .get::<si::mile>()
                            / (phev_calc.cs_fs_kwh / fuel_props.kwh_per_gge())))),
                sd.veh
                    .state
                    .dist
                    .get_fresh(|| format_dbg!())?
                    .get::<si::mile>()
                    / (phev_calc.cs_fs_kwh / fuel_props.kwh_per_gge())
                    * (1.0 - max_epa_adj),
            ));

            for (c, _) in phev_calc.lab_iter_kwh_per_mi.iter().enumerate() {
                if phev_calc.lab_iter_kwh_per_mi[c] == 0.0 {
                    adj_iter_kwh_per_mi_vals[c] = 0.0;
                } else {
                    adj_iter_kwh_per_mi_vals[c] =
                        (1.0 / f64::max(
                            1.0 / (adj_params.hwy_intercept
                                + (adj_params.hwy_slope
                                    / ((1.0 / phev_calc.lab_iter_kwh_per_mi[c])
                                        * fuel_props.kwh_per_gge()))),
                            (1.0 - max_epa_adj)
                                * ((1.0 / phev_calc.lab_iter_kwh_per_mi[c])
                                    * fuel_props.kwh_per_gge()),
                        )) * fuel_props.kwh_per_gge();
                }
            }
        }
        phev_calc.adj_iter_mpgge = adj_iter_mpgge_vals;
        phev_calc.adj_iter_kwh_per_mi = adj_iter_kwh_per_mi_vals;

        phev_calc.adj_iter_cd_miles = vec![0.0; phev_calc.cd_cycs.ceil() as usize + 2];
        for c in 0..phev_calc.adj_iter_cd_miles.len() {
            if c == 0 {
                phev_calc.adj_iter_cd_miles[c] = 0.0;
            } else if c <= phev_calc.cd_cycs.floor() as usize {
                phev_calc.adj_iter_cd_miles[c] = phev_calc.adj_iter_cd_miles[c - 1]
                    + phev_calc.cd_ess_kwh_per_mi
                        * sd.veh
                            .state
                            .dist
                            .get_fresh(|| format_dbg!())?
                            .get::<si::mile>()
                        / phev_calc.adj_iter_kwh_per_mi[c];
            } else if c == phev_calc.cd_cycs.floor() as usize + 1 {
                phev_calc.adj_iter_cd_miles[c] = phev_calc.adj_iter_cd_miles[c - 1]
                    + phev_calc.trans_ess_kwh_per_mi
                        * sd.veh
                            .state
                            .dist
                            .get_fresh(|| format_dbg!())?
                            .get::<si::mile>()
                        / phev_calc.adj_iter_kwh_per_mi[c];
            } else {
                phev_calc.adj_iter_cd_miles[c] = 0.0;
            }
        }

        let mut soc_hist: Vec<f64> = vec![];
        for soc in sd
            .veh
            .res()
            .with_context(|| format_dbg!())?
            .history
            .soc
            .clone()
        {
            soc_hist.push(soc.get_fresh(|| format_dbg!())?.get::<si::ratio>());
        }

        phev_calc.adj_cd_miles =
            if max_soc - label_fe_phev.regen_soc_buffer - (*soc_hist.min()? * uc::R) < 0.01 * uc::R
            {
                1000.0
            } else {
                *phev_calc.adj_iter_cd_miles.max()?
            };

        // utility factor calculation for last charge depletion iteration and transition iteration
        // ported from excel

        phev_calc.adj_iter_uf = vec![];
        for x in phev_calc.adj_iter_cd_miles.clone() {
            phev_calc.adj_iter_uf.push(
                phev_utilization_params.uf_array[first_grtr(
                    &phev_utilization_params.rechg_freq_miles,
                    x,
                )
                .with_context(|| format_dbg!())?
                    - 1],
            )
        }

        let adj_iter_uf_diff = phev_calc.adj_iter_uf.diff();
        phev_calc.adj_iter_uf_gpm = vec![0.0; phev_calc.cd_cycs.floor() as usize];
        phev_calc.adj_iter_uf_gpm.push(
            (1.0 / phev_calc.adj_iter_mpgge[phev_calc.adj_iter_mpgge.len() - 2])
                * adj_iter_uf_diff[adj_iter_uf_diff.len() - 2],
        );
        phev_calc.adj_iter_uf_gpm.push(
            (1.0 / phev_calc
                .adj_iter_mpgge
                .last()
                .with_context(|| format_dbg!())?)
                * (1.0 - phev_calc.adj_iter_uf[phev_calc.adj_iter_uf.len() - 2]),
        );

        let adj_uf_diff = phev_calc.adj_iter_uf.diff();
        phev_calc.adj_iter_uf_kwh_per_mi = phev_calc
            .adj_iter_kwh_per_mi
            .iter()
            .zip(adj_uf_diff.iter())
            .map(|(kwh, uf)| kwh * uf)
            .collect();

        let max_uf: f64 = phev_calc
            .adj_iter_uf
            .iter()
            .fold(0.0f64, |acc, x| acc.max(*x));
        phev_calc.adj_cd_mpgge =
            1.0 / phev_calc.adj_iter_uf_gpm[phev_calc.adj_iter_uf_gpm.len() - 2] * max_uf;
        phev_calc.adj_cs_mpgge = 1.0
            / phev_calc
                .adj_iter_uf_gpm
                .last()
                .with_context(|| format_dbg!())?
            * (1.0 - max_uf);

        phev_calc.adj_uf = phev_utilization_params.uf_array[first_grtr(
            &phev_utilization_params.rechg_freq_miles,
            phev_calc.adj_cd_miles,
        )
        .with_context(|| format_dbg!())?
            - 1];

        phev_calc.adj_mpgge = 1.0
            / (phev_calc.adj_uf / phev_calc.adj_cd_mpgge
                + (1.0 - phev_calc.adj_uf) / phev_calc.adj_cs_mpgge);

        let uf_kwh_sum: f64 = phev_calc
            .adj_iter_uf_kwh_per_mi
            .iter()
            .fold(0.0, |acc, x| acc + x);
        phev_calc.adj_kwh_per_mi = uf_kwh_sum / max_uf / chg_eff;

        phev_calc.adj_ess_kwh_per_mi = uf_kwh_sum / max_uf;

        match *key {
            "udds" => label_fe_phev.udds = phev_calc.clone(),
            "hwy" => label_fe_phev.hwy = phev_calc.clone(),
            &_ => bail!("No field for cycle {}", key),
        };
    }

    Ok(label_fe_phev)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[cfg(feature = "compat")]
    use crate::compat::fastsim_2::fastsim_core::traits::SerdeAPI;

    pub struct Tolerances {
        pub udds_tolerance: f64,
        pub comb_tolerance: f64,
        pub hwy_tolerance: f64,
        pub accel_tolerance: f64,
    }

    #[cfg(feature = "compat")]
    fn assert_labels_match_within_tolerance(
        label_fe_f3: &LabelFe,
        label_fe_f2: &crate::compat::fastsim_2::fastsim_core::simdrivelabel::LabelFe,
        tol: &Tolerances,
        all_electric: bool,
    ) {
        let mut all_passed: bool = true;
        let mut message: String = String::new();
        // Check MPGe values for HEV
        if all_electric {
            let udds_err = (label_fe_f3.lab_udds_kwh_per_mi - label_fe_f2.lab_udds_kwh_per_mi)
                .abs()
                / label_fe_f2.lab_udds_kwh_per_mi;
            let test = udds_err < tol.udds_tolerance;
            all_passed = all_passed && test;
            message = format!(
                "{}\n{}UDDS kWh/mi mismatch: F3={:.3}, F2={:.3}; err = {:.3} (> tol {:.3})",
                message,
                if test { "  " } else { "* " },
                label_fe_f3.lab_udds_kwh_per_mi,
                label_fe_f2.lab_udds_kwh_per_mi,
                udds_err,
                tol.udds_tolerance
            );
            let comb_err = (label_fe_f3.lab_comb_kwh_per_mi - label_fe_f2.lab_comb_kwh_per_mi)
                .abs()
                / label_fe_f2.lab_comb_kwh_per_mi;
            let test = comb_err < tol.comb_tolerance;
            all_passed = all_passed && test;
            message = format!(
                "{}\n{}Combined kWh/mi mismatch: F3={:.3}, F2={:.3}; err = {:.3} (> tol {:.3})",
                message,
                if test { "  " } else { "* " },
                label_fe_f3.lab_comb_kwh_per_mi,
                label_fe_f2.lab_comb_kwh_per_mi,
                comb_err,
                tol.comb_tolerance
            );
            let hwy_err = (label_fe_f3.lab_hwy_kwh_per_mi - label_fe_f2.lab_hwy_kwh_per_mi).abs()
                / label_fe_f2.lab_hwy_kwh_per_mi;
            let test = hwy_err < tol.hwy_tolerance;
            all_passed = all_passed && test;
            message = format!(
                "{}\n{}HWY kWh/mi mismatch: F3={:.3}, F2={:.3}; err = {:.3} (> tol {:.3})",
                message,
                if test { "  " } else { "* " },
                label_fe_f3.lab_hwy_kwh_per_mi,
                label_fe_f2.lab_hwy_kwh_per_mi,
                hwy_err,
                tol.hwy_tolerance
            );
        } else {
            let udds_err = (label_fe_f3.lab_udds_mpgge - label_fe_f2.lab_udds_mpgge).abs()
                / label_fe_f2.lab_udds_mpgge;
            let test = udds_err < tol.udds_tolerance;
            all_passed = all_passed && test;
            message = format!(
                "{}\n{}UDDS MPGe mismatch: F3={:.3}, F2={:.3}; err = {:.3} (> tol {:.3})",
                message,
                if test { "  " } else { "* " },
                label_fe_f3.lab_udds_mpgge,
                label_fe_f2.lab_udds_mpgge,
                udds_err,
                tol.udds_tolerance
            );
            let comb_err = (label_fe_f3.lab_comb_mpgge - label_fe_f2.lab_comb_mpgge).abs()
                / label_fe_f2.lab_comb_mpgge;
            let test = comb_err < tol.comb_tolerance;
            all_passed = all_passed && test;
            message = format!(
                "{}\n{}Combined MPGe mismatch: F3={:.3}, F2={:.3}; err = {:.3} (> tol {:.3})",
                message,
                if test { "  " } else { "* " },
                label_fe_f3.lab_comb_mpgge,
                label_fe_f2.lab_comb_mpgge,
                comb_err,
                tol.comb_tolerance
            );
            let hwy_err = (label_fe_f3.lab_hwy_mpgge - label_fe_f2.lab_hwy_mpgge).abs()
                / label_fe_f2.lab_hwy_mpgge;
            let test = hwy_err < tol.hwy_tolerance;
            all_passed = all_passed && test;
            message = format!(
                "{}\n{}Hwy MPGe mismatch: F3={:.3}, F2={:.3}; err = {:.3} (> tol {:.3})",
                message,
                if test { "  " } else { "* " },
                label_fe_f3.lab_hwy_mpgge,
                label_fe_f2.lab_hwy_mpgge,
                hwy_err,
                tol.hwy_tolerance
            );
        }
        let accel_err =
            (label_fe_f3.net_accel - label_fe_f2.net_accel).abs() / label_fe_f2.net_accel;
        let test = accel_err < tol.accel_tolerance;
        all_passed = all_passed && test;
        message = format!(
            "{}\n{}Acceleration time mismatch: F3={:.3}, F2={:.3}; err = {:.3} (> tol {:.3})",
            message,
            if test { "  " } else { "* " },
            label_fe_f3.net_accel,
            label_fe_f2.net_accel,
            accel_err,
            tol.accel_tolerance
        );
        assert!(all_passed, "Individual Test Results:\n{}", message);
    }

    /// Test that label FE calculations for conventional vehicles match FASTSim-2 results
    #[test]
    #[cfg(all(feature = "compat", feature = "resources", feature = "yaml"))]
    fn test_label_fe_conv_vs_fastsim2() {
        let file_contents = crate::compat::fastsim_2::ASSETS_DIR
            .get_file("vehicles/2012_Ford_Fusion.yaml")
            .unwrap()
            .contents();
        let f2veh = crate::compat::fastsim_2::fastsim_core::vehicle::RustVehicle::from_reader(
            file_contents,
            "yaml",
            false,
        )
        .unwrap();
        let mut veh = Vehicle::try_from(f2veh.clone()).unwrap();

        // Get FASTSim-3 label FE results
        let (label_fe_f3, _) = get_label_fe(&mut veh, None, false, None, None, None, false)
            .with_context(|| format_dbg!())
            .unwrap();

        // Get FASTSim-2 label FE results
        let (label_fe_f2, _) = crate::compat::fastsim_2::fastsim_core::simdrivelabel::get_label_fe(
            &f2veh.clone(),
            None,
            None,
        )
        .with_context(|| format_dbg!())
        .unwrap();

        let tol = Tolerances {
            udds_tolerance: 0.03, // 3% tolerance
            comb_tolerance: 0.03,
            hwy_tolerance: 0.03,
            accel_tolerance: 0.06, // bumped from 0.05: F3 accel is now faster due to speed flooring fix
        };

        assert_labels_match_within_tolerance(&label_fe_f3, &label_fe_f2, &tol, false);

        println!("Conventional vehicle label FE test passed!");
        println!(
            "F3 Combined MPGe: {:.3}, F2: {:.3}",
            label_fe_f3.lab_comb_mpgge, label_fe_f2.lab_comb_mpgge
        );
    }

    /// Test that label FE calculations for BEV vehicles match FASTSim-2 results
    #[test]
    #[cfg(all(feature = "compat", feature = "resources", feature = "yaml"))]
    fn test_label_fe_bev_vs_fastsim2() {
        let file_contents = crate::compat::fastsim_2::ASSETS_DIR
            .get_file("vehicles/2022_Renault_Zoe_ZE50_R135.yaml")
            .unwrap()
            .contents();
        let f2veh = crate::compat::fastsim_2::fastsim_core::vehicle::RustVehicle::from_reader(
            file_contents,
            "yaml",
            false,
        )
        .unwrap();
        let mut veh = Vehicle::try_from(f2veh.clone()).unwrap();

        // Get FASTSim-3 label FE results
        let (label_fe_f3, _) = get_label_fe(&mut veh, None, false, None, None, None, false)
            .with_context(|| format_dbg!())
            .unwrap();

        let (label_fe_f2, _) = crate::compat::fastsim_2::fastsim_core::simdrivelabel::get_label_fe(
            &f2veh.clone(),
            None,
            None,
        )
        .with_context(|| format_dbg!())
        .unwrap();

        let tol = Tolerances {
            udds_tolerance: 0.011, // 1.1% tolerance
            comb_tolerance: 0.011,
            hwy_tolerance: 0.011,
            accel_tolerance: 0.15,
        };

        assert_labels_match_within_tolerance(&label_fe_f3, &label_fe_f2, &tol, true);

        println!("BEV label FE test passed!");
        println!(
            "F3 Combined kWh/mi: {:.3}, F2: {:.3}",
            label_fe_f3.lab_comb_kwh_per_mi, label_fe_f2.lab_comb_kwh_per_mi
        );
    }

    /// Test that label FE calculations for HEV vehicles match FASTSim-2 results
    #[test]
    #[cfg(all(feature = "compat", feature = "resources", feature = "yaml"))]
    fn test_label_fe_hev_vs_fastsim2() {
        let file_contents = crate::compat::fastsim_2::ASSETS_DIR
            .get_file("vehicles/2016_Toyota_Prius_Two.yaml")
            .unwrap()
            .contents();
        let f2veh = crate::compat::fastsim_2::fastsim_core::vehicle::RustVehicle::from_reader(
            file_contents,
            "yaml",
            false,
        )
        .unwrap();
        let mut veh = Vehicle::try_from(f2veh.clone()).unwrap();

        // Get FASTSim-3 label FE results
        let (label_fe_f3, _) = get_label_fe(&mut veh, None, false, None, None, None, false)
            .with_context(|| format_dbg!())
            .unwrap();

        let (label_fe_f2, _) =
            crate::compat::fastsim_2::fastsim_core::simdrivelabel::get_label_fe(&f2veh, None, None)
                .with_context(|| format_dbg!())
                .unwrap();

        // NOTE: EPA data is closer to Fastsim 3 results for UDDS
        // https://www.fueleconomy.gov/feg/PowerSearch.do?action=noform&path=1&year1=2016&year2=2016&make=Toyota&baseModel=Prius&srchtyp=ymm&pageno=1&rowLimit=50
        let tol = Tolerances {
            udds_tolerance: 0.15, // 15% tolerance
            comb_tolerance: 0.15,
            hwy_tolerance: 0.15,
            accel_tolerance: 0.06, // bumped from 0.05: F3 accel is now faster due to speed flooring fix
        };

        assert_labels_match_within_tolerance(&label_fe_f3, &label_fe_f2, &tol, false);
    }

    /// Test that creates a mock PHEV vehicle from FASTSim-2 data and compares label FE calculations
    #[test]
    #[cfg(all(feature = "compat", feature = "resources", feature = "yaml"))]
    fn test_label_fe_phev_vs_fastsim2() {
        let file_contents = crate::compat::fastsim_2::ASSETS_DIR
            .get_file("vehicles/2016_Chevrolet_Volt.yaml")
            .unwrap()
            .contents();
        // Load FASTSim-2 vehicle and convert to FASTSim-3
        let f2_veh = crate::compat::fastsim_2::fastsim_core::vehicle::RustVehicle::from_reader(
            file_contents,
            "yaml",
            false,
        )
        .with_context(|| format_dbg!())
        .unwrap();
        assert!(f2_veh.veh_pt_type == crate::compat::fastsim_2::fastsim_core::vehicle::PHEV);
        let mut veh = Vehicle::try_from(f2_veh.clone())
            .with_context(|| format_dbg!())
            .unwrap();
        assert!(
            veh.pt_type.is_plug_in_hybrid_electric_vehicle(),
            "`veh.pt_type.variant_as_str()`: {}\n`f2_veh.veh_pt_type`: {}",
            veh.pt_type.variant_as_str(),
            f2_veh.veh_pt_type
        );

        // Get FASTSim-3 label FE results (if PHEV functionality is implemented)
        let label_fe_f3 = get_label_fe(&mut veh, None, false, None, None, None, false)
            .unwrap()
            .0;

        // Get FASTSim-2 label FE results
        let label_fe_f2 = crate::compat::fastsim_2::fastsim_core::simdrivelabel::get_label_fe(
            &f2_veh, None, None,
        )
        .unwrap()
        .0;

        let tol = Tolerances {
            udds_tolerance: 0.05, // 5% tolerance
            comb_tolerance: 0.05,
            hwy_tolerance: 0.05,
            accel_tolerance: 0.12, // bumped from 0.105: F3 accel is now faster due to speed flooring fix
        };

        assert_labels_match_within_tolerance(&label_fe_f3, &label_fe_f2, &tol, false);
    }

    fn frac_diff(base: f64, new_value: f64) -> f64 {
        let abs_diff = (new_value - base).abs();
        if base != 0.0 {
            abs_diff / base
        } else {
            abs_diff
        }
    }

    #[cfg(feature = "compat")]
    fn assert_label_fe_same(
        label_fe_f2: &crate::compat::fastsim_2::fastsim_core::simdrivelabel::LabelFe,
        label_fe_f3: &LabelFe,
        tol: f64,
    ) {
        let mut all_pass = true;
        let mut message = String::new();
        let diff = frac_diff(label_fe_f2.lab_comb_mpgge, label_fe_f3.lab_comb_mpgge);
        all_pass = all_pass && diff < tol;
        message = format!(
            "{}\nlab_comb_mpgge: F3: {:.3}; F2: {:.3} ({:.3})",
            message, label_fe_f3.lab_comb_mpgge, label_fe_f2.lab_comb_mpgge, diff
        );
        let diff = frac_diff(
            label_fe_f2.lab_comb_kwh_per_mi,
            label_fe_f3.lab_comb_kwh_per_mi,
        );
        all_pass = all_pass && diff < tol;
        message = format!(
            "{}\nlab_comb_kwh_per_mi: F3: {:.3}; F2 {:.3} ({:.3})",
            message, label_fe_f3.lab_comb_kwh_per_mi, label_fe_f2.lab_comb_kwh_per_mi, diff
        );
        let diff = frac_diff(label_fe_f2.adj_udds_mpgge, label_fe_f3.adj_udds_mpgge);
        all_pass = all_pass && diff < tol;
        message = format!(
            "{}\nadj_udds_mpgge: F3: {:.3}; F2: {:.3} ({:.3})",
            message, label_fe_f3.adj_udds_mpgge, label_fe_f2.adj_udds_mpgge, diff
        );
        let diff = frac_diff(label_fe_f2.adj_hwy_mpgge, label_fe_f3.adj_hwy_mpgge);
        all_pass = all_pass && diff < tol;
        message = format!(
            "{}\nadj_hwy_mpgge: F3: {:.3}; F2: {:.3} ({:.3})",
            message, label_fe_f3.adj_hwy_mpgge, label_fe_f2.adj_hwy_mpgge, diff
        );
        let diff = frac_diff(label_fe_f2.adj_comb_mpgge, label_fe_f3.adj_comb_mpgge);
        all_pass = all_pass && diff < tol;
        message = format!(
            "{}\nadj_comb_mpgge: F3: {:.3}; F2: {:.3} ({:.3})",
            message, label_fe_f3.adj_comb_mpgge, label_fe_f2.adj_comb_mpgge, diff
        );
        let diff = frac_diff(
            label_fe_f2.adj_udds_kwh_per_mi,
            label_fe_f3.adj_udds_kwh_per_mi,
        );
        all_pass = all_pass && diff < tol;
        message = format!(
            "{}\nadj_udds_kwh_per_mi: F3 {:.3}; F2 {:.3} ({:.3})",
            message, label_fe_f3.adj_udds_kwh_per_mi, label_fe_f2.adj_udds_kwh_per_mi, diff
        );
        let diff = frac_diff(
            label_fe_f2.adj_hwy_kwh_per_mi,
            label_fe_f3.adj_hwy_kwh_per_mi,
        );
        all_pass = all_pass && diff < tol;
        message = format!(
            "{}\nadj_hwy_kwh_per_mi: F3 {:.3}; F2 {:.3} ({:.3})",
            message, label_fe_f3.adj_hwy_kwh_per_mi, label_fe_f2.adj_hwy_kwh_per_mi, diff
        );
        let diff = frac_diff(
            label_fe_f2.adj_comb_kwh_per_mi,
            label_fe_f3.adj_comb_kwh_per_mi,
        );
        all_pass = all_pass && diff < tol;
        message = format!(
            "{}\nadj_comb_kwh_per_mi: F3: {:.3}; F2: {:.3} ({:.3})",
            message, label_fe_f3.adj_comb_kwh_per_mi, label_fe_f2.adj_comb_kwh_per_mi, diff
        );
        let diff = frac_diff(label_fe_f2.net_accel, label_fe_f3.net_accel);
        all_pass = all_pass && diff < tol;
        message = format!(
            "{}\nnet_accel: F3: {:.3}; F2: {:.3} ({:.3})",
            message, label_fe_f3.net_accel, label_fe_f2.net_accel, diff
        );
        assert!(
            all_pass,
            "ERROR: At least some tests exceed tolerance of {:.3}:\n{}",
            tol, message
        );
    }

    #[cfg(feature = "compat")]
    fn run_fe_label_comparison_for(
        f2veh: &crate::compat::fastsim_2::fastsim_core::vehicle::RustVehicle,
        tolerance: f64,
    ) {
        // Get FASTSim-2 label FE results
        let (label_fe_f2, result) =
            crate::compat::fastsim_2::fastsim_core::simdrivelabel::get_label_fe(
                &f2veh,
                Some(true),
                None,
            )
            .with_context(|| format_dbg!())
            .unwrap();

        let sim_data = SimulationDataForLabel::ConvOrHev {
            veh_year: f2veh.veh_year,
            udds_mpgge: label_fe_f2.lab_udds_mpgge,
            hwy_mpgge: label_fe_f2.lab_hwy_mpgge,
            fs_energy_capacity_kwh: f2veh.fs_kwh,
        };
        let max_epa_adj = 0.3;
        assert!(result.is_some());
        let results_data = result.unwrap();
        assert!(results_data.contains_key("accel"));
        let accel_sd = &results_data["accel"];
        let accel_data = AccelData {
            time_s: accel_sd.cyc.time_s.to_vec(),
            speed_mph: accel_sd.mph_ach.to_vec(),
        };
        let label_fe_f3 = calculate_label_fuel_economy(
            &FuelProperties::default(),
            &PhevUtilizationParams::default(),
            max_epa_adj,
            &sim_data,
            &accel_data,
        )
        .expect("should return an OK result");
        assert_label_fe_same(&label_fe_f2, &label_fe_f3, tolerance);
    }
    #[test]
    #[cfg(all(feature = "compat", feature = "resources", feature = "yaml"))]
    pub fn test_label_fe_post_proc_calcs_for_conv() {
        let file_contents = crate::compat::fastsim_2::ASSETS_DIR
            .get_file("vehicles/2012_Ford_Fusion.yaml")
            .unwrap()
            .contents();
        let f2veh = crate::compat::fastsim_2::fastsim_core::vehicle::RustVehicle::from_reader(
            file_contents,
            "yaml",
            false,
        )
        .unwrap();
        let tolerance = 1e-6;
        run_fe_label_comparison_for(&f2veh, tolerance);
    }
    #[test]
    #[cfg(all(feature = "compat", feature = "resources", feature = "yaml"))]
    pub fn test_label_fe_post_proc_calcs_for_hev() {
        let file_contents = crate::compat::fastsim_2::ASSETS_DIR
            .get_file("vehicles/2016_Toyota_Prius_Two.yaml")
            .unwrap()
            .contents();
        let f2veh = crate::compat::fastsim_2::fastsim_core::vehicle::RustVehicle::from_reader(
            file_contents,
            "yaml",
            false,
        )
        .unwrap();
        let tolerance = 1e-6;
        run_fe_label_comparison_for(&f2veh, tolerance);
    }
    #[test]
    #[cfg(all(feature = "compat", feature = "resources", feature = "yaml"))]
    pub fn test_label_fe_post_proc_calcs_for_bev() {
        let file_contents = crate::compat::fastsim_2::ASSETS_DIR
            .get_file("vehicles/2022_Renault_Zoe_ZE50_R135.yaml")
            .unwrap()
            .contents();
        let f2veh = crate::compat::fastsim_2::fastsim_core::vehicle::RustVehicle::from_reader(
            file_contents,
            "yaml",
            false,
        )
        .unwrap();

        // Get FASTSim-2 label FE results
        let (label_fe_f2, result) =
            crate::compat::fastsim_2::fastsim_core::simdrivelabel::get_label_fe(
                &f2veh,
                Some(true),
                None,
            )
            .with_context(|| format_dbg!())
            .unwrap();
        let sim_data = SimulationDataForLabel::Bev {
            veh_year: f2veh.veh_year,
            udds_kwh_per_mi: label_fe_f2.lab_udds_kwh_per_mi,
            hwy_kwh_per_mi: label_fe_f2.lab_hwy_kwh_per_mi,
            bev_energy_capacity_kwh: f2veh.ess_max_kwh,
        };
        let max_epa_adj = 0.3;
        assert!(result.is_some());
        let results_data = result.unwrap();
        assert!(results_data.contains_key("accel"));
        let accel_sd = &results_data["accel"];
        let accel_data = AccelData {
            time_s: accel_sd.cyc.time_s.to_vec(),
            speed_mph: accel_sd.mph_ach.to_vec(),
        };
        let label_fe_f3 = calculate_label_fuel_economy(
            &FuelProperties::default(),
            &PhevUtilizationParams::default(),
            max_epa_adj,
            &sim_data,
            &accel_data,
        )
        .expect("should have OK result");
        let tolerance = 1e-6;
        assert_label_fe_same(&label_fe_f2, &label_fe_f3, tolerance);
    }
    #[test]
    #[cfg(all(feature = "compat", feature = "resources", feature = "yaml"))]
    pub fn test_label_fe_post_proc_calcs_for_phev() {
        let file_contents = crate::compat::fastsim_2::ASSETS_DIR
            .get_file("vehicles/2016_Chevrolet_Volt.yaml")
            .unwrap()
            .contents();

        // Load FASTSim-2 vehicle and convert to FASTSim-3
        let f2_veh = crate::compat::fastsim_2::fastsim_core::vehicle::RustVehicle::from_reader(
            file_contents,
            "yaml",
            false,
        )
        .with_context(|| format_dbg!())
        .unwrap();
        assert!(f2_veh.veh_pt_type == crate::compat::fastsim_2::fastsim_core::vehicle::PHEV);

        let veh = Vehicle::try_from(f2_veh.clone())
            .with_context(|| format_dbg!())
            .unwrap();
        assert!(
            veh.pt_type.is_plug_in_hybrid_electric_vehicle(),
            "`veh.pt_type.variant_as_str()`: {}\n`f2_veh.veh_pt_type`: {}",
            veh.pt_type.variant_as_str(),
            f2_veh.veh_pt_type
        );

        // Get FASTSim-2 label FE results
        let (label_fe_f2, result) =
            crate::compat::fastsim_2::fastsim_core::simdrivelabel::get_label_fe(
                &f2_veh,
                Some(true),
                None,
            )
            .with_context(|| format_dbg!())
            .unwrap();
        assert!(label_fe_f2.phev_calcs.is_some());
        let phev_calcs = label_fe_f2.phev_calcs.clone().unwrap();
        eprintln!("phev_calcs: {:?}", phev_calcs);
        assert!(result.is_some());
        let result = result.unwrap();
        eprintln!("result.keys: {:?}", result.keys());
        assert!(result.contains_key("udds"));
        let udds_result = &result["udds"];
        assert!(result.contains_key("hwy"));
        let hwy_result = &result["hwy"];
        eprintln!(
            "udds start soc: {:?}; min soc: {:?}",
            udds_result.soc[0],
            udds_result
                .soc
                .iter()
                .min_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal))
        );
        eprintln!(
            "hwy start soc: {:?}; min soc: {:?}",
            hwy_result.soc[0],
            hwy_result
                .soc
                .iter()
                .min_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal))
        );
        let fuel_props = FuelProperties::default();
        let sim_data = SimulationDataForLabel::Phev {
            veh_year: f2_veh.veh_year,
            info: PhevVehicleInfo {
                max_soc: f2_veh.max_soc * uc::R,
                min_soc: f2_veh.min_soc * uc::R,
                // NOTE: for F3, max regen is hard-coded to be 0.98; that just
                // happens to be what this vehicle model also uses.
                phev_max_regen: f2_veh.max_regen * uc::R,
                veh_mass: f2_veh.veh_kg * uc::KG,
                em_peak_eff: f2_veh.mc_peak_eff() * uc::R,
                energy_capacity: f2_veh.ess_max_kwh * uc::KWH,
                chg_eff: DEFAULT_CHG_EFF,
                fuel_storage_capacity: f2_veh.fs_kwh * uc::KWH,
            },
            udds: PhevSimulationDataForLabel {
                cd_fuel_consumed_kwh: phev_calcs.udds.cd_fs_kwh,
                cd_soc_start: f2_veh.max_soc,
                cd_soc_end: f2_veh.max_soc - phev_calcs.udds.delta_soc,
                cyc_dist_mi: udds_result.dist_mi.sum(),
                cd_kwh_per_mi: phev_calcs.udds.cd_ess_kwh_per_mi,
                // NOTE: calculating cd_mpg as F2's phev_calcs.udds.cd_mpg appears to have a mistake
                cd_mpg: udds_result.dist_mi.sum()
                    / (phev_calcs.udds.cd_fs_kwh / fuel_props.kwh_per_gge()),
                cs_fuel_consumed_kwh: phev_calcs.udds.cs_fs_kwh,
                cs_ess_energy_kwh: phev_calcs.udds.cs_ess_kwh,
                cs_kwh_per_mi: phev_calcs.udds.cs_ess_kwh_per_mi,
                cs_mpg: phev_calcs.udds.cs_mpg,
                cs_min_soc: f2_veh.min_soc,
                cs_fs_energy_capacity_kwh: f2_veh.fs_kwh,
            },
            hwy: PhevSimulationDataForLabel {
                cd_fuel_consumed_kwh: phev_calcs.hwy.cd_fs_kwh,
                cd_soc_start: f2_veh.max_soc,
                cd_soc_end: f2_veh.max_soc - phev_calcs.hwy.delta_soc,
                cyc_dist_mi: hwy_result.dist_mi.sum(),
                cd_kwh_per_mi: phev_calcs.hwy.cd_ess_kwh_per_mi,
                // NOTE: calculating cd_mpg as F2's phev_calcs.hwy.cd_mpg appears to have a mistake
                cd_mpg: hwy_result.dist_mi.sum()
                    / (phev_calcs.hwy.cd_fs_kwh / fuel_props.kwh_per_gge()),
                cs_fuel_consumed_kwh: phev_calcs.hwy.cs_fs_kwh,
                cs_ess_energy_kwh: phev_calcs.hwy.cs_ess_kwh,
                cs_kwh_per_mi: phev_calcs.hwy.cs_ess_kwh_per_mi,
                cs_mpg: phev_calcs.hwy.cs_mpg,
                cs_min_soc: f2_veh.min_soc,
                cs_fs_energy_capacity_kwh: f2_veh.fs_kwh,
            },
        };
        let max_epa_adj = 0.3;
        assert!(result.contains_key("accel"));
        let accel_sd = &result["accel"];
        let accel_data = AccelData {
            time_s: accel_sd.cyc.time_s.to_vec(),
            speed_mph: accel_sd.mph_ach.to_vec(),
        };
        let label_fe_f3 = calculate_label_fuel_economy(
            &FuelProperties::default(),
            &PhevUtilizationParams::default(),
            max_epa_adj,
            &sim_data,
            &accel_data,
        )
        .expect("expect OK result");
        let tolerance = 0.002;
        assert_label_fe_same(&label_fe_f2, &label_fe_f3, tolerance);
    }
}
