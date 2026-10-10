use super::utils::ScalingMethods;
use super::*;
use crate::prelude::*;
use crate::utils::interp::InterpolatorMutMethods;
use std::f64::consts::PI;

// TODO: think about how to incorporate life modeling for Fuel Cells and other tech

#[serde_api]
#[derive(Deserialize, Serialize, Debug, Clone, PartialEq, StateMethods, SetCumulative)]
/// Struct for modeling [FuelConverter] (e.g. engine, fuel cell.) thermal plant
#[non_exhaustive]
#[serde(deny_unknown_fields)]
#[cfg_attr(feature = "pyo3", pyclass(module = "fastsim", subclass, eq))]
pub struct FuelConverter {
    /// [Self] Thermal plant, including thermal management controls
    #[serde(default)]
    #[has_state]
    pub thrml: FuelConverterThermalOption,
    /// [Self] mass
    #[serde(default)]
    pub(crate) mass: Option<si::Mass>,
    /// FuelConverter specific power
    pub(crate) specific_pwr: Option<si::SpecificPower>,
    /// max rated brake output power
    pub pwr_out_max: si::Power,
    /// starting/baseline transient power limit
    #[serde(default)]
    pub pwr_out_max_init: si::Power,
    // TODO: consider a ramp down rate, which may be needed for fuel cells
    /// lag time for ramp up
    pub pwr_ramp_lag: si::Time,
    /// interpolator for calculating [Self] efficiency as a function of output power
    #[serde(serialize_with = "serialize_nested")]
    pub eff_interp_from_pwr_out: InterpolatorEnum<f64>,
    /// power at which peak efficiency occurs
    #[serde(skip)]
    pub(crate) pwr_for_peak_eff: si::Power,
    /// idle fuel power to overcome internal friction (not including aux load) \[W\]
    pub pwr_idle_fuel: si::Power,
    /// struct for tracking current state
    #[serde(default)]
    pub state: FuelConverterState,
    /// Custom vector of [Self::state]
    #[serde(
        default,
        skip_serializing_if = "FuelConverterStateHistoryVec::is_empty"
    )]
    pub history: FuelConverterStateHistoryVec,
    /// time step interval between saves. 1 is a good option. If None, no saving occurs.
    #[py_get]
    pub save_interval: Option<usize>,
}

#[pyo3_api]
impl FuelConverter {
    // optional, custom, struct-specific pymethods
    #[getter("eff_max")]
    fn get_eff_max_py(&self) -> PyResult<f64> {
        Ok(*self.get_eff_max()?)
    }

    #[setter("__eff_max")]
    fn set_eff_max_py(&mut self, eff_max: f64) -> PyResult<()> {
        Ok(self.set_eff_max(eff_max, None)?)
    }

    #[getter("eff_min")]
    fn get_eff_min_py(&self) -> PyResult<f64> {
        Ok(*self.get_eff_min()?)
    }

    #[setter("__eff_min")]
    fn set_eff_min_py(&mut self, eff_min: f64) -> PyResult<()> {
        Ok(self.set_eff_min(eff_min, None)?)
    }

    #[setter("__eff_range")]
    fn set_eff_range_py(&mut self, eff_range: f64) -> PyResult<()> {
        self.set_eff_range(eff_range)?;
        Ok(())
    }

    // TODO: handle `side_effects` and uncomment
    // #[setter("__mass_kg")]
    // fn set_mass_py(&mut self, mass_kg: Option<f64>) -> anyhow::Result<()> {
    //     self.set_mass(mass_kg.map(|m| m * uc::KG))?;
    //     Ok(())
    // }

    #[getter("mass_kg")]
    fn get_mass_py(&self) -> PyResult<Option<f64>> {
        Ok(self.mass()?.map(|m| m.get::<si::kilogram>()))
    }

    #[getter]
    fn get_specific_pwr_kw_per_kg(&self) -> Option<f64> {
        self.specific_pwr
            .map(|x| x.get::<si::kilowatt_per_kilogram>())
    }
}

/// implementing constructor for FuelConverter
impl FuelConverter {
    pub fn new(
        thrml: FuelConverterThermalOption,
        mass: Option<si::Mass>,
        specific_pwr: Option<si::SpecificPower>,
        pwr_out_max: si::Power,
        pwr_out_max_init: si::Power,
        pwr_ramp_lag: si::Time,
        eff_interp_from_pwr_out: InterpolatorEnum<f64>,
        pwr_for_peak_eff: si::Power,
        pwr_idle_fuel: si::Power,
        save_interval: Option<usize>,
    ) -> anyhow::Result<Self> {
        let mut fc = Self {
            thrml,
            mass,
            specific_pwr,
            pwr_out_max,
            pwr_out_max_init,
            pwr_ramp_lag,
            eff_interp_from_pwr_out,
            pwr_for_peak_eff,
            pwr_idle_fuel,
            state: FuelConverterState::default(),
            history: FuelConverterStateHistoryVec::default(),
            save_interval,
        };
        fc.init()?;
        Ok(fc)
    }
}

impl SerdeAPI for FuelConverter {}
impl Init for FuelConverter {
    fn init(&mut self) -> Result<(), Error> {
        let _ = self
            .mass()
            .map_err(|err| Error::InitError(format_dbg!(err)))?;
        self.thrml.init()?;
        self.state
            .init()
            .map_err(|err| Error::InitError(format_dbg!(err)))?;
        let eff_max = self
            .get_eff_max()
            .map_err(|err| Error::InitError(format_dbg!(err)))?;
        self.pwr_for_peak_eff = match &self.eff_interp_from_pwr_out {
            InterpolatorEnum::Interp1D(interp) => *interp.data.grid[0]
                .get(
                    interp
                        .data
                        .values
                        .iter()
                        .position(|eff| eff == eff_max)
                        .ok_or_else(|| Error::InitError(format_dbg!()))?,
                )
                .ok_or_else(|| Error::InitError(format_dbg!()))?,
            _ => {
                return Err(Error::InitError(format_dbg!(
                    "Only 1-D interpolators are supported"
                )))
            }
        } * self.pwr_out_max;
        Ok(())
    }
}
impl HistoryMethods for FuelConverter {
    fn save_interval(&self) -> anyhow::Result<Option<usize>> {
        Ok(self.save_interval)
    }
    fn set_save_interval(&mut self, save_interval: Option<usize>) -> anyhow::Result<()> {
        self.save_interval = save_interval;
        self.thrml.set_save_interval(save_interval)?;
        Ok(())
    }
    fn clear(&mut self) {
        self.history.clear();
        self.thrml.clear();
    }
}

impl Mass for FuelConverter {
    fn mass(&self) -> anyhow::Result<Option<si::Mass>> {
        let derived_mass = self
            .derived_mass()
            .with_context(|| anyhow!(format_dbg!()))?;
        if let (Some(derived_mass), Some(set_mass)) = (derived_mass, self.mass) {
            ensure!(
                utils::almost_eq_uom(&set_mass, &derived_mass, None),
                format!(
                    "{}",
                    format_dbg!(utils::almost_eq_uom(&set_mass, &derived_mass, None)),
                )
            );
        }
        Ok(self.mass)
    }

