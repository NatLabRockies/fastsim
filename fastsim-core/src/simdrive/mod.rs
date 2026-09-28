pub mod params;
pub mod roadload;

pub use params::{SimParams, TraceMissOptions, TraceMissTolerance};
use roadload::StepInfo;

use super::drive_cycle::Cycle;
use super::vehicle::Vehicle;
use crate::drive_cycle::manipulation_utils::calc_best_rendezvous;
use crate::imports::*;
use crate::prelude::*;
use crate::vehicle::conv::DfcoControls;

#[serde_api]
#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, StateMethods)]
#[non_exhaustive]
#[serde(deny_unknown_fields)]
#[cfg_attr(feature = "pyo3", pyclass(module = "fastsim", subclass, eq))]
pub struct SimDrive {
    #[has_state]
    pub veh: Vehicle,
    pub cyc: Cycle,
    pub sim_params: SimParams,
}

#[pyo3_api]
impl SimDrive {
    #[new]
    #[pyo3(signature = (veh, cyc, sim_params=None))]
    fn __new__(veh: Vehicle, cyc: Cycle, sim_params: Option<SimParams>) -> anyhow::Result<Self> {
        Ok(SimDrive::new(veh, cyc, sim_params))
    }

    /// Run vehicle simulation once
    #[pyo3(name = "run_once")]
    fn run_once_py(&mut self) -> anyhow::Result<()> {
        self.run_once()
    }

    /// Run vehicle simulation, and, if applicable, apply powertrain-specific
    /// corrections (e.g. iterate `run` until SOC balance is achieved -- i.e. initial
    /// and final SOC are nearly identical)
    #[pyo3(name = "run")]
    fn run_py(&mut self) -> anyhow::Result<()> {
        self.run()
    }

    /// Deprecated alias for [Self::run_once]
    #[pyo3(name = "walk_once")]
    fn walk_once_py(&mut self, py: Python<'_>) -> anyhow::Result<()> {
        Self::warn_deprecated_walk(py, "walk_once", "run_once");
        self.run_once()
    }

    /// Deprecated alias for [Self::run]
    #[pyo3(name = "walk")]
    fn walk_py(&mut self, py: Python<'_>) -> anyhow::Result<()> {
        Self::warn_deprecated_walk(py, "walk", "run");
        self.run()
    }

    #[pyo3(name = "reset")]
    /// Combines [Self::reset_cumulative], [Self::reset_step], [Self::clear]
    fn reset_py(&mut self) -> anyhow::Result<()> {
        self.reset_cumulative(|| format_dbg!())?;
        self.reset_step(|| format_dbg!())?;
        self.clear();
        Ok(())
    }

    #[pyo3(name = "clear")]
    fn clear_py(&mut self) {
        self.clear()
    }

    #[pyo3(name = "reset_step")]
    fn reset_step_py(&mut self) -> anyhow::Result<()> {
        self.reset_step(|| format_dbg!())
    }

    #[pyo3(name = "reset_cumulative")]
    fn reset_cumulative_py(&mut self) -> anyhow::Result<()> {
        self.reset_cumulative(|| format_dbg!())
    }
}

impl SerdeAPI for SimDrive {}
impl Init for SimDrive {
    fn init(&mut self) -> Result<(), Error> {
        self.veh
            .init()
            .map_err(|err| Error::InitError(format_dbg!(err)))?;
        self.cyc
            .init()
            .map_err(|err| Error::InitError(format_dbg!(err)))?;
        self.sim_params
            .init()
            .map_err(|err| Error::InitError(format_dbg!(err)))?;
        Ok(())
    }
}

impl SimDrive {
    pub fn new(veh: Vehicle, cyc: Cycle, sim_params: Option<SimParams>) -> Self {
        Self {
            veh,
            cyc,
            sim_params: sim_params.unwrap_or_default(),
        }
    }

    /// Emit a Python `DeprecationWarning` pointing users from `old_name` to `new_name`
    #[cfg(feature = "pyo3")]
    fn warn_deprecated_walk(py: Python<'_>, old_name: &str, new_name: &str) {
        if let Ok(warnings) = py.import("warnings") {
            let msg =
                format!("SimDrive.{old_name} is deprecated; use SimDrive.{new_name} instead.",);
            let kwargs = PyDict::new(py);
            let _ = kwargs.set_item("stacklevel", 2);
            let _ = kwargs.set_item("category", py.get_type::<PyDeprecationWarning>());
            let _ = warnings.call_method("warn", (msg,), Some(&kwargs));
        }
    }

    // # TODO:
    // ## Features
    // - [ ] regen limiting curve during speeds approaching zero per f2 -- less urgent
    // - [ ] ability to manipulate friction/regen brake split based on required braking
    //       power -- new feature -- move this to enum
    // - [x] make enum `EngineOnCause::{AlreadyOn, TooCold,
    //       PowerDemand}` and save it in a vec or some such for when there are
    //       multiple causes -- new feature