    fn set_mass(
        &mut self,
        new_mass: Option<si::Mass>,
        side_effect: MassSideEffect,
    ) -> anyhow::Result<()> {
        let derived_mass = self
            .derived_mass()
            .with_context(|| anyhow!(format_dbg!()))?;
        self.mass = match (new_mass, derived_mass) {
            // Set using provided `new_mass`, setting constituent mass fields to `None` to match if inconsistent
            (Some(new_mass), Some(dm)) => {
                if dm != new_mass {
                    match side_effect {
                        MassSideEffect::Extensive => {
                            self.pwr_out_max = self.specific_pwr.with_context(|| {
                                format!(
                                    "{}\nExpected `self.specific_pwr` to be `Some`.",
                                    format_dbg!()
                                )
                            })? * new_mass;
                        }
                        MassSideEffect::Intensive => {
                            self.specific_pwr = Some(self.pwr_out_max / new_mass);
                        }
                        MassSideEffect::None => {
                            self.specific_pwr = None;
                        }
                    }
                }
                Some(new_mass)
            }
            (Some(new_mass), None) => Some(new_mass),
            (None, Some(dm)) => Some(dm),
            (None, None) => bail!(
                "Not all mass fields in `{}` are set and no mass was provided.",
                stringify!(FuelConverter)
            ),
        };
        ensure!(
            self.mass > Some(0.0 * uc::KG),
            "{} mass must be positive",
            stringify!(FuelConverter)
        );
        Ok(())
    }

    fn derived_mass(&self) -> anyhow::Result<Option<si::Mass>> {
        Ok(self
            .specific_pwr
            .map(|specific_pwr| self.pwr_out_max / specific_pwr))
    }

    fn expunge_mass_fields(&mut self) {
        self.mass = None;
        self.specific_pwr = None;
    }
}

// non-py methods
impl FuelConverter {
    /// Sets maximum possible total power [FuelConverter]
    /// can produce.
    /// # Arguments
    /// - `dt`: simulation time step size
    pub fn set_curr_pwr_out_max(&mut self, dt: si::Time) -> anyhow::Result<()> {
        if self.pwr_out_max_init == si::Power::ZERO {
            // TODO: think about how to initialize power
            self.pwr_out_max_init = self.pwr_out_max / 10.
        };
        let pwr_out_max = (*self.state.pwr_prop.get_stale(|| format_dbg!())?
            + *self.state.pwr_aux.get_stale(|| format_dbg!())?
            + self.pwr_out_max / self.pwr_ramp_lag * dt)
            .min(self.pwr_out_max)
            .max(self.pwr_out_max_init);
        self.state
            .pwr_out_max
            .update(pwr_out_max, || format_dbg!())?;
        Ok(())
    }

    /// Sets maximum possible propulsion-related power [FuelConverter]
    /// can produce, accounting for any aux-related power required.
    /// # Arguments
    /// - `pwr_aux`: aux-related power required from this component
    pub fn set_curr_pwr_prop_max(&mut self, pwr_aux: si::Power) -> anyhow::Result<()> {
        ensure!(
            pwr_aux >= si::Power::ZERO,
            format!(
                "{}\n`pwr_aux` must be >= 0",
                format_dbg!(pwr_aux >= si::Power::ZERO),
            )
        );
        self.state.pwr_aux.update(pwr_aux, || format_dbg!())?;
        self.state.pwr_prop_max.update(
            *self.state.pwr_out_max.get_fresh(|| format_dbg!())? - pwr_aux,
            || format_dbg!(),
        )?;
        Ok(())
    }

    /// Solves for this powertrain system/component efficiency and sets/returns power output values.
    /// # Arguments
    /// - `pwr_out_req`: tractive power output required to achieve presribed speed
    /// - `fc_on`: whether component is actively running
    /// - `dt`: simulation time step size
    pub fn solve(
        &mut self,
        pwr_out_req: si::Power,
        fc_on: bool,
        dt: si::Time,
    ) -> anyhow::Result<()> {
        self.state.fc_on.update(fc_on, || format_dbg!())?;
        if fc_on {
            self.state.time_on.increment(dt, || format_dbg!())?;
        } else {
            self.state
                .time_on
                .update(si::Time::ZERO, || format_dbg!())?;
        }
        // NOTE: think about the possibility of engine braking, not urgent
        ensure!(
            pwr_out_req >= si::Power::ZERO,
            format!(
                "{}\n`pwr_out_req` must be >= 0",
                format_dbg!(pwr_out_req >= si::Power::ZERO),
            )
        );
        ensure!(
            pwr_out_req <= *self.state.pwr_prop_max.get_fresh(|| format_dbg!())?,
            format!(
                "{}\n`pwr_out_req` ({} W) must be < `self.state.pwr_prop_max` ({} W)",
                format_dbg!(),
                pwr_out_req.get::<si::watt>().format_eng(Some(5)),
                self.state
                    .pwr_prop_max
                    .get_fresh(|| format_dbg!())?
                    .get::<si::watt>()
                    .format_eng(Some(5))
            )
        );
        // if the engine is not on, `pwr_out_req` should be 0.0
        ensure!(
            fc_on || (pwr_out_req == si::Power::ZERO && *self.state.pwr_aux.get_fresh(|| format_dbg!())? == si::Power::ZERO),
            format!(
                "{}\nEngine is off but pwr_out_req + pwr_aux is non-zero\n`pwr_out_req`: {} kW\n`self.state.pwr_aux`: {} kW",
                format_dbg!(
                    fc_on
                        || (pwr_out_req == si::Power::ZERO
                            && *self.state.pwr_aux.get_fresh(|| format_dbg!())? == si::Power::ZERO)
                ),
               pwr_out_req.get::<si::kilowatt>(),
               self.state.pwr_aux.get_fresh(|| format_dbg!())?.get::<si::kilowatt>()
            )
        );
        self.state.pwr_prop.update(pwr_out_req, || format_dbg!())?;
        self.state.eff.update(
            if fc_on {
                uc::R
                    * self
                        .eff_interp_from_pwr_out
                        .interpolate(&[((pwr_out_req
                            + *self.state.pwr_aux.get_fresh(|| format_dbg!())?)
                            / self.pwr_out_max)
                            .get::<si::ratio>()])
                        .with_context(|| {
                            anyhow!(
                                "{}\n failed to calculate {}",
                                format_dbg!(),
                                stringify!(self.state.eff)
                            )
                        })?
            } else {
                si::Ratio::ZERO
            } * match self.thrml.temp_eff_coeff() {
                Some(tec) => *tec.get_fresh(|| format_dbg!())?,
                None => 1.0 * uc::R,
            },
            || format_dbg!(),
        )?;
        ensure!(
            (*self.state.eff.get_fresh(|| format_dbg!())? >= 0.0 * uc::R
                && *self.state.eff.get_fresh(|| format_dbg!())? <= 1.0 * uc::R),
            format!(
                "fc efficiency ({}) must be either between 0 and 1",
                self.state
                    .eff
                    .get_fresh(|| format_dbg!())?
                    .get::<si::ratio>()
            )
        );

        self.state.pwr_fuel.update(
            if *self.state.fc_on.get_fresh(|| format_dbg!())? {
                ((pwr_out_req + *self.state.pwr_aux.get_fresh(|| format_dbg!())?)
                    / *self.state.eff.get_fresh(|| format_dbg!())?)
                .max(self.pwr_idle_fuel)
            } else {
                si::Power::ZERO
            },
            || format_dbg!(),
        )?;
        self.state.pwr_loss.update(
            *self.state.pwr_fuel.get_fresh(|| format_dbg!())?
                - *self.state.pwr_prop.get_fresh(|| format_dbg!())?,
            || format_dbg!(),
        )?;

        // TODO: put this in `SetCumulative::set_custom_cumulative`
        // ensure!(
        //     self.state.energy_loss.get::<si::joule>() >= 0.0,
        //     format!(
        //         "{}\nEnergy loss must be non-negative",
        //         format_dbg!(self.state.energy_loss.get::<si::joule>() >= 0.0)
        //     )
        // );
        Ok(())
    }