    /// Run vehicle simulation, and, if applicable, apply powertrain-specific
    /// corrections:
    /// - for HEV, set initial SOC to mean of min and max SOC, and then iterate
    ///   `run_once` until SOC balance is achieved -- i.e. initial and final SOC are
    ///   nearly identical
    /// - for PHEV, set initial SOC to max SOC, and then simulate once
    /// - for BEV, set initial SOC to max SOC, and then simulate once
    /// - for Conv, simulate once
    ///
    /// # Important Considerations
    /// If you need to run a [ReversibleEnergyStorage]-equipped vehicle for
    /// only one iteration without modifying the initial SOC, then run the
    /// [Self::run_once] method directly
    pub fn run(&mut self) -> anyhow::Result<()> {
        match self.veh.pt_type {
            PowertrainType::HybridElectricVehicle(_) => {
                // Net battery energy used per amount of fuel used
                // clone initial vehicle to preserve starting state (TODO: figure out if this is a huge CPU burden)
                let veh_init = self.veh.clone();
                let res_mut = self.veh.res_mut().with_context(|| format_dbg!())?;
                res_mut.state.soc.mark_stale();
                res_mut
                    .state
                    .soc
                    .update(0.5 * (res_mut.min_soc + res_mut.max_soc), || format_dbg!())?;
                loop {
                    self.veh
                        .hev_mut()
                        .with_context(|| format_dbg!())?
                        .soc_bal_iters
                        .mark_stale();
                    self.veh
                        .hev_mut()
                        .with_context(|| format_dbg!())?
                        .soc_bal_iters
                        .increment(1, || format_dbg!())?;
                    self.run_once().map_err(|err| {
                        anyhow::anyhow!(format!(
                            "HEV run_once failed at line {}\ntime step: {}\n with originating error: [{}]",
                            format_dbg!(),
                            self.veh.state.i,
                            err
                        ))
                    })?;
                    // final SOC, to use as starting SOC for next iteration (if needed)
                    let soc_final = self.veh.res().with_context(|| format_dbg!())?.state.soc;
                    // current iteration number, to use as counter for next iteration (if needed)
                    let soc_bal_iters =
                        self.veh.hev().with_context(|| format_dbg!())?.soc_bal_iters;
                    let res_per_fuel = *self
                        .veh
                        .res()
                        .with_context(|| format_dbg!())?
                        .state
                        .energy_out_chemical
                        .get_fresh(|| format_dbg!())?
                        / *self
                            .veh
                            .fc()
                            .with_context(|| format_dbg!())?
                            .state
                            .energy_fuel
                            .get_fresh(|| format_dbg!())?;
                    if self
                        .veh
                        .hev()
                        .with_context(|| format_dbg!())?
                        .soc_bal_iters
                        .get_fresh(|| format_dbg!())?
                        > &self
                            .veh
                            .hev()
                            .with_context(|| format_dbg!())?
                            .sim_params
                            .soc_balance_iter_err
                    {
                        bail!(
                            "{}\nSOC balancing surpassed sim_params.soc_balance_iter_err = {} iterations",
                            format_dbg!(),
                            self.veh.hev().with_context(|| format_dbg!())?.sim_params.soc_balance_iter_err,
                        );
                    }
                    if res_per_fuel.abs()
                        < self
                            .veh
                            .hev()
                            .with_context(|| format_dbg!())?
                            .sim_params
                            .res_per_fuel_lim
                        || !self
                            .veh
                            .hev()
                            .with_context(|| format_dbg!())?
                            .sim_params
                            .balance_soc
                        || self.sim_params.ambient_thermal_soak
                    {
                        break;
                    } else {
                        // prep for another iteration
                        if let Some(&mut ref mut hev) = self.veh.hev_mut() {
                            if hev.sim_params.save_soc_bal_iters {
                                hev.soc_bal_iter_history.push(hev.clone());
                                hev.soc_bal_iters.mark_stale();
                            }
                        }
                        // reset vehicle to initial state
                        self.veh = veh_init.clone();
                        // start SOC at previous final value
                        self.veh.res_mut().with_context(|| format_dbg!())?.state.soc = soc_final;
                        // keep soc_bal_iters value from previous iteration
                        self.veh
                            .hev_mut()
                            .with_context(|| format_dbg!())?
                            .soc_bal_iters = soc_bal_iters;
                    }
                }
            }
            PowertrainType::PlugInHybridElectricVehicle(_) => {
                let res_mut = self.veh.res_mut().with_context(|| format_dbg!())?;
                res_mut.state.soc.mark_stale();
                res_mut
                    .state
                    .soc
                    .update(res_mut.max_soc, || format_dbg!())?;
                self.run_once()?
            }
            PowertrainType::BatteryElectricVehicle(_) => {
                let res_mut = self.veh.res_mut().with_context(|| format_dbg!())?;
                res_mut.state.soc.mark_stale();
                res_mut
                    .state
                    .soc
                    .update(res_mut.max_soc, || format_dbg!())?;
                self.run_once()?
            }
            PowertrainType::ConventionalVehicle(_) => self.run_once()?,
        }
        Ok(())
    }

    #[deprecated(since = "3.1.0", note = "Use SimDrive::run instead")]
    pub fn walk(&mut self) -> anyhow::Result<()> {
        self.run()
    }

    /// Run vehicle simulation once
    pub fn run_once(&mut self) -> anyhow::Result<()> {
        let len = &self.cyc.len_checked().with_context(|| format_dbg!())?;
        ensure!(len >= &2, format_dbg!(len < &2));
        self.save_state(|| format_dbg!())?;

        self.veh.state.mass.mark_stale();
        self.veh.state.mass.update(
            self.veh
                .mass()
                .with_context(|| format_dbg!())?
                .with_context(|| format_dbg!("Expected mass to have been set."))?,
            || format_dbg!(),
        )?;

        let hvac: Option<HVACOption> = if self.sim_params.ambient_thermal_soak {
            ensure!(
                self.cyc.speed.iter().all(|s| *s == si::Velocity::ZERO),
                format!(
                    "{}\nDuring thermal soak, cycle speed should always be zero",
                    format_dbg!()
                )
            );
            if !self.veh.hvac.is_none() {
                // turn off HVAC if vehicle is not active
                let hvac_some = Some(self.veh.hvac.clone());
                self.veh.hvac = HVACOption::None;
                hvac_some
            } else {
                None
            }
        } else {
            None
        };

        loop {
            self.check_and_reset(|| format_dbg!())?;
            self.veh.state.mass.mark_fresh(|| format_dbg!())?;
            if let Some(res) = self.veh.res_mut() {
                res.state.soh.mark_fresh(|| format_dbg!())?;
            }
            self.step(|| format_dbg!())?;
            self.solve_step().map_err(|err| {
                anyhow::anyhow!(format!(
                    "solver step failed at line {}\ntime step: {}\n with originating error: [{}]",
                    format_dbg!(),
                    self.veh.state.i,
                    err
                ))
            })?;
            self.save_state(|| format_dbg!())?;
            if *self.veh.state.i.get_fresh(|| format_dbg!())? == len - 1 {
                break;
            }
        }

        if let Some(hvac) = hvac {
            // reset original hvac
            self.veh.hvac = hvac;
        }

        Ok(())
    }

    #[deprecated(since = "3.1.0", note = "Use SimDrive::run_once instead")]
    pub fn walk_once(&mut self) -> anyhow::Result<()> {
        self.run_once()
    }

    /// Calculates the derivative dv/dd (change in speed by change in distance)
    /// - speed_m_per_s: the speed at which to evaluate dv/dd (m/s)
    /// - grade: the road grade as a decimal fraction
    ///
    /// RETURN: number, the dv/dd for these conditions
    pub fn calc_dvdd(&self, speed_m_per_s: f64, grade: f64) -> anyhow::Result<f64> {
        let v = speed_m_per_s;
        if v <= 0.0 {
            Ok(0.0)
        } else {
            let (atan_grade_sin, atan_grade_cos) = if grade == 0.0 {
                (0.0, 1.0)
            } else {
                let atan_grade = grade.atan();
                (atan_grade.sin(), atan_grade.cos())
            };
            let g = uc::ACC_GRAV.get::<si::meter_per_second_squared>();
            let m = self
                .veh
                .mass
                .with_context(|| {
                    format!(
                        "{}\nVehicle mass should have been set already.",
                        format_dbg!()
                    )
                })?
                .get::<si::kilogram>();
            let rho_cdfa = self
                .veh
                .state
                .air_density
                .get_stale(|| format_dbg!())?
                .get::<si::kilogram_per_cubic_meter>()
                * self.veh.chassis.drag_coef.get::<si::ratio>()
                * self.veh.chassis.frontal_area.get::<si::square_meter>();
            let rrc = self.veh.chassis.wheel_rr_coef.get::<si::ratio>();
            Ok(-((g / v) * (atan_grade_sin + rrc * atan_grade_cos)
                + (0.5 * rho_cdfa * (1.0 / m) * v)))
        }
    }

    /// Solves current time step
    pub fn solve_step(&mut self) -> anyhow::Result<()> {
        let i = *self.veh.state.i.get_fresh(|| format_dbg!())?;
        let time_prev = *self.veh.state.time.get_stale(|| format_dbg!())?;
        ensure!(self.cyc.time.len() > i);
        self.veh
            .state
            .time
            .update(
                *self.cyc.time.get(i).ok_or({
                    anyhow::anyhow!(format!(
                        "failed to get time for index {} at line {}",
                        i,
                        format_dbg!()
                    ))
                })?,
                || format_dbg!(),
            )
            .map_err(|err| {
                anyhow::anyhow!(format!(
                    "updating time failed at line {}\ntime step: {}\n with originating error [{}]",
                    format_dbg!(),
                    self.veh.state.i,
                    err
                ))
            })?;
        let dt = *self.veh.state.time.get_fresh(|| format_dbg!())? - time_prev;
        // maybe make controls like:
        // ```
        // pub enum HVACAuxPriority {
        //     /// Prioritize [ReversibleEnergyStorage] thermal management
        //     ReversibleEnergyStorage
        //     /// Prioritize [Cabin] and [ReversibleEnergyStorage] proportionally to their requests
        //     Proportional
        // }
        // ```

        // `solve_thermal` must happen before the other methods because it impacts aux power demand
        self.veh
            .solve_thermal(self.cyc.temp_amb_air[i], dt)
            .with_context(|| format!("{}\n`self.veh.state.i`: {}", format_dbg!(), i))?;
        match self.sim_params.ambient_thermal_soak {
            false => {
                self.veh
                    .set_curr_pwr_out_max(dt)
                    .with_context(|| anyhow!(format_dbg!()))?;
                self.set_pwr_prop_for_speed(
                    self.cyc.speed[i],
                    *self.veh.state.speed_ach.get_stale(|| format_dbg!())?,
                    dt,
                )
                .with_context(|| anyhow!(format_dbg!()))?;
                self.veh.state.pwr_tractive_for_cyc.update(
                    *self.veh.state.pwr_tractive.get_fresh(|| format_dbg!())?,
                    || format_dbg!(),
                )?;
                self.set_ach_speed(self.cyc.speed[i], self.cyc.dist[i], dt).map_err(|err| anyhow::anyhow!(format!(
                    "set_ach_speed failed at line {} with cycle speed {:?}, cyc dist {:?}, and dt {:?} and originating error {}",
                    format_dbg!(),
                    self.cyc.speed[i],
                    self.cyc.dist[i],
                    dt,
                    err
                )))?;

                // Handle control options requiring current step's speed
                match &mut self.veh.pt_type {
                    PowertrainType::HybridElectricVehicle(hev) => hev
                        .pt_cntrl
                        .handle_fc_on_causes_for_speed(self.cyc.speed[i])?,
                    PowertrainType::PlugInHybridElectricVehicle(hev) => hev
                        .pt_cntrl
                        .handle_fc_on_causes_for_speed(self.cyc.speed[i])?,
                    PowertrainType::ConventionalVehicle(conv) => {
                        conv.dfco_cntrl.state.vehicle_dynamics_prevent_dfco.update(
                            DfcoControls::is_dfco_disabled_due_to_veh_dynamics(
                                self.cyc.speed[i - 1],
                                self.cyc.speed[i],
                                dt,
                                conv.dfco_cntrl.dfco_enabled,
                                conv.dfco_cntrl.minimum_dfco_speed,
                                conv.dfco_cntrl.minimum_dfco_deceleration,
                                conv.dfco_cntrl.stopped_speed_threshold,
                            ),
                            || format_dbg!(),
                        )?;
                        conv.pt_cntrl
                            .handle_fc_on_causes_for_speed(self.cyc.speed[i])?
                    }
                    _ => (),
                }
                self.veh.solve_powertrain(dt).map_err(|err| {
                    anyhow::anyhow!(format!(
                        "solve_powertrain failed at line {} with originating error [{}]",
                        format_dbg!(),
                        err
                    ))
                })?;
            }
            true => {
                self.veh.mark_non_thermal_fresh()?;
            }
        }
        self.set_cumulative(dt, || format_dbg!())?;
        Ok(())
    }

    /// Sets power required for given prescribed speed
    /// # Arguments
    /// - `speed`: prescribed or achieved speed
    /// - `dt`: simulation time step size
    pub fn set_pwr_prop_for_speed(
        &mut self,
        speed: si::Velocity,
        speed_prev: si::Velocity,
        dt: si::Time,
    ) -> anyhow::Result<()> {
        let i = *self.veh.state.i.get_fresh(|| format_dbg!())?;
        let vs = &mut self.veh.state;
        // TODO: get @mokeefe to give this a serious look and think about grade alignment issues that may arise
        // TODO: memo-ize this
        //     - if we get back on trace or nearly back on trace, revert to just using the index
        //     - we can also shorten the x and y values by removing stuff that's already happened
        let interp_pt_dist: &[f64] = match self.cyc.grade_interp {
            Some(InterpolatorEnum::Interp0D(_)) => &[],
            Some(InterpolatorEnum::Interp1D(_)) => {
                &[vs.dist.get_fresh(|| format_dbg!())?.get::<si::meter>()]
            }
            _ => unreachable!(),
        };
        vs.grade_curr.update(
            if *vs.cyc_met_overall.get_stale(|| format_dbg!())? {
                *self
                    .cyc
                    .grade
                    .get(i)
                    .with_context(|| format_dbg!(self.cyc.grade.len()))?
            } else {
                uc::R
                    * self
                        .cyc
                        .grade_interp
                        .as_ref()
                        .with_context(|| format_dbg!("You might have somehow bypassed `init()`"))?
                        .interpolate(interp_pt_dist)
                        .with_context(|| format_dbg!())?
            },
            || format_dbg!(),
        )?;
        vs.elev_curr.update(
            if *vs.cyc_met_overall.get_stale(|| format_dbg!())? {
                *self.cyc.elev.get(i).with_context(|| format_dbg!())?
            } else {
                uc::M
                    * self
                        .cyc
                        .elev_interp
                        .as_ref()
                        .with_context(|| format_dbg!("You might have somehow bypassed `init()`"))?
                        .interpolate(interp_pt_dist)
                        .with_context(|| format_dbg!())?
            },
            || format_dbg!(),
        )?;

        vs.air_density.update(
            if self.sim_params.f2_const_air_density {
                1.2 * uc::KGPM3
            } else {
                let te_amb_air = {
                    let te_amb_air = self
                        .cyc
                        .temp_amb_air
                        .get(i)
                        .with_context(|| format_dbg!())?;
                    if *te_amb_air == *TE_STD_AIR {
                        None
                    } else {
                        Some(te_amb_air)
                    }
                };
                Air::get_density(
                    te_amb_air.copied(),
                    Some(*vs.elev_curr.get_fresh(|| format_dbg!())?),
                )
            },
            || format_dbg!(),
        )?;

        let mass = self.veh.mass.with_context(|| {
            format!(
                "{}\nVehicle mass should have been set already.",
                format_dbg!()
            )
        })?;
        vs.pwr_accel.update(
            mass / (2.0 * dt) * (speed.powi(P2::new()) - speed_prev.powi(P2::new())),
            || format_dbg!(),
        )?;
        vs.pwr_ascent.update(
            uc::ACC_GRAV
                * *vs.grade_curr.get_fresh(|| format_dbg!())?
                * mass
                * (speed_prev + speed)
                / 2.0,
            || format_dbg!(),
        )?;
        vs.pwr_drag.update(
            0.5
            // TODO: feed in elevation
            * Air::get_density(None, None)
            * self.veh.chassis.drag_coef
            * self.veh.chassis.frontal_area
            * ((speed + speed_prev) / 2.0).powi(P3::new()),
            || format_dbg!(),
        )?;
        vs.pwr_rr.update(
            mass * uc::ACC_GRAV
                * self.veh.chassis.wheel_rr_coef
                * vs.grade_curr.get_fresh(|| format_dbg!())?.atan().cos()
                * (speed_prev + speed)
                / 2.,
            || format_dbg!(),
        )?;
        vs.pwr_whl_inertia.update(
            0.5 * self.veh.chassis.wheel_inertia
                * self.veh.chassis.num_wheels as f64
                * ((speed
                    / self
                        .veh
                        .chassis
                        .wheel_radius
                        .with_context(|| format_dbg!())?)
                .powi(P2::new())
                    - (speed_prev
                        / self
                            .veh
                            .chassis
                            .wheel_radius
                            .with_context(|| format_dbg!())?)
                    .powi(P2::new()))
                / self.cyc.dt_at_i(i).with_context(|| format_dbg!())?,
            || format_dbg!(),
        )?;

        vs.pwr_tractive.update(
            *vs.pwr_rr.get_fresh(|| format_dbg!())?
                + *vs.pwr_whl_inertia.get_fresh(|| format_dbg!())?
                + *vs.pwr_accel.get_fresh(|| format_dbg!())?
                + *vs.pwr_ascent.get_fresh(|| format_dbg!())?
                + *vs.pwr_drag.get_fresh(|| format_dbg!())?,
            || format_dbg!(),
        )?;
        Ok(())
    }