    pub fn solve_thermal(
        &mut self,
        te_amb: si::Temperature,
        pwr_thrml_fc_to_cab: Option<si::Power>,
        veh_state: &mut VehicleState,
        dt: si::Time,
    ) -> anyhow::Result<()> {
        let veh_speed = *veh_state.speed_ach.get_stale(|| format_dbg!())?;
        self.thrml
            .solve_thermal(&self.state, te_amb, pwr_thrml_fc_to_cab, veh_speed, dt)
            .with_context(|| format_dbg!())
    }

    /// If thermal model is appropriately configured, returns current lumped [Self] temperature
    pub fn temperature(&self) -> Option<&TrackedState<si::Temperature>> {
        match &self.thrml {
            FuelConverterThermalOption::FuelConverterThermal(fct) => Some(&fct.state.temperature),
            FuelConverterThermalOption::None => None,
        }
    }

    /// Returns max value of [Self::eff_interp_from_pwr_out]
    pub fn get_eff_max(&self) -> anyhow::Result<&f64> {
        self.eff_interp_from_pwr_out.max()
    }

    /// Returns min value of [Self::eff_interp_from_pwr_out]
    pub fn get_eff_min(&self) -> anyhow::Result<&f64> {
        self.eff_interp_from_pwr_out.min()
    }

    /// Scales eff_interp_fwd and eff_interp_bwd by ratio of new `eff_max` per
    /// current calculated max (Note: this may change eff_min)
    pub fn set_eff_max(
        &mut self,
        eff_max: f64,
        scaling: Option<ScalingMethods>,
    ) -> anyhow::Result<()> {
        if (0.0..=1.0).contains(&eff_max) {
            self.eff_interp_from_pwr_out.set_max(eff_max, scaling)?;
        } else {
            return Err(anyhow!(
                "`eff_max` ({:.3}) must be between 0.0 and 1.0",
                eff_max,
            ));
        }
        // to update any dependent fields
        self.init().map_err(|err| anyhow!("{:?}", err))?;
        Ok(())
    }

    /// Scales eff_interp_fwd and eff_interp_bwd by ratio of new `eff_min` per
    /// current calculated min (Note: this may change eff_max)
    pub fn set_eff_min(
        &mut self,
        eff_min: f64,
        scaling: Option<ScalingMethods>,
    ) -> anyhow::Result<()> {
        self.eff_interp_from_pwr_out.set_min(eff_min, scaling)
    }

    /// Scales values of `eff_interp_fwd.f_x` and `eff_interp_bwd.f_x` without
    /// changing max such that max - min is equal to new range.  Will change max
    /// if needed to ensure no values are less than zero.
    pub fn set_eff_range(&mut self, eff_range: f64) -> anyhow::Result<()> {
        if (0. ..=1.0).contains(&eff_range) {
            self.eff_interp_from_pwr_out.set_range(eff_range)
        } else {
            Err(anyhow!(format!(
                "`eff_range` ({:.3}) must be between 0.0 and 1.0",
                eff_range,
            )))
        }
    }

    pub fn fc_thrml_state_mut(&mut self) -> Option<&mut FuelConverterThermalState> {
        match &mut self.thrml {
            FuelConverterThermalOption::FuelConverterThermal(fct) => Some(&mut fct.state),
            FuelConverterThermalOption::None => None,
        }
    }
}

impl TryFrom<FCBuilder> for FuelConverter {
    type Error = anyhow::Error;
    fn try_from(fcbuilder: FCBuilder) -> Result<FuelConverter, anyhow::Error> {
        let mut fc = FuelConverter {
            state: Default::default(),
            thrml: Default::default(),
            mass: None,
            specific_pwr: None,
            pwr_out_max: fcbuilder.pwr_out_max,
            // assumes 1 s time step
            pwr_out_max_init: fcbuilder.pwr_out_max / fcbuilder.pwr_ramp_lag.get::<si::second>(),
            pwr_ramp_lag: fcbuilder.pwr_ramp_lag,
            eff_interp_from_pwr_out: fcbuilder.eff_interp_from_pwr_out,
            pwr_for_peak_eff: uc::KW * f64::NAN, // this gets updated in `init`
            // TODO: make a function for setting this according with below line
            // this means that aux power must include idle fuel
            pwr_idle_fuel: si::Power::ZERO,
            save_interval: Some(1),
            history: Default::default(),
        };
        fc.init()?;
        Ok(fc)
    }
}

#[serde_api]
#[derive(Deserialize, Serialize, Debug, Clone, PartialEq)]
#[serde(deny_unknown_fields)]
#[cfg_attr(feature = "pyo3", pyclass(module = "fastsim", subclass, eq))]
/// Builder for [FuelConverter].  Use this to instantiate EM with minimal parameterization
pub struct FCBuilder {
    pub pwr_out_max: si::Power,
    // TODO: consider a ramp down rate, which may be needed for fuel cells
    /// lag time for ramp up
    pub pwr_ramp_lag: si::Time,
    /// interpolator for calculating [Self] efficiency as a function of output power
    #[serde(serialize_with = "serialize_nested")]
    pub eff_interp_from_pwr_out: InterpolatorEnum<f64>,
    /// power at which peak efficiency occurs
    #[serde(skip)]
    pub(crate) pwr_for_peak_eff: si::Power,
    /// idle fuel power to overcome internal friction (not including aux load) \[W\]
    pub pwr_idle_fuel: si::Power,
    /// time step interval between saves. 1 is a good option. If None, no saving occurs.
    pub save_interval: Option<usize>,
}