    /// Sets achieved speed based on known current max power
    /// # Arguments
    /// - `cyc_speed`: prescribed speed
    /// - `cyc_dist`: prescribed cumulative distance
    /// - `dt`: simulation time step size
    pub fn set_ach_speed(
        &mut self,
        cyc_speed: si::Velocity,
        cyc_dist: si::Length,
        dt: si::Time,
    ) -> anyhow::Result<()> {
        self.set_ach_speed_at_depth(cyc_speed, cyc_dist, dt, 1)
    }

    /// Implementation of [Self::set_ach_speed] that tracks how many nested
    /// solves have happened within the current time step. `depth` is 1 on the
    /// initial call and increases by one for each re-solve. `depth` is bounded by
    /// `sim_params.ach_speed_max_depth`.
    fn set_ach_speed_at_depth(
        &mut self,
        cyc_speed: si::Velocity,
        cyc_dist: si::Length,
        dt: si::Time,
        depth: u32,
    ) -> anyhow::Result<()> {
        self.veh.state.cyc_met.update(
            self.veh.state.pwr_tractive.get_fresh(|| format_dbg!())?
                <= self
                    .veh
                    .state
                    .pwr_prop_fwd_max
                    .get_fresh(|| format_dbg!())?
                && cyc_speed
                    <= *self
                        .veh
                        .state
                        .speed_trac_fwd_max
                        .get_fresh(|| format_dbg!())?,
            || format_dbg!(),
        )?;
        self.veh.state.cyc_met_overall.update(
            if !*self.veh.state.cyc_met.get_fresh(|| format_dbg!())? {
                // if current power demand is not met, then this becomes false for
                // the rest of the cycle and should not be manipulated anywhere else
                false
            } else {
                *self
                    .veh
                    .state
                    .cyc_met_overall
                    .clone()
                    .get_stale(|| format_dbg!())?
            },
            || format_dbg!(),
        )?;
        let speed_prev = *self.veh.state.speed_ach.get_stale(|| format_dbg!())?;
        if *self.veh.state.cyc_met.get_fresh(|| format_dbg!())? {
            self.veh
                .state
                .speed_ach
                .update(cyc_speed, || format_dbg!())?;
            return Ok(());
        }
        let step_info = StepInfo {
            dt,
            speed_prev,
            cyc_speed,
            grade_curr: *self.veh.state.grade_curr.get_fresh(|| format_dbg!())?,
            air_density: *self.veh.state.air_density.get_fresh(|| format_dbg!())?,
            mass: self.veh.mass.with_context(|| {
                format!("{}\nMass should have been set before now", format_dbg!())
            })?,
            drag_coef: self.veh.chassis.drag_coef,
            frontal_area: self.veh.chassis.frontal_area,
            wheel_inertia: self.veh.chassis.wheel_inertia,
            num_wheels: self.veh.chassis.num_wheels,
            wheel_radius: self
                .veh
                .chassis
                .wheel_radius
                .with_context(|| format_dbg!())?,
            wheel_rr_coef: self.veh.chassis.wheel_rr_coef,
            pwr_prop_fwd_max: *self
                .veh
                .state
                .pwr_prop_fwd_max
                .get_fresh(|| format_dbg!())?,
        };
        let speed_ach = step_info.solve_for_speed(
            self.sim_params.ach_speed_max_iter * 10,
            self.sim_params.ach_speed_tol,
            self.sim_params.ach_speed_solver_gain,
        );
        // Clamp to traction (tire grip) speed limit. The solver operates with
        // the raw powertrain power budget; traction is enforced here as a speed
        // ceiling rather than a power cap, which avoids the near-zero power
        // bottleneck at low speeds (where P = F*v ≈ 0).
        let speed_trac_max = *self
            .veh
            .state
            .speed_trac_fwd_max
            .get_fresh(|| format_dbg!())?;
        let speed_ach = speed_ach.min(speed_trac_max);
        let speed_ach_floored = {
            // NOTE: we only subtract a tiny epsilon when the solved speed exactly equals
            // the previous speed, to guarantee forward progress and avoid float precision
            // issues that could send us right back into trace miss. Previously, this
            // floored to the nearest 0.1 m/s every step, which accumulated significant
            // speed loss during sustained trace-miss events (e.g., 0-60 acceleration).
            if speed_ach == speed_prev {
                (speed_ach - 1.0e-6 * uc::MPS).max(si::Velocity::ZERO)
            } else {
                speed_ach
            }
        };

        self.veh
            .state
            .speed_ach
            .update(speed_ach_floored, || format_dbg!())?;
        // NOTE: need to reset tracked state to allow
        // for calling set_pwr_prop_for_speed(.) again this step.
        // set_pwr_prop_for_speed has already been called so the
        // following variables have already been set fresh but need
        // to be re-iterated.
        self.veh.state.air_density.mark_stale();
        self.veh.state.cyc_met.mark_stale();
        self.veh.state.cyc_met_overall.mark_stale();
        self.veh.state.elev_curr.mark_stale();
        self.veh.state.grade_curr.mark_stale();
        self.veh.state.pwr_accel.mark_stale();
        self.veh.state.pwr_ascent.mark_stale();
        self.veh.state.pwr_drag.mark_stale();
        self.veh.state.pwr_rr.mark_stale();
        self.veh.state.pwr_tractive.mark_stale();
        self.veh.state.pwr_whl_inertia.mark_stale();
        self.veh.state.speed_ach.mark_stale();

        // Rerun again to ensure we have updated achieved speed and state
        self.set_pwr_prop_for_speed(speed_ach_floored, speed_prev, dt)
            .with_context(|| format_dbg!())?;
        ensure!(
            depth < self.sim_params.ach_speed_max_depth,
            "{}\n`set_ach_speed` reached sim_params.ach_speed_max_depth = {} nested solves with solved speed {:?}, pwr_tractive {:?}, pwr_prop_fwd_max {:?}, and grade {:?}",
            format_dbg!(),
            depth,
            speed_ach_floored,
            self.veh.state.pwr_tractive.get_fresh(|| format_dbg!())?,
            step_info.pwr_prop_fwd_max,
            step_info.grade_curr,
        );
        self.set_ach_speed_at_depth(speed_ach, cyc_dist, dt, depth + 1)?;

        match self.sim_params.trace_miss_opts {
            TraceMissOptions::Allow => {
                // do nothing
            }
            TraceMissOptions::AllowChecked => {
                // TraceMissOptions::AllowChecked | TraceMissOptions::Warn => {
                let ach_speed = *self.veh.state.speed_ach.get_fresh(|| format_dbg!())?;
                let ach_dist = *self.veh.state.dist.get_stale(|| format_dbg!())? + ach_speed * dt; // distance actually gets updated after step is completely solved
                self.sim_params
                    .trace_miss_tol
                    .check_trace_miss(cyc_speed, ach_speed, cyc_dist, ach_dist)
                    .map_err(|e| {
                        anyhow!(
                            concat!(
                                "\n{}\n",
                                "{}\n",
                                "failed to meet speed trace\n",
                                "    prescribed speed: {} mph\n",
                                "    achieved speed: {} mph\n",
                                "    pwr_tractive_for_cyc: {} kW\n",
                                "    pwr_tractive: {} kW\n",
                                "    pwr_prop_fwd_max: {} kW\n",
                                "    pwr deficit: {} kW\n",
                            ),
                            format_dbg!(),
                            e,
                            cyc_speed.get::<si::mile_per_hour>(),
                            ach_speed.get::<si::mile_per_hour>(),
                            self.veh
                                .state
                                .pwr_tractive_for_cyc
                                .get_fresh(|| format_dbg!())
                                .unwrap()
                                .get::<si::kilowatt>(),
                            self.veh
                                .state
                                .pwr_tractive
                                .get_fresh(|| format_dbg!())
                                .unwrap()
                                .get::<si::kilowatt>(),
                            self.veh
                                .state
                                .pwr_prop_fwd_max
                                .get_fresh(|| format_dbg!())
                                .unwrap()
                                .get::<si::kilowatt>(),
                            (*self
                                .veh
                                .state
                                .pwr_tractive
                                .get_fresh(|| format_dbg!())
                                .unwrap()
                                - *self
                                    .veh
                                    .state
                                    .pwr_prop_fwd_max
                                    .get_fresh(|| format_dbg!())
                                    .unwrap())
                            .get::<si::kilowatt>()
                            .format_eng(None),
                        )
                    })?;
            }
            TraceMissOptions::Error => {
                let ach_speed = *self.veh.state.speed_ach.get_fresh(|| format_dbg!())?;
                bail!(
                    concat!(
                        "{}\nfailed to meet speed trace\n",
                        "    prescribed speed: {} mph\n",
                        "    achieved speed: {} mph\n",
                        "    pwr_tractive_for_cyc: {} kW\n",
                        "    pwr_tractive: {} kW\n",
                        "    pwr_prop_fwd_max: {} kW\n",
                        "    pwr deficit: {} kW\n",
                    ),
                    format_dbg!(),
                    cyc_speed.get::<si::mile_per_hour>(),
                    ach_speed.get::<si::mile_per_hour>(),
                    self.veh
                        .state
                        .pwr_tractive_for_cyc
                        .get_fresh(|| format_dbg!())?
                        .get::<si::kilowatt>(),
                    self.veh
                        .state
                        .pwr_tractive
                        .get_fresh(|| format_dbg!())?
                        .get::<si::kilowatt>(),
                    self.veh
                        .state
                        .pwr_prop_fwd_max
                        .get_fresh(|| format_dbg!())?
                        .get::<si::kilowatt>(),
                    (*self.veh.state.pwr_tractive.get_fresh(|| format_dbg!())?
                        - *self
                            .veh
                            .state
                            .pwr_prop_fwd_max
                            .get_fresh(|| format_dbg!())?)
                    .get::<si::kilowatt>()
                    .format_eng(None)
                )
            }
            TraceMissOptions::Correct => {
                // We will correct the deviation from trace by modifying the cycle to re-rendezvous with a later time/distance.
                // In so doing, we will use a less agressive roadload.
                let i = *self.veh.state.i.get_fresh(|| format_dbg!())?;
                let max_steps = self.sim_params.trace_miss_correct_max_steps.max(2) as usize;
                let correction = calc_best_rendezvous(i, max_steps, &self.cyc, speed_ach_floored);
                if correction.steps >= 2 {
                    // NOTE: in theory, grade could be slightly
                    // off with this deviation from trace. However, since we
                    // rendezvous in a small number of time steps, it should be
                    // close. The call again to init() should correct distance
                    // and elevation calculations.
                    self.cyc.speed[i] = speed_ach_floored;
                    self.cyc.modify_by_const_jerk_trajectory(
                        i + 1,
                        correction.steps,
                        correction.jerk_m_per_s3 * uc::MPS3,
                        correction.acceleration_m_per_s2 * uc::MPS2,
                    );
                    self.cyc.dist.clear();
                    self.cyc.elev.clear();
                    self.cyc.init().unwrap();
                }
            }
        }

        Ok(())
    }