#[serde_api]
#[derive(
    Clone,
    Debug,
    Default,
    Deserialize,
    Serialize,
    PartialEq,
    HistoryVec,
    StateMethods,
    SetCumulative,
)]
#[non_exhaustive]
#[serde(default)]
#[serde(deny_unknown_fields)]
#[cfg_attr(feature = "pyo3", pyclass(module = "fastsim", subclass, eq))]
pub struct FuelConverterState {
    /// time step index
    pub i: TrackedState<usize>,
    /// max total output power fc can produce at current time
    pub pwr_out_max: TrackedState<si::Power>,
    /// max propulsion power fc can produce at current time
    pub pwr_prop_max: TrackedState<si::Power>,
    /// efficiency evaluated at current demand
    pub eff: TrackedState<si::Ratio>,
    /// instantaneous power going to drivetrain, not including aux
    pub pwr_prop: TrackedState<si::Power>,
    /// integral of [Self::pwr_prop]
    pub energy_prop: TrackedState<si::Energy>,
    /// power going to auxiliaries
    pub pwr_aux: TrackedState<si::Power>,
    /// Integral of [Self::pwr_aux]
    pub energy_aux: TrackedState<si::Energy>,
    /// instantaneous fuel power flow
    pub pwr_fuel: TrackedState<si::Power>,
    /// Integral of [Self::pwr_fuel]
    pub energy_fuel: TrackedState<si::Energy>,
    /// loss power, including idle
    pub pwr_loss: TrackedState<si::Power>,
    /// Integral of [Self::pwr_loss]
    pub energy_loss: TrackedState<si::Energy>,
    /// If true, engine is on, and if false, off (no idle)
    pub fc_on: TrackedState<bool>,
    /// Time the engine has been on
    pub time_on: TrackedState<si::Time>,
}

#[pyo3_api]
impl FuelConverterState {}
impl SerdeAPI for FuelConverterState {}
impl Init for FuelConverterState {}

/// Options for handling [FuelConverter] thermal model
#[derive(
    Clone, Default, Debug, Serialize, Deserialize, PartialEq, IsVariant, derive_more::From, TryInto,
)]
pub enum FuelConverterThermalOption {
    /// Basic thermal plant for [FuelConverter]
    FuelConverterThermal(Box<FuelConverterThermal>),
    /// no thermal plant for [FuelConverter]
    #[default]
    None,
}

impl StateMethods for FuelConverterThermalOption {}

impl SaveState for FuelConverterThermalOption {
    fn save_state<F: Fn() -> String>(&mut self, loc: F) -> anyhow::Result<()> {
        match self {
            Self::FuelConverterThermal(fct) => fct.save_state(loc)?,
            Self::None => {}
        }
        Ok(())
    }
}
impl TrackedStateMethods for FuelConverterThermalOption {
    fn check_and_reset<F: Fn() -> String>(&mut self, loc: F) -> anyhow::Result<()> {
        match self {
            Self::FuelConverterThermal(fct) => {
                fct.check_and_reset(|| format!("{}\n{}", loc(), format_dbg!()))?
            }
            Self::None => {}
        }
        Ok(())
    }

    fn mark_fresh<F: Fn() -> String>(&mut self, loc: F) -> anyhow::Result<()> {
        match self {
            Self::FuelConverterThermal(fct) => {
                fct.mark_fresh(|| format!("{}\n{}", loc(), format_dbg!()))?
            }
            Self::None => {}
        }
        Ok(())
    }
}
impl Step for FuelConverterThermalOption {
    fn step<F: Fn() -> String>(&mut self, loc: F) -> anyhow::Result<()> {
        match self {
            Self::FuelConverterThermal(fct) => fct.step(|| format!("{}\n{}", loc(), format_dbg!())),
            Self::None => Ok(()),
        }
    }

    fn reset_step<F: Fn() -> String>(&mut self, loc: F) -> anyhow::Result<()> {
        match self {
            Self::FuelConverterThermal(fct) => {
                fct.reset_step(|| format!("{}\n{}", loc(), format_dbg!()))
            }
            Self::None => Ok(()),
        }
    }
}
impl Init for FuelConverterThermalOption {
    fn init(&mut self) -> Result<(), Error> {
        match self {
            Self::FuelConverterThermal(fct) => fct.init()?,
            Self::None => {}
        }
        Ok(())
    }
}
impl SerdeAPI for FuelConverterThermalOption {}
impl SetCumulative for FuelConverterThermalOption {
    fn set_cumulative<F: Fn() -> String>(&mut self, dt: si::Time, loc: F) -> anyhow::Result<()> {
        match self {
            Self::FuelConverterThermal(fct) => {
                fct.set_cumulative(dt, || format!("{}\n{}", loc(), format_dbg!()))?
            }
            Self::None => {}
        }
        Ok(())
    }

    fn reset_cumulative<F: Fn() -> String>(&mut self, loc: F) -> anyhow::Result<()> {
        match self {
            Self::FuelConverterThermal(fct) => {
                fct.reset_cumulative(|| format!("{}\n{}", loc(), format_dbg!()))?
            }
            Self::None => {}
        }
        Ok(())
    }
}
impl HistoryMethods for FuelConverterThermalOption {
    fn save_interval(&self) -> anyhow::Result<Option<usize>> {
        match self {
            FuelConverterThermalOption::FuelConverterThermal(fct) => fct.save_interval(),
            FuelConverterThermalOption::None => Ok(None),
        }
    }
    fn set_save_interval(&mut self, save_interval: Option<usize>) -> anyhow::Result<()> {
        match self {
            FuelConverterThermalOption::FuelConverterThermal(fct) => {
                fct.set_save_interval(save_interval)
            }
            FuelConverterThermalOption::None => Ok(()),
        }
    }
    fn clear(&mut self) {
        match self {
            FuelConverterThermalOption::FuelConverterThermal(fct) => {
                fct.clear();
            }
            FuelConverterThermalOption::None => {}
        }
    }
}
impl FuelConverterThermalOption {
    /// Solve change in temperature and other thermal effects
    /// # Arguments
    /// - `fc_state`: [FuelConverter] state
    /// - `te_amb`: ambient temperature
    /// - `pwr_thrml_fc_to_cab`: heat demand from [Vehicle::hvac] system -- zero if `None` is passed
    /// - `veh_speed`: current achieved speed
    fn solve_thermal(
        &mut self,
        fc_state: &FuelConverterState,
        te_amb: si::Temperature,
        pwr_thrml_fc_to_cab: Option<si::Power>,
        veh_speed: si::Velocity,
        dt: si::Time,
    ) -> anyhow::Result<()> {
        match self {
            Self::FuelConverterThermal(fct) => fct
                .solve(
                    fc_state,
                    te_amb,
                    pwr_thrml_fc_to_cab.unwrap_or_default(),
                    veh_speed,
                    dt,
                )
                .with_context(|| format_dbg!())?,
            Self::None => {
                ensure!(
                    pwr_thrml_fc_to_cab.is_none(),
                    format_dbg!(
                        "`FuelConverterThermal needs to be configured to provide heat demand`"
                    )
                );
            }
        }
        Ok(())
    }

    /// If appropriately configured, returns temperature-dependent efficiency coefficient
    fn temp_eff_coeff(&self) -> Option<&TrackedState<si::Ratio>> {
        match self {
            Self::FuelConverterThermal(fct) => Some(&fct.state.eff_coeff),
            Self::None => None,
        }
    }
}

#[serde_api]
#[derive(Deserialize, Serialize, Debug, Clone, PartialEq, StateMethods)]
#[cfg_attr(feature = "pyo3", pyclass(module = "fastsim", subclass, eq))]
#[non_exhaustive]
#[serde(deny_unknown_fields)]
/// Struct for modeling Fuel Converter (e.g. engine, fuel cell.)
pub struct FuelConverterThermal {
    /// [FuelConverter] thermal capacitance
    pub heat_capacitance: si::HeatCapacity,
    /// parameter for engine characteristic length for heat transfer calcs
    pub length_for_convection: si::Length,
    /// parameter for heat transfer coeff from [FuelConverter] to ambient during vehicle stop
    pub htc_to_amb_stop: si::HeatTransferCoeff,

    /// Heat transfer coefficient between adiabatic flame temperature and [FuelConverterThermal] temperature
    pub conductance_from_comb: si::ThermalConductance,
    /// Max coefficient for fraction of combustion heat that goes to [FuelConverter]
    /// (engine) thermal mass. Remainder goes to environment (e.g. via tailpipe).
    pub max_frac_from_comb: si::Ratio,
    /// parameter for temperature at which thermostat starts to open
    pub tstat_te_sto: Option<si::Temperature>,
    /// temperature delta over which thermostat is partially open
    pub tstat_te_delta: Option<si::TemperatureInterval>,
    #[serde(default = "tstat_interp_default", serialize_with = "serialize_nested")]
    pub tstat_interp: Interp1D<f64, strategy::Linear>,
    /// Radiator effectiveness -- ratio of active heat rejection from
    /// radiator to passive heat rejection, always greater than 1
    pub radiator_effectiveness: si::Ratio,
    /// Model for [FuelConverter] dependence on efficiency
    pub fc_eff_model: FCTempEffModel,
    /// struct for tracking current state
    #[serde(default)]
    pub state: FuelConverterThermalState,
    /// Custom vector of [Self::state]
    #[serde(
        default,
        skip_serializing_if = "FuelConverterThermalStateHistoryVec::is_empty"
    )]
    pub history: FuelConverterThermalStateHistoryVec,
    pub save_interval: Option<usize>,
}

#[pyo3_api]
impl FuelConverterThermal {
    #[staticmethod]
    #[pyo3(name = "default")]
    fn default_py() -> Self {
        Default::default()
    }
}

impl FuelConverterThermal {
    pub fn new(
        heat_capacitance: si::HeatCapacity,
        length_for_convection: si::Length,
        htc_to_amb_stop: si::HeatTransferCoeff,
        conductance_from_comb: si::ThermalConductance,
        max_frac_from_comb: si::Ratio,
        tstat_te_sto: Option<si::Temperature>,
        tstat_te_delta: Option<si::TemperatureInterval>,
        tstat_interp: Interp1D<f64, strategy::Linear>,
        radiator_effectiveness: si::Ratio,
        fc_eff_model: FCTempEffModel,
        save_interval: Option<usize>,
    ) -> anyhow::Result<Self> {
        let mut fc_thermal = Self {
            heat_capacitance,
            length_for_convection,
            htc_to_amb_stop,
            conductance_from_comb,
            max_frac_from_comb,
            tstat_te_sto,
            tstat_te_delta,
            tstat_interp,
            radiator_effectiveness,
            fc_eff_model,
            state: FuelConverterThermalState::default(),
            history: FuelConverterThermalStateHistoryVec::default(),
            save_interval,
        };
        fc_thermal.init()?;
        Ok(fc_thermal)
    }
}

impl HistoryMethods for FuelConverterThermal {
    fn save_interval(&self) -> anyhow::Result<Option<usize>> {
        Ok(self.save_interval)
    }
    fn set_save_interval(&mut self, save_interval: Option<usize>) -> anyhow::Result<()> {
        self.save_interval = save_interval;
        Ok(())
    }
    fn clear(&mut self) {
        self.history.clear();
    }
}

/// Dummy interpolator that will be overridden in [FuelConverterThermal::init]
fn tstat_interp_default() -> Interp1D<f64, strategy::Linear> {
    Interp1D::new(
        array![85.0, 90.0],
        array![0.0, 1.0],
        strategy::Linear,
        Extrapolate::Clamp,
    )
    .unwrap()
}

lazy_static! {
    /// gasoline stoichiometric air-fuel ratio https://en.wikipedia.org/wiki/Air%E2%80%93fuel_ratio
    pub static ref AFR_STOICH_GASOLINE: si::Ratio = uc::R * 14.7;
    /// gasoline density in https://inchem.org/documents/icsc/icsc/eics1400.htm
    /// This is reasonably constant with respect to temperature and pressure
    pub static ref GASOLINE_DENSITY: si::MassDensity = 0.75 * uc::KG / uc::L;
    /// TODO: find a source for this value
    pub static ref GASOLINE_LHV: si::SpecificEnergy = 33.7 * uc::KWH / uc::GALLON / *GASOLINE_DENSITY;
    pub static ref TE_ADIABATIC_STD: si::Temperature = Air::get_te_from_u(
            Air::get_specific_energy(*TE_STD_AIR).with_context(|| format_dbg!()).unwrap()
                + (Octane::get_specific_energy(*TE_STD_AIR).with_context(|| format_dbg!()).unwrap()
                    + *GASOLINE_LHV)
                    / *AFR_STOICH_GASOLINE,
        )
        .with_context(|| format_dbg!()).unwrap_or_else(|_| panic!("{}\nFailed to calculate adiabatic flame temp for gasoline", format_dbg!()));
}