    pub fn clear(&mut self) {
        self.veh.clear();
    }
}

impl SetCumulative for SimDrive {
    fn set_cumulative<F: Fn() -> String>(&mut self, dt: si::Time, loc: F) -> anyhow::Result<()> {
        self.veh
            .set_cumulative(dt, || format!("{}\n{}", loc(), format_dbg!()))?;
        Ok(())
    }

    fn reset_cumulative<F: Fn() -> String>(&mut self, loc: F) -> anyhow::Result<()> {
        self.veh
            .reset_cumulative(|| format!("{}\n{}", loc(), format_dbg!()))?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::vehicle::vehicle_model::tests::*;

    /// Short cycle no vehicle can follow: 74 mph from a standstill on a
    /// 29.5% grade. `set_ach_speed` has to re-solve on the very first step,
    /// which exercises the `ach_speed_max_depth` guard.
    fn infeasible_grade_cycle() -> Cycle {
        let n = 30;
        let mut cyc = Cycle {
            name: String::from("infeasible grade"),
            init_elev: None,
            time: (0..n).map(|t| t as f64 * uc::S).collect(),
            speed: vec![33.08 * uc::MPS; n],
            dist: vec![],
            grade: vec![0.295 * uc::R; n],
            elev: vec![],
            pwr_max_chrg: vec![],
            grade_interp: Default::default(),
            elev_interp: Default::default(),
            temp_amb_air: Default::default(),
            pwr_solar_load: Default::default(),
        };
        cyc.init().unwrap();
        cyc
    }

    #[test]
    #[cfg(feature = "resources")]
    fn test_ach_speed_max_depth_errors_instead_of_recursing() {
        // With the cap at 1 the first re-solve is refused, so the failure must
        // surface as an error that names the parameter.
        let veh = Vehicle::from_resource("2012_Ford_Fusion.yaml", false).unwrap();
        let sim_params = SimParams {
            ach_speed_max_depth: 1,
            ..Default::default()
        };
        let mut sd = SimDrive::new(veh, infeasible_grade_cycle(), Some(sim_params));
        let err = sd.run().expect_err("infeasible cycle must not succeed");
        let msg = format!("{err:#}");
        assert!(
            msg.contains("ach_speed_max_depth"),
            "expected the depth-limit error to surface, got:\n{msg}"
        );

        // With the default cap the same cycle still fails, but for the usual
        // reason (trace miss), so the cap does not interfere with the normal
        // error path.
        let veh = Vehicle::from_resource("2012_Ford_Fusion.yaml", false).unwrap();
        let mut sd = SimDrive::new(veh, infeasible_grade_cycle(), Default::default());
        let err = sd.run().expect_err("infeasible cycle must not succeed");
        let msg = format!("{err:#}");
        assert!(
            !msg.contains("ach_speed_max_depth"),
            "default cap must not be reached on this cycle, got:\n{msg}"
        );
        assert!(
            msg.contains("failed to meet speed trace"),
            "expected a trace-miss error, got:\n{msg}"
        );
    }

    #[test]
    #[cfg(feature = "resources")]
    fn test_sim_drive_conv() {
        let _veh = Vehicle::from_resource("2012_Ford_Fusion.yaml", false).unwrap();
        let _cyc = Cycle::from_resource("udds.csv", false).unwrap();
        let mut sd = SimDrive::new(_veh, _cyc, Default::default());
        sd.run().unwrap();
        assert!(
            *sd.veh.state.i.get_fresh(String::new).unwrap() == sd.cyc.len_checked().unwrap() - 1
        );
        assert!(
            *sd.veh
                .fc()
                .unwrap()
                .state
                .energy_fuel
                .get_fresh(String::new)
                .unwrap()
                > si::Energy::ZERO
        );
        assert!(sd.veh.res().is_none());
    }

    #[test]
    #[cfg(feature = "resources")]
    fn test_sim_drive_hev() {
        let _veh = Vehicle::from_resource("2016_TOYOTA_Prius_Two.yaml", false).unwrap();
        let _cyc = Cycle::from_resource("udds.csv", false).unwrap();
        let mut sd = SimDrive::new(_veh, _cyc, Default::default());
        sd.run().unwrap();
        assert!(
            *sd.veh.state.i.get_fresh(String::new).unwrap() == sd.cyc.len_checked().unwrap() - 1
        );
        assert!(
            *sd.veh
                .fc()
                .unwrap()
                .state
                .energy_fuel
                .get_fresh(String::new)
                .unwrap()
                > si::Energy::ZERO
        );
        assert!(
            *sd.veh
                .res()
                .unwrap()
                .state
                .energy_out_chemical
                .get_fresh(String::new)
                .unwrap()
                != si::Energy::ZERO
        );
    }

    #[test]
    #[cfg(feature = "resources")]
    fn test_sim_drive_hev_thrml() {
        let _veh =
            Vehicle::from_resource("2021_Hyundai_Sonata_Hybrid_Blue_thrml.yaml", false).unwrap();
        let _cyc = Cycle::from_resource("udds.csv", false).unwrap();

        let te_amb_and_cab_and_batt_init_deg_c: Vec<(f64, f64)> = vec![
            (-6.7, -6.7),
            (5.0, 18.0),
            (22.0, 22.0),
            (25.0, 35.0),
            (45.0, 45.0),
        ];
        let te_amb: Vec<si::Temperature> = te_amb_and_cab_and_batt_init_deg_c
            .iter()
            .map(|t| (t.0 + uc::CELSIUS_TO_KELVIN) * uc::KELVIN)
            .collect();
        let te_batt_and_cab_init: Vec<si::Temperature> = te_amb_and_cab_and_batt_init_deg_c
            .iter()
            .map(|t| (t.1 + uc::CELSIUS_TO_KELVIN) * uc::KELVIN)
            .collect();
        let te_fc_init: Vec<si::Temperature> = [-6.7, 70.0, 90.0]
            .iter()
            .map(|t| (*t + uc::CELSIUS_TO_KELVIN) * uc::KELVIN)
            .collect();
        for ((te_amb, te_init), te_fc_init) in
            te_amb.iter().zip(te_batt_and_cab_init).zip(te_fc_init)
        {
            let mut veh = _veh.clone();

            veh.res_mut()
                .unwrap()
                .res_thrml_state_mut()
                .unwrap()
                .temperature
                .mark_stale();
            veh.res_mut()
                .unwrap()
                .res_thrml_state_mut()
                .unwrap()
                .temperature
                .update(te_init, || format_dbg!())
                .unwrap();

            veh.res_mut()
                .unwrap()
                .res_thrml_state_mut()
                .unwrap()
                .temp_prev
                .mark_stale();
            veh.res_mut()
                .unwrap()
                .res_thrml_state_mut()
                .unwrap()
                .temp_prev
                .update(te_init, || format_dbg!())
                .unwrap();
            if let CabinOption::LumpedCabin(lc) = &mut veh.cabin {
                lc.state.temperature.mark_stale();
                lc.state
                    .temperature
                    .update(te_init, || format_dbg!())
                    .unwrap();
                lc.state.temp_prev.mark_stale();
                lc.state
                    .temp_prev
                    .update(te_init, || format_dbg!())
                    .unwrap();
            }

            veh.fc_mut()
                .unwrap()
                .fc_thrml_state_mut()
                .unwrap()
                .temperature
                .mark_stale();
            veh.fc_mut()
                .unwrap()
                .fc_thrml_state_mut()
                .unwrap()
                .temperature
                .update(te_fc_init, || format_dbg!())
                .unwrap();
            let mut cyc = _cyc.clone();
            cyc.temp_amb_air = vec![*te_amb; cyc.len_checked().unwrap()];
            let mut sd = SimDrive::new(veh, cyc, Default::default());
            sd.run()
                .with_context(|| {
                    format!(
                        "ambient temperature: {}*C\ninit temperature: {}",
                        te_amb.get::<si::degree_celsius>(),
                        te_init.get::<si::degree_celsius>()
                    )
                })
                .unwrap();
            assert!(
                *sd.veh.state.i.get_fresh(String::new).unwrap()
                    == sd.cyc.len_checked().unwrap() - 1
            );
            assert!(
                *sd.veh
                    .fc()
                    .unwrap()
                    .state
                    .energy_fuel
                    .get_fresh(String::new)
                    .unwrap()
                    > si::Energy::ZERO
            );
            assert!(
                *sd.veh
                    .res()
                    .unwrap()
                    .state
                    .energy_out_chemical
                    .get_fresh(String::new)
                    .unwrap()
                    != si::Energy::ZERO
            );
        }
    }

    #[test]
    #[cfg(feature = "resources")]
    /// Simulate prep cycle, soak cycle, and test cycle with thermal effects
    fn test_sim_drive_hev_thrml_soak() {
        let _veh =
            Vehicle::from_resource("2021_Hyundai_Sonata_Hybrid_Blue_thrml.yaml", false).unwrap();
        let mut cyc = Cycle::from_resource("udds.csv", false).unwrap();
        // zero out speed in soak cyc
        let mut soak_cyc_no_temp = cyc.clone();
        soak_cyc_no_temp
            .speed
            .iter_mut()
            .for_each(|v| *v = si::Velocity::ZERO);

        let te_amb: Vec<si::Temperature> = [-6.7, -6.7, 38.0]
            .iter()
            .map(|t| (*t + uc::CELSIUS_TO_KELVIN) * uc::KELVIN)
            .collect();
        let te_batt_and_cab_init: Vec<si::Temperature> = [-6.7, 22.0, 45.0]
            .iter()
            .map(|t| (*t + uc::CELSIUS_TO_KELVIN) * uc::KELVIN)
            .collect();
        let te_fc_init: Vec<si::Temperature> = [-6.7, 70.0, 90.0]
            .iter()
            .map(|t| (*t + uc::CELSIUS_TO_KELVIN) * uc::KELVIN)
            .collect();
        for ((te_amb, te_init), te_fc_init) in
            te_amb.iter().zip(te_batt_and_cab_init).zip(te_fc_init)
        {
            let prep_cyc = cyc
                .with_temp_amb_air(vec![*te_amb; cyc.len_checked().unwrap()])
                .unwrap();
            let soak_cyc = soak_cyc_no_temp
                .with_temp_amb_air(vec![*te_amb; cyc.len_checked().unwrap()])
                .unwrap();
            let test_cyc = cyc
                .with_temp_amb_air(vec![*te_amb; cyc.len_checked().unwrap()])
                .unwrap();

            let mut veh = _veh.clone();

            veh.res_mut()
                .unwrap()
                .res_thrml_state_mut()
                .unwrap()
                .temperature
                .mark_stale();
            veh.res_mut()
                .unwrap()
                .res_thrml_state_mut()
                .unwrap()
                .temperature
                .update(te_init, || format_dbg!())
                .unwrap();

            veh.res_mut()
                .unwrap()
                .res_thrml_state_mut()
                .unwrap()
                .temp_prev
                .mark_stale();
            veh.res_mut()
                .unwrap()
                .res_thrml_state_mut()
                .unwrap()
                .temp_prev
                .update(te_init, || format_dbg!())
                .unwrap();
            if let CabinOption::LumpedCabin(lc) = &mut veh.cabin {
                lc.state.temperature.mark_stale();
                lc.state
                    .temperature
                    .update(te_init, || format_dbg!())
                    .unwrap();
                lc.state.temp_prev.mark_stale();
                lc.state
                    .temp_prev
                    .update(te_init, || format_dbg!())
                    .unwrap();
            }

            veh.fc_mut()
                .unwrap()
                .fc_thrml_state_mut()
                .unwrap()
                .temperature
                .mark_stale();
            veh.fc_mut()
                .unwrap()
                .fc_thrml_state_mut()
                .unwrap()
                .temperature
                .update(te_fc_init, || format_dbg!())
                .unwrap();

            // simulate prep cycle
            dbg!("Running `sd_prep`");
            let mut sd_prep = SimDrive::new(veh, prep_cyc, None);
            sd_prep
                .run()
                .with_context(|| {
                    format!(
                        "\nprep cycle:\nambient temperature: {}*C\ninit temperature: {}",
                        te_amb.get::<si::degree_celsius>(),
                        te_init.get::<si::degree_celsius>()
                    )
                })
                .unwrap();
            assert!(
                *sd_prep.veh.state.i.get_fresh(String::new).unwrap()
                    == sd_prep.cyc.len_checked().unwrap() - 1
            );
            sd_prep.reset_step(|| format_dbg!()).unwrap();
            sd_prep.veh.clear();
            sd_prep.reset_cumulative(|| format_dbg!()).unwrap();

            // simulate soak cycle
            dbg!("Running `sd_soak`");
            let mut sd_soak = SimDrive::new(
                sd_prep.veh.clone(),
                soak_cyc,
                Some(SimParams {
                    ambient_thermal_soak: true,
                    ..Default::default()
                }),
            );
            sd_soak
                .run()
                .with_context(|| {
                    format!(
                        "\nsoak cycle:\nambient temperature: {}*C\ninit temperature: {}",
                        te_amb.get::<si::degree_celsius>(),
                        te_init.get::<si::degree_celsius>()
                    )
                })
                .unwrap();
            assert!(
                *sd_soak.veh.state.i.get_fresh(String::new).unwrap()
                    == sd_soak.cyc.len_checked().unwrap() - 1
            );
            sd_soak.reset_step(|| format_dbg!()).unwrap();
            sd_soak.veh.clear();
            sd_soak.reset_cumulative(|| format_dbg!()).unwrap();

            // simulate test cycle
            dbg!("Running `sd_test`");
            let mut sd_test = SimDrive::new(sd_soak.veh.clone(), test_cyc, None);
            sd_test
                .run()
                .with_context(|| {
                    format!(
                        "\ntest cycle:\nambient temperature: {}*C\ninit temperature: {}",
                        te_amb.get::<si::degree_celsius>(),
                        te_init.get::<si::degree_celsius>()
                    )
                })
                .unwrap();
            assert!(
                *sd_test.veh.state.i.get_fresh(String::new).unwrap()
                    == sd_test.cyc.len_checked().unwrap() - 1
            );
            sd_test.reset_step(|| format_dbg!()).unwrap();
            sd_test.veh.clear();
            sd_test.reset_cumulative(|| format_dbg!()).unwrap();
        }
    }

    #[test]
    #[cfg(feature = "resources")]
    /// Simulate prep cycle, soak cycle, and test cycle with thermal effects
    fn test_sim_drive_bev_thrml_soak() {
        let _veh = Vehicle::from_resource("2020 Chevrolet Bolt EV thrml.yaml", false).unwrap();
        let mut cyc = Cycle::from_resource("udds.csv", false).unwrap();
        // zero out speed in soak cyc
        let mut soak_cyc_no_temp = cyc.clone();
        soak_cyc_no_temp
            .speed
            .iter_mut()
            .for_each(|v| *v = si::Velocity::ZERO);

        let te_amb: Vec<si::Temperature> = [-6.7, -6.7, 38.0]
            .iter()
            .map(|t| (*t + uc::CELSIUS_TO_KELVIN) * uc::KELVIN)
            .collect();
        let te_batt_and_cab_init: Vec<si::Temperature> = [-6.7, 22.0, 45.0]
            .iter()
            .map(|t| (*t + uc::CELSIUS_TO_KELVIN) * uc::KELVIN)
            .collect();

        // sweep ambient and initial conditions
        for (te_amb, te_init) in te_amb.iter().zip(te_batt_and_cab_init) {
            let prep_cyc = cyc
                .with_temp_amb_air(vec![*te_amb; cyc.len_checked().unwrap()])
                .unwrap();
            let soak_cyc = soak_cyc_no_temp
                .with_temp_amb_air(vec![*te_amb; cyc.len_checked().unwrap()])
                .unwrap();
            let test_cyc = cyc
                .with_temp_amb_air(vec![*te_amb; cyc.len_checked().unwrap()])
                .unwrap();
            let mut veh = _veh.clone();

            veh.res_mut()
                .unwrap()
                .res_thrml_state_mut()
                .unwrap()
                .temperature
                .mark_stale();
            veh.res_mut()
                .unwrap()
                .res_thrml_state_mut()
                .unwrap()
                .temperature
                .update(te_init, || format_dbg!())
                .unwrap();

            veh.res_mut()
                .unwrap()
                .res_thrml_state_mut()
                .unwrap()
                .temp_prev
                .mark_stale();
            veh.res_mut()
                .unwrap()
                .res_thrml_state_mut()
                .unwrap()
                .temp_prev
                .update(te_init, || format_dbg!())
                .unwrap();

            // setup initial conditions
            if let CabinOption::LumpedCabin(lc) = &mut veh.cabin {
                lc.state.temperature.mark_stale();
                lc.state
                    .temperature
                    .update(te_init, || format_dbg!())
                    .unwrap();
                lc.state.temp_prev.mark_stale();
                lc.state
                    .temp_prev
                    .update(te_init, || format_dbg!())
                    .unwrap();
            }

            // simulate prep cycle
            dbg!("Running `sd_prep`");
            let mut sd_prep = SimDrive::new(veh, prep_cyc, None);
            sd_prep
                .run()
                .with_context(|| {
                    format!(
                        "\nprep cycle:\nambient temperature: {}*C\ninit temperature: {}",
                        te_amb.get::<si::degree_celsius>(),
                        te_init.get::<si::degree_celsius>()
                    )
                })
                .unwrap();
            assert!(
                *sd_prep.veh.state.i.get_fresh(String::new).unwrap()
                    == sd_prep.cyc.len_checked().unwrap() - 1
            );
            sd_prep.reset_step(|| format_dbg!()).unwrap();
            sd_prep.veh.clear();
            sd_prep.reset_cumulative(|| format_dbg!()).unwrap();

            // simulate soak cycle
            dbg!("Running `sd_soak`");
            let mut sd_soak = SimDrive::new(
                sd_prep.veh.clone(),
                soak_cyc,
                Some(SimParams {
                    ambient_thermal_soak: true,
                    ..Default::default()
                }),
            );
            sd_soak
                .run()
                .with_context(|| {
                    format!(
                        "\nsoak cycle:\nambient temperature: {}*C\ninit temperature: {}",
                        te_amb.get::<si::degree_celsius>(),
                        te_init.get::<si::degree_celsius>()
                    )
                })
                .unwrap();
            assert!(
                *sd_soak.veh.state.i.get_fresh(String::new).unwrap()
                    == sd_soak.cyc.len_checked().unwrap() - 1
            );
            sd_soak.reset_step(|| format_dbg!()).unwrap();
            sd_soak.veh.clear();
            sd_soak.reset_cumulative(|| format_dbg!()).unwrap();

            // simulate test cycle
            dbg!("Running `sd_test`");
            let mut sd_test = SimDrive::new(sd_soak.veh.clone(), test_cyc, None);
            sd_test
                .run()
                .with_context(|| {
                    format!(
                        "\ntest cycle:\nambient temperature: {}*C\ninit temperature: {}",
                        te_amb.get::<si::degree_celsius>(),
                        te_init.get::<si::degree_celsius>()
                    )
                })
                .unwrap();
            assert!(
                *sd_test.veh.state.i.get_fresh(String::new).unwrap()
                    == sd_test.cyc.len_checked().unwrap() - 1
            );
            sd_test.reset_step(|| format_dbg!()).unwrap();
            sd_test.veh.clear();
            sd_test.reset_cumulative(|| format_dbg!()).unwrap();
        }
    }

    #[test]
    #[cfg(feature = "resources")]
    fn test_sim_drive_bev() {
        let _veh = Vehicle::from_resource("2022_Renault_Zoe_ZE50_R135.yaml", false).unwrap();
        let _cyc = Cycle::from_resource("udds.csv", false).unwrap();
        let mut sd = SimDrive {
            veh: _veh,
            cyc: _cyc,
            sim_params: Default::default(),
        };
        sd.run().unwrap();
        assert!(
            *sd.veh.state.i.get_fresh(String::new).unwrap() == sd.cyc.len_checked().unwrap() - 1
        );
        assert!(sd.veh.fc().is_none());
        assert!(
            *sd.veh
                .res()
                .unwrap()
                .state
                .energy_out_chemical
                .get_fresh(String::new)
                .unwrap()
                != si::Energy::ZERO
        );
    }

    #[test]
    #[cfg(feature = "resources")]
    fn test_sim_drive_bev_thrml() {
        let _veh = Vehicle::from_resource("2020 Chevrolet Bolt EV thrml.yaml", false).unwrap();
        let _cyc = Cycle::from_resource("udds.csv", false).unwrap();

        let te_amb_and_cab_and_batt_init_deg_c: Vec<(f64, f64)> = vec![
            (-6.7, -6.7),
            (5.0, 18.0),
            (22.0, 22.0),
            (25.0, 35.0),
            (45.0, 45.0),
        ];
        let te_amb: Vec<si::Temperature> = te_amb_and_cab_and_batt_init_deg_c
            .iter()
            .map(|t| (t.0 + uc::CELSIUS_TO_KELVIN) * uc::KELVIN)
            .collect();
        let te_batt_and_cab_init: Vec<si::Temperature> = te_amb_and_cab_and_batt_init_deg_c
            .iter()
            .map(|t| (t.1 + uc::CELSIUS_TO_KELVIN) * uc::KELVIN)
            .collect();
        for (te_amb, te_init) in te_amb.iter().zip(te_batt_and_cab_init) {
            let mut veh = _veh.clone();
            veh.res_mut()
                .unwrap()
                .res_thrml_state_mut()
                .unwrap()
                .temperature
                .mark_stale();
            veh.res_mut()
                .unwrap()
                .res_thrml_state_mut()
                .unwrap()
                .temperature
                .update(te_init, || format_dbg!())
                .unwrap();

            veh.res_mut()
                .unwrap()
                .res_thrml_state_mut()
                .unwrap()
                .temp_prev
                .mark_stale();
            veh.res_mut()
                .unwrap()
                .res_thrml_state_mut()
                .unwrap()
                .temp_prev
                .update(te_init, || format_dbg!())
                .unwrap();

            if let CabinOption::LumpedCabin(lc) = &mut veh.cabin {
                lc.state.temperature.mark_stale();
                lc.state
                    .temperature
                    .update(te_init, || format_dbg!())
                    .unwrap();

                lc.state.temp_prev.mark_stale();
                lc.state
                    .temp_prev
                    .update(te_init, || format_dbg!())
                    .unwrap();
            } else {
                panic!("cabin should have been configured");
            }
            let mut cyc = _cyc.clone();
            cyc.temp_amb_air = vec![*te_amb; cyc.len_checked().unwrap()];
            let mut sd = SimDrive::new(veh, cyc, Default::default());
            if let CabinOption::LumpedCabin(lc) = sd.veh.cabin.clone() {
                assert_eq!(
                    *lc.state.temperature.get_fresh(|| format_dbg!()).unwrap(),
                    te_init
                );
            } else {
                panic!();
            };
            sd.run()
                .with_context(|| {
                    format!(
                        "ambient temperature: {}*C\ninit temperature: {}",
                        te_amb.get::<si::degree_celsius>(),
                        te_init.get::<si::degree_celsius>()
                    )
                })
                .unwrap();
            assert!(
                *sd.veh.state.i.get_fresh(String::new).unwrap()
                    == sd.cyc.len_checked().unwrap() - 1
            );
            assert!(sd.veh.fc().is_none());
            assert!(
                *sd.veh
                    .res()
                    .unwrap()
                    .state
                    .energy_out_chemical
                    .get_fresh(String::new)
                    .unwrap()
                    != si::Energy::ZERO
            );
            sd.veh.reset_step(|| format_dbg!()).unwrap();
            sd.veh.state.time.mark_stale();
            sd.veh
                .state
                .time
                .update(si::Time::ZERO, || format_dbg!())
                .unwrap();
            assert!(*sd.veh.state.i.get_fresh(|| format_dbg!()).unwrap() == 0);
            sd.run()
                .with_context(|| {
                    format!(
                        "ambient temperature: {}*C\ninit temperature: {}",
                        te_amb.get::<si::degree_celsius>(),
                        te_init.get::<si::degree_celsius>()
                    )
                })
                .unwrap();
            sd.reset_cumulative(|| format_dbg!()).unwrap();
            assert_eq!(*sd.veh.state.i.get_fresh(|| format_dbg!()).unwrap(), 1369);
        }
    }
}