impl FuelConverterThermal {
    /// Solve change in temperature and other thermal effects
    /// # Arguments
    /// - `fc_state`: [FuelConverter] state
    /// - `te_amb`: ambient temperature
    /// - `pwr_thrml_fc_to_cab`: heat demand from [Vehicle::hvac] system
    /// - `veh_speed`: current achieved speed
    /// - `dt`: simulation time step size
    fn solve(
        &mut self,
        fc_state: &FuelConverterState,
        te_amb: si::Temperature,
        pwr_thrml_fc_to_cab: si::Power,
        veh_speed: si::Velocity,
        dt: si::Time,
    ) -> anyhow::Result<()> {
        self.state
            .pwr_thrml_fc_to_cab
            .update(pwr_thrml_fc_to_cab, || format_dbg!())?;
        // film temperature for external convection calculations
        let te_air_film: si::Temperature = 0.5
            * (self
                .state
                .temperature
                .get_stale(|| format_dbg!())?
                .get::<si::kelvin_abs>()
                + te_amb.get::<si::kelvin_abs>())
            * uc::KELVIN;
        // Reynolds number = density * speed * diameter / dynamic viscosity
        // NOTE: might be good to pipe in elevation
        let fc_air_film_re =
            Air::get_density(Some(te_air_film), None) * veh_speed * self.length_for_convection
                / Air::get_dyn_visc(te_air_film).with_context(|| format_dbg!())?;

        // calculate heat transfer coeff. from engine to ambient [W / (m ** 2 * K)]
        self.state.htc_to_amb.update(
            if veh_speed < 1.0 * uc::MPS {
                // if stopped, scale based on thermostat opening and constant convection
                self.state.tstat_open_frac.update(
                    self.tstat_interp
                        .interpolate(&[self
                            .state
                            .temperature
                            .get_stale(|| format_dbg!())?
                            .get::<si::degree_celsius>()])
                        .with_context(|| format_dbg!())?,
                    || format_dbg!(),
                )?;
                (uc::R
                    + *self.state.tstat_open_frac.get_fresh(|| format_dbg!())?
                        * self.radiator_effectiveness)
                    * self.htc_to_amb_stop
            } else {
                // Calculate heat transfer coefficient for sphere,
                // from Incropera's Intro to Heat Transfer, 5th Ed., eq. 7.44
                let sphere_conv_params = get_sphere_conv_params(fc_air_film_re.get::<si::ratio>());
                let htc_to_amb_sphere: si::HeatTransferCoeff = sphere_conv_params.0
                    * fc_air_film_re.get::<si::ratio>().powf(sphere_conv_params.1)
                    * Air::get_pr(te_air_film)
                        .with_context(|| format_dbg!())?
                        .get::<si::ratio>()
                        .powf(1.0 / 3.0)
                    * Air::get_therm_cond(te_air_film).with_context(|| format_dbg!())?
                    / self.length_for_convection;
                // if stopped, scale based on thermostat opening and constant convection
                self.state.tstat_open_frac.update(
                    self.tstat_interp
                        .interpolate(&[self
                            .state
                            .temperature
                            .get_stale(|| format_dbg!())?
                            .get::<si::degree_celsius>()])
                        .with_context(|| format_dbg!())?,
                    || format_dbg!(),
                )?;
                *self.state.tstat_open_frac.get_fresh(|| format_dbg!())? * htc_to_amb_sphere
            },
            || format_dbg!(),
        )?;

        self.state.pwr_thrml_to_amb.update(
            *self.state.htc_to_amb.get_fresh(|| format_dbg!())?
                * PI
                * self.length_for_convection.powi(P2::new())
                / 4.0
                * (self
                    .state
                    .temperature
                    .get_stale(|| format_dbg!())?
                    .get::<si::degree_celsius>()
                    - te_amb.get::<si::degree_celsius>())
                * uc::KELVIN_INT,
            || format_dbg!(),
        )?;

        // let heat_to_amb = ;
        // assumes fuel/air mixture is entering combustion chamber at block temperature
        // assumes stoichiometric combustion
        self.state.te_adiabatic.update(
            Air::get_te_from_u(
                Air::get_specific_energy(*self.state.temperature.get_stale(|| format_dbg!())?)
                    .with_context(|| format_dbg!())?
                    + (Octane::get_specific_energy(*self.state.temperature.get_stale(|| format_dbg!())?)
                    .with_context(|| format_dbg!())?
                    // TODO: make config. for other fuels -- e.g. with enum for specific fuels and/or fuel properties
                    + *GASOLINE_LHV)
                        / *AFR_STOICH_GASOLINE,
            )
            .with_context(|| format_dbg!())?,
            || format_dbg!(),
        )?;
        // heat that will go both to the block and out the exhaust port
        self.state.pwr_fuel_as_heat.update(
            *fc_state.pwr_fuel.get_stale(|| format_dbg!())?
                - (*fc_state.pwr_prop.get_stale(|| format_dbg!())?
                    + *fc_state.pwr_aux.get_stale(|| format_dbg!())?),
            || format_dbg!(),
        )?;
        self.state.pwr_thrml_to_tm.update(
            (self.conductance_from_comb
                * (self
                    .state
                    .te_adiabatic
                    .get_fresh(|| format_dbg!())?
                    .get::<si::degree_celsius>()
                    - self
                        .state
                        .temperature
                        .get_stale(|| format_dbg!())?
                        .get::<si::degree_celsius>())
                * uc::KELVIN_INT)
                .min(
                    self.max_frac_from_comb
                        * *self.state.pwr_fuel_as_heat.get_fresh(|| format_dbg!())?,
                ),
            || format_dbg!(),
        )?;
        let delta_temp: si::TemperatureInterval =
            ((*self.state.pwr_thrml_to_tm.get_fresh(|| format_dbg!())?
                - *self.state.pwr_thrml_fc_to_cab.get_fresh(|| format_dbg!())?
                - *self.state.pwr_thrml_to_amb.get_fresh(|| format_dbg!())?)
                * dt)
                / self.heat_capacitance;
        // Interestingly, it seems to be ok to add a `TemperatureInterval` to a `Temperature` here
        self.state.temperature.update(
            *self.state.temperature.get_stale(|| format_dbg!())? + delta_temp,
            || format_dbg!(),
        )?;

        self.state.eff_coeff.update(
            match self.fc_eff_model {
                FCTempEffModel::Linear(FCTempEffModelLinear {
                    offset,
                    slope_per_kelvin: slope,
                    minimum,
                }) => minimum.max(
                    {
                        let calc_unbound: si::Ratio = offset
                            + slope * uc::R / uc::KELVIN
                                * *self.state.temperature.get_fresh(|| format_dbg!())?;
                        calc_unbound
                    }
                    .min(1.0 * uc::R),
                ),
                FCTempEffModel::Exponential(FCTempEffModelExponential {
                    offset,
                    lag,
                    minimum,
                }) => {
                    let dte: si::TemperatureInterval = (self
                        .state
                        .temperature
                        .get_fresh(|| format_dbg!())?
                        .get::<si::kelvin_abs>()
                        - offset.get::<si::kelvin_abs>())
                        * uc::KELVIN_INT;
                    ((1.0 - f64::exp((-dte / lag).get::<si::ratio>())) * uc::R).max(minimum)
                }
            },
            || format_dbg!(),
        )?;
        Ok(())
    }
}
impl SerdeAPI for FuelConverterThermal {}
impl SetCumulative for FuelConverterThermal {
    fn set_cumulative<F: Fn() -> String>(&mut self, dt: si::Time, loc: F) -> anyhow::Result<()> {
        self.state
            .set_cumulative(dt, || format!("{}\n{}", loc(), format_dbg!()))
    }

    fn reset_cumulative<F: Fn() -> String>(&mut self, loc: F) -> anyhow::Result<()> {
        self.state
            .reset_cumulative(|| format!("{}\n{}", loc(), format_dbg!()))
    }
}
impl Init for FuelConverterThermal {
    fn init(&mut self) -> Result<(), Error> {
        self.tstat_te_sto = self
            .tstat_te_sto
            .or(Some((85. + uc::CELSIUS_TO_KELVIN) * uc::KELVIN));
        self.tstat_te_delta = self.tstat_te_delta.or(Some(5. * uc::KELVIN_INT));
        self.tstat_interp = Interp1D::new(
            array![
                self.tstat_te_sto.unwrap().get::<si::degree_celsius>(),
                self.tstat_te_sto.unwrap().get::<si::degree_celsius>()
                    + self.tstat_te_delta.unwrap().get::<si::kelvin>(),
            ],
            array![0.0, 1.0],
            strategy::Linear,
            Extrapolate::Clamp,
        )
        .map_err(|err| {
            Error::InitError(format!(
                "{}\n{}\n{}",
                err,
                format_dbg!(self.tstat_te_sto),
                format_dbg!(self.tstat_te_delta)
            ))
        })?;
        Ok(())
    }
}
impl Default for FuelConverterThermal {
    fn default() -> Self {
        let mut fct = Self {
            heat_capacitance: Default::default(),
            length_for_convection: Default::default(),
            htc_to_amb_stop: Default::default(),
            conductance_from_comb: Default::default(),
            max_frac_from_comb: Default::default(),
            tstat_te_sto: None,
            tstat_te_delta: None,
            tstat_interp: tstat_interp_default(),
            radiator_effectiveness: Default::default(),
            fc_eff_model: Default::default(),
            state: Default::default(),
            history: Default::default(),
            save_interval: Some(1),
        };
        fct.init().unwrap();
        fct
    }
}

#[serde_api]
#[derive(
    Clone, Debug, Deserialize, Serialize, PartialEq, HistoryVec, StateMethods, SetCumulative,
)]
#[serde(default)]
#[cfg_attr(feature = "pyo3", pyclass(module = "fastsim", subclass, eq))]
#[serde(deny_unknown_fields)]
pub struct FuelConverterThermalState {
    /// time step index
    pub i: TrackedState<usize>,
    /// Adiabatic flame temperature assuming complete (i.e. all fuel is consumed
    /// if fuel lean or stoich or all air is consumed if fuel rich) combustion
    pub te_adiabatic: TrackedState<si::Temperature>,
    /// Current engine thermal mass temperature (lumped engine block and coolant)
    pub temperature: TrackedState<si::Temperature>,
    /// thermostat open fraction (1 = fully open, 0 = fully closed)
    pub tstat_open_frac: TrackedState<f64>,
    /// Current heat transfer coefficient from [FuelConverter] to ambient
    pub htc_to_amb: TrackedState<si::HeatTransferCoeff>,
    /// Current heat transfer power to ambient
    pub pwr_thrml_to_amb: TrackedState<si::Power>,
    /// Cumulative heat transfer energy to ambient
    pub energy_thrml_to_amb: TrackedState<si::Energy>,
    /// Efficency coefficient, used to modify [FuelConverter] effciency based on temperature
    pub eff_coeff: TrackedState<si::Ratio>,
    /// Thermal power flowing from fuel converter to cabin
    pub pwr_thrml_fc_to_cab: TrackedState<si::Power>,
    /// Cumulative thermal energy flowing from fuel converter to cabin
    pub energy_thrml_fc_to_cab: TrackedState<si::Energy>,
    /// Fuel power that is not converted to mechanical work
    pub pwr_fuel_as_heat: TrackedState<si::Power>,
    /// Cumulative fuel energy that is not converted to mechanical work
    pub energy_fuel_as_heat: TrackedState<si::Energy>,
    /// Thermal power flowing from combustion to [FuelConverter] thermal mass
    pub pwr_thrml_to_tm: TrackedState<si::Power>,
    /// Cumulative thermal energy flowing from combustion to [FuelConverter] thermal mass
    pub energy_thrml_to_tm: TrackedState<si::Energy>,
}
#[pyo3_api]
impl FuelConverterThermalState {}

impl Init for FuelConverterThermalState {}
impl SerdeAPI for FuelConverterThermalState {}
impl Default for FuelConverterThermalState {
    fn default() -> Self {
        Self {
            i: Default::default(),
            te_adiabatic: TrackedState::new(*TE_ADIABATIC_STD),
            temperature: TrackedState::new(*TE_STD_AIR),
            tstat_open_frac: Default::default(),
            htc_to_amb: Default::default(),
            eff_coeff: TrackedState::new(uc::R),
            pwr_thrml_fc_to_cab: Default::default(),
            energy_thrml_fc_to_cab: Default::default(),
            pwr_thrml_to_amb: Default::default(),
            energy_thrml_to_amb: Default::default(),
            pwr_fuel_as_heat: Default::default(),
            energy_fuel_as_heat: Default::default(),
            pwr_thrml_to_tm: Default::default(),
            energy_thrml_to_tm: Default::default(),
        }
    }
}

/// Model variants for how FC efficiency depends on temperature
#[derive(
    Debug, Clone, Deserialize, Serialize, PartialEq, IsVariant, derive_more::From, TryInto,
)]
pub enum FCTempEffModel {
    /// Linear temperature dependence
    Linear(FCTempEffModelLinear),
    /// Exponential temperature dependence
    Exponential(FCTempEffModelExponential),
}

impl Default for FCTempEffModel {
    fn default() -> Self {
        FCTempEffModel::Exponential(FCTempEffModelExponential::default())
    }
}

#[derive(Debug, Clone, Deserialize, Serialize, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct FCTempEffModelLinear {
    pub offset: si::Ratio,
    /// Change in efficiency factor per change in temperature /[K/]
    pub slope_per_kelvin: f64,
    pub minimum: si::Ratio,
}

impl FCTempEffModelLinear {
    pub fn new(
        offset: si::Ratio,
        slope_per_kelvin: f64,
        minimum: si::Ratio,
    ) -> anyhow::Result<Self> {
        Ok(Self {
            offset,
            slope_per_kelvin,
            minimum,
        })
    }
}

impl Default for FCTempEffModelLinear {
    fn default() -> Self {
        Self {
            offset: 0.0 * uc::R,
            slope_per_kelvin: 25.0,
            minimum: 0.2 * uc::R,
        }
    }
}

#[derive(Debug, Clone, Deserialize, Serialize, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct FCTempEffModelExponential {
    /// temperature at which `fc_eta_temp_coeff` begins to grow
    pub offset: si::Temperature,
    /// exponential lag parameter [K^-1]
    pub lag: si::TemperatureInterval,
    /// minimum value that `fc_eta_temp_coeff` can take
    pub minimum: si::Ratio,
}

impl FCTempEffModelExponential {
    pub fn new(
        offset: si::Temperature,
        lag: si::TemperatureInterval,
        minimum: si::Ratio,
    ) -> anyhow::Result<Self> {
        Ok(Self {
            offset,
            lag,
            minimum,
        })
    }
}

impl Default for FCTempEffModelExponential {
    fn default() -> Self {
        Self {
            // TODO: update after reasonable calibration
            offset: 0.0 * uc::KELVIN,
            lag: 25.0 * uc::KELVIN_INT,
            minimum: 0.2 * uc::R,
        }
    }
}

#[cfg(test)]
mod tests {
    use approx::assert_abs_diff_eq;

    use super::*;

    struct FuelConverterAndResult {
        fc: FuelConverter,
        result: anyhow::Result<()>,
    }

    // TODO: add ability to access vehicle state from FuelConverter
    // -- perhaps an optional read-only reference to Veh?
    const EFF_AT_000_PERCENT_PWR: f64 = 0.30;
    const EFF_AT_080_PERCENT_PWR: f64 = 0.35;
    const EFF_AT_100_PERCENT_PWR: f64 = 0.31;
    const PEAK_POWER_KW: f64 = 50.0;

    fn create_test_fuel_converter(
        aux_pwr: si::Power,
        idle_pwr: si::Power,
        is_on: bool,
    ) -> FuelConverterAndResult {
        let peak_pwr = PEAK_POWER_KW * uc::KW;
        let eff_interp_pwr_out_fraction = vec![0.0, 0.8, 1.0];
        let eff_interp_eff_out = vec![
            EFF_AT_000_PERCENT_PWR,
            EFF_AT_080_PERCENT_PWR,
            EFF_AT_100_PERCENT_PWR,
        ];
        // NOTE: the below documents which fields at minimum must be marked fresh when coming into the
        // FuelConverter::solve() method. Possibly, more fields would be required if using more options.
        let mut fc_state = FuelConverterState::default();
        fc_state.i.mark_stale();
        let res_i_update = fc_state.i.update(1, || format_dbg!());
        assert!(res_i_update.is_ok());
        fc_state.pwr_out_max.mark_stale();
        fc_state.pwr_prop_max.mark_stale();
        let res_pwr_prop_max_update = fc_state.pwr_prop_max.update(0.0 * uc::KW, || format_dbg!());
        assert!(res_pwr_prop_max_update.is_ok());
        fc_state.eff.mark_stale();
        fc_state.pwr_prop.mark_stale();
        fc_state.energy_prop.mark_stale();
        fc_state.pwr_aux.mark_stale();
        let res_pwr_aux_update = fc_state.pwr_aux.update(aux_pwr, || format_dbg!());
        assert!(res_pwr_aux_update.is_ok());
        fc_state.energy_aux.mark_stale();
        fc_state.pwr_fuel.mark_stale();
        fc_state.energy_fuel.mark_stale();
        fc_state.pwr_loss.mark_stale();
        fc_state.energy_loss.mark_stale();
        fc_state.fc_on.mark_stale();
        fc_state.time_on.mark_stale();
        let mut fc = FuelConverter {
            thrml: FuelConverterThermalOption::None,
            mass: None,
            specific_pwr: None,
            pwr_out_max: peak_pwr,
            pwr_out_max_init: 5.0 * uc::KW,
            pwr_ramp_lag: 5.0 * uc::S,
            eff_interp_from_pwr_out: InterpolatorEnum::new_1d(
                eff_interp_pwr_out_fraction.into(),
                eff_interp_eff_out.into(),
                strategy::Linear,
                Extrapolate::Error,
            )
            .unwrap(),
            pwr_for_peak_eff: peak_pwr * 0.8,
            pwr_idle_fuel: idle_pwr,
            state: fc_state,
            history: FuelConverterStateHistoryVec::default(),
            save_interval: None,
        };
        let init_result = fc.init();
        assert!(init_result.is_ok());
        let pwr_out_req = 0.0 * uc::KW;
        let fc_on = is_on;
        let dt = 1.0 * uc::S;
        let solve_result = fc.solve(pwr_out_req, fc_on, dt);
        FuelConverterAndResult {
            fc,
            result: solve_result,
        }
    }

    #[test]
    fn calling_solve_with_aux_load_and_fc_on() {
        let peak_pwr = PEAK_POWER_KW * uc::KW;
        let aux_pwr = 2.0 * uc::KW;
        let idle_pwr = 1.0 * uc::KW;
        let fc_is_on = true;
        let fc_and_res = create_test_fuel_converter(aux_pwr, idle_pwr, fc_is_on);
        assert!(fc_and_res.result.is_ok());
        let fc = fc_and_res.fc;
        // (eff_at_80_percent_pwr - eff_at_0_percent_pwr) * alpha + eff_at_0_percent_pwr
        // alpha = (aux_pwr - 0) / (peak_pwr * 0.8 - 0)
        let alpha = aux_pwr.value / (peak_pwr.value * 0.8);
        let expected_eff =
            (EFF_AT_080_PERCENT_PWR - EFF_AT_000_PERCENT_PWR) * alpha + EFF_AT_000_PERCENT_PWR;
        let expected_fuel_in = aux_pwr.value / expected_eff;
        let actual_fuel_in_result = fc.state.pwr_fuel.get_fresh(|| format_dbg!());
        assert!(actual_fuel_in_result.is_ok());
        let actual_fuel_in = actual_fuel_in_result.unwrap().value;
        assert_abs_diff_eq!(actual_fuel_in, expected_fuel_in);
        let fc_on_result = fc.state.fc_on.get_fresh(|| format_dbg!());
        assert!(fc_on_result.is_ok());
        let fc_on = *fc_on_result.unwrap();
        assert_eq!(fc_on, fc_is_on);
    }

    #[test]
    fn calling_solve_with_no_aux_load_but_fc_on_causes_idle_fuel_use() {
        let aux_pwr = 0.0 * uc::KW;
        let idle_pwr = 1.0 * uc::KW;
        let fc_is_on = true;
        let fc_and_res = create_test_fuel_converter(aux_pwr, idle_pwr, fc_is_on);
        assert!(fc_and_res.result.is_ok());
        let fc = fc_and_res.fc;
        let expected_fuel_in = idle_pwr.value;
        let actual_fuel_in_result = fc.state.pwr_fuel.get_fresh(|| format_dbg!());
        assert!(actual_fuel_in_result.is_ok());
        let actual_fuel_in = actual_fuel_in_result.unwrap().value;
        assert_abs_diff_eq!(actual_fuel_in, expected_fuel_in);
        let fc_on_result = fc.state.fc_on.get_fresh(|| format_dbg!());
        assert!(fc_on_result.is_ok());
        let fc_on = *fc_on_result.unwrap();
        assert_eq!(fc_on, fc_is_on);
    }

    #[test]
    fn calling_solve_with_engine_off_and_no_aux_load_results_in_no_fuel_use() {
        let aux_pwr = 0.0 * uc::KW;
        let idle_pwr = 1.0 * uc::KW;
        let fc_is_on = false;
        let fc_and_res = create_test_fuel_converter(aux_pwr, idle_pwr, fc_is_on);
        assert!(fc_and_res.result.is_ok());
        let fc = fc_and_res.fc;
        let expected_fuel_in = 0.0;
        let actual_fuel_in_result = fc.state.pwr_fuel.get_fresh(|| format_dbg!());
        assert!(actual_fuel_in_result.is_ok());
        let actual_fuel_in = actual_fuel_in_result.unwrap().value;
        assert_abs_diff_eq!(actual_fuel_in, expected_fuel_in);
        let fc_on_result = fc.state.fc_on.get_fresh(|| format_dbg!());
        assert!(fc_on_result.is_ok());
        let fc_on = *fc_on_result.unwrap();
        assert_eq!(fc_on, fc_is_on);
    }

    #[test]
    fn calling_solve_with_engine_off_and_postive_aux_load_is_error() {
        let aux_pwr = 2.0 * uc::KW;
        let idle_pwr = 1.0 * uc::KW;
        let fc_is_on = false;
        let fc_and_res = create_test_fuel_converter(aux_pwr, idle_pwr, fc_is_on);
        assert!(fc_and_res.result.is_err());
    }
}
