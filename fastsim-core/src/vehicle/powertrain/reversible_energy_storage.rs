use super::{utils::ScalingMethods, *};
use crate::utils::interp::*;

#[allow(unused_imports)]
#[cfg(feature = "pyo3")]
use crate::pyo3::*;

const TOL: f64 = 1e-3;

#[serde_api]
#[derive(Deserialize, Serialize, Debug, Clone, PartialEq, StateMethods, SetCumulative)]
#[non_exhaustive]
#[serde(deny_unknown_fields)]
#[cfg_attr(feature = "pyo3", pyclass(module = "fastsim", subclass, eq))]
/// Struct for modeling technology-naive Reversible Energy Storage (e.g. battery, flywheel).
pub struct ReversibleEnergyStorage {
    /// [Self] Thermal plant, including thermal management controls
    #[has_state]
    #[serde(default)]
    pub thrml: RESThermalOption,
    /// ReversibleEnergyStorage mass
    #[serde(default)]
    pub(crate) mass: Option<si::Mass>,
    /// ReversibleEnergyStorage specific energy
    pub(crate) specific_energy: Option<si::SpecificEnergy>,
    /// Max output (and input) power battery can produce (accept)
    pub pwr_out_max: si::Power,

    /// Total energy capacity of battery of full discharge SOC of 0.0 and 1.0
    pub energy_capacity: si::Energy,

    /// interpolator for calculating [Self] efficiency
    pub eff_interp: RESEfficiency,

    /// Hard limit on minimum SOC, e.g. 0.05
    pub min_soc: si::Ratio,
    /// Hard limit on maximum SOC, e.g. 0.95
    pub max_soc: si::Ratio,
    /// struct for tracking current state
    #[serde(default)]
    #[is_state]
    pub state: ReversibleEnergyStorageState,
    /// Custom vector of [Self::state]
    #[serde(
        default,
        skip_serializing_if = "ReversibleEnergyStorageStateHistoryVec::is_empty"
    )]
    pub history: ReversibleEnergyStorageStateHistoryVec,
    /// Time step interval at which history is saved
    pub save_interval: Option<usize>,
}

#[pyo3_api]
impl ReversibleEnergyStorage {
    // #[getter("eff_max")]
    // fn get_eff_max_py(&self) -> f64 {
    //     self.get_eff_max()
    // }

    // #[setter("__eff_max")]
    // fn set_eff_max_py(&mut self, eff_max: f64) -> PyResult<()> {
    //     self.set_eff_max(eff_max).map_err(PyValueError::new_err)
    // }

    // #[getter("eff_min")]
    // fn get_eff_min_py(&self) -> f64 {
    //     self.get_eff_min()
    // }

    // #[getter("eff_range")]
    // fn get_eff_range_py(&self) -> f64 {
    //     self.get_eff_range()
    // }

    // #[setter("__eff_range")]
    // fn set_eff_range_py(&mut self, eff_range: f64) -> anyhow::Result<()> {
    //     self.set_eff_range(eff_range)
    // }

    // TODO: decide on way to deal with `side_effect` coming after optional arg and uncomment
    #[pyo3(name = "set_mass")]
    #[pyo3(signature = (mass_kg=None, side_effect=None))]
    fn set_mass_py(
        &mut self,
        mass_kg: Option<f64>,
        side_effect: Option<String>,
    ) -> anyhow::Result<()> {
        let side_effect = side_effect.unwrap_or_else(|| "Intensive".into());
        self.set_mass(
            mass_kg.map(|m| m * uc::KG),
            MassSideEffect::try_from(side_effect)?,
        )?;
        Ok(())
    }

    #[getter("mass_kg")]
    fn get_mass_kg_py(&mut self) -> anyhow::Result<Option<f64>> {
        Ok(self.mass()?.map(|m| m.get::<si::kilogram>()))
    }

    #[getter]
    fn get_specific_energy_kjoules_per_kg(&self) -> Option<f64> {
        self.specific_energy
            .map(|se| se.get::<si::kilojoule_per_kilogram>())
    }

    #[getter]
    fn get_energy_capacity_usable_joules(&self) -> f64 {
        self.energy_capacity_usable().get::<si::joule>()
    }

    #[pyo3(name = "set_default_pwr_interp")]
    #[cfg(all(feature = "yaml", feature = "resources"))]
    fn set_default_pwr_interp_py(&mut self) -> anyhow::Result<()> {
        self.set_default_pwr_interp()
    }

    #[pyo3(name = "set_default_pwr_and_soc_interp")]
    #[cfg(all(feature = "yaml", feature = "resources"))]
    fn set_default_pwr_and_soc_interp_py(&mut self) -> anyhow::Result<()> {
        self.set_default_pwr_and_soc_interp()
    }

    #[pyo3(name = "set_default_pwr_and_temp_interp")]
    #[cfg(all(feature = "yaml", feature = "resources"))]
    fn set_default_pwr_and_temp_interp_py(&mut self) -> anyhow::Result<()> {
        self.set_default_pwr_and_temp_interp()
    }

    #[pyo3(name = "set_default_pwr_soc_and_temp_interp")]
    #[cfg(all(feature = "yaml", feature = "resources"))]
    fn set_default_pwr_soc_and_temp_interp_py(&mut self) -> anyhow::Result<()> {
        self.set_default_pwr_soc_and_temp_interp()
    }
}

impl ReversibleEnergyStorage {
    /// Constructor for ReversibleEnergyStorage
    pub fn new(
        thrml: RESThermalOption,
        mass: Option<si::Mass>,
        specific_energy: Option<si::SpecificEnergy>,
        pwr_out_max: si::Power,
        energy_capacity: si::Energy,
        eff_interp: RESEfficiency,
        min_soc: si::Ratio,
        max_soc: si::Ratio,
        save_interval: Option<usize>,
    ) -> anyhow::Result<Self> {
        let mut reversible_energy_storage = Self {
            thrml,
            mass,
            specific_energy,
            pwr_out_max,
            energy_capacity,
            eff_interp,
            min_soc,
            max_soc,
            state: ReversibleEnergyStorageState::default(),
            history: ReversibleEnergyStorageStateHistoryVec::default(),
            save_interval,
        };
        reversible_energy_storage.init()?;
        Ok(reversible_energy_storage)
    }
}

impl ReversibleEnergyStorage {
    pub fn solve(&mut self, pwr_out_req: si::Power, dt: si::Time) -> anyhow::Result<()> {
        let te_res: Option<si::Temperature> = self.temperature()?;
        let state = &mut self.state;

        ensure!(
            *state.soc.get_stale(|| format_dbg!())? <= self.max_soc,
            format_dbg!(state.soc.get_stale(|| format_dbg!())?.get::<si::ratio>())
        );
        ensure!(
            almost_ge_uom(
                state.soc.get_stale(|| format_dbg!())?,
                &self.min_soc,
                Some(1e-3)
            ),
            "{}\n{}\n{}",
            format_dbg!(state.soc.get_stale(|| format_dbg!())?.get::<si::ratio>()),
            format_dbg!(state
                .soc_disch_buffer
                .get_fresh(|| format_dbg!())?
                .get::<si::ratio>()),
            format_dbg!(state.pwr_aux.get_fresh(|| format_dbg!())?.get::<si::watt>())
        );

        state.pwr_out_prop.update(pwr_out_req, || format_dbg!())?;
        state.pwr_out_electrical.update(
            *state.pwr_out_prop.get_fresh(|| format_dbg!())?
                + *state.pwr_aux.get_fresh(|| format_dbg!())?,
            || format_dbg!(),
        )?;

        if pwr_out_req + *state.pwr_aux.get_fresh(|| format_dbg!())? >= si::Power::ZERO {
            // discharging
            ensure!(
                utils::almost_le_uom(
                    &(pwr_out_req + *state.pwr_aux.get_fresh(|| format_dbg!())?),
                    &self.pwr_out_max,
                    Some(TOL)),
                "{}\nres required power ({:.6} kW) exceeds static max discharge power ({:.6} kW)\nstate.soc = {}",
                format_dbg!(utils::almost_le_uom(
                    &(pwr_out_req + *state.pwr_aux.get_fresh(|| format_dbg!())?),
                    &self.pwr_out_max,
                    Some(TOL)
                )),
                (pwr_out_req + *state.pwr_aux.get_fresh(|| format_dbg!())?).get::<si::kilowatt>(),
                &self.pwr_out_max.get::<si::kilowatt>(),
                state.soc.get_stale(|| format_dbg!())?.get::<si::ratio>()
            );
            ensure!(
                utils::almost_le_uom(
                    &(pwr_out_req + *state.pwr_aux.get_fresh(|| format_dbg!())?),
                    state.pwr_disch_max.get_fresh(|| format_dbg!())?, Some(TOL)
                ),
                "{}\nres required power ({:.6} kW) exceeds current max discharge power ({:.6} kW)\nstate.soc .get_fresh(|| format_dbg!())?= {}",
                format_dbg!(utils::almost_le_uom(
                    &(pwr_out_req + *state.pwr_aux.get_fresh(|| format_dbg!())?),
                    state.pwr_disch_max.get_fresh(|| format_dbg!())?, Some(TOL)
                )),
                (pwr_out_req + *state.pwr_aux.get_fresh(|| format_dbg!())?).get::<si::kilowatt>(),
                state.pwr_disch_max.get_fresh(|| format_dbg!())?.get::<si::kilowatt>(),
                state.soc.get_stale(|| format_dbg!())?.get::<si::ratio>()
            );
        } else {
            // charging
            ensure!(
                utils::almost_ge_uom(
                    &(pwr_out_req + *state.pwr_aux.get_fresh(|| format_dbg!())?),
                    &-self.pwr_out_max,
                    Some(TOL)
                ),
                format!(
                    "{}\nres required power ({:.6} kW) exceeds static max power ({:.6} kW)",
                    format_dbg!(utils::almost_ge_uom(
                        &(pwr_out_req + *state.pwr_aux.get_fresh(|| format_dbg!())?),
                        &-self.pwr_out_max,
                        Some(TOL)
                    )),
                    (pwr_out_req + *state.pwr_aux.get_fresh(|| format_dbg!())?)
                        .get::<si::kilowatt>(),
                    state
                        .pwr_charge_max
                        .get_fresh(|| format_dbg!())?
                        .get::<si::kilowatt>()
                )
            );
            ensure!(
                utils::almost_ge_uom(
                    &(pwr_out_req + *state.pwr_aux.get_fresh(|| format_dbg!())?),
                    &-*state.pwr_charge_max.get_fresh(|| format_dbg!())?,
                    Some(TOL)
                ),
                format!(
                    "{}\nres required power ({:.6} kW) exceeds current max power ({:.6} kW)",
                    format_dbg!(utils::almost_ge_uom(
                        &(pwr_out_req + *state.pwr_aux.get_fresh(|| format_dbg!())?),
                        &-*state.pwr_charge_max.get_fresh(|| format_dbg!())?,
                        Some(TOL)
                    )),
                    (pwr_out_req + *state.pwr_aux.get_fresh(|| format_dbg!())?)
                        .get::<si::kilowatt>(),
                    -state
                        .pwr_charge_max
                        .get_fresh(|| format_dbg!())?
                        .get::<si::kilowatt>()
                )
            );
        }
        let interp_pt: &[f64] = match &self.eff_interp {
            RESEfficiency::Constant(_) => &[],
            RESEfficiency::CRate(_) => &[state
                .pwr_out_electrical
                .get_fresh(|| format_dbg!())?
                .get::<si::watt>()
                / self.energy_capacity.get::<si::watt_hour>()],
            RESEfficiency::CRateSOC(_) => &[
                state
                    .pwr_out_electrical
                    .get_fresh(|| format_dbg!())?
                    .get::<si::watt>()
                    / self.energy_capacity.get::<si::watt_hour>(),
                state.soc.get_stale(|| format_dbg!())?.get::<si::ratio>(),
            ],
            RESEfficiency::CRateTemperature(_) => &[
                state
                    .pwr_out_electrical
                    .get_fresh(|| format_dbg!())?
                    .get::<si::watt>()
                    / self.energy_capacity.get::<si::watt_hour>(),
                te_res
                    .with_context(|| format_dbg!("Expected thermal model to be configured"))?
                    .get::<si::degree_celsius>(),
            ],
            RESEfficiency::CRateSOCTemperature(_) => &[
                state
                    .pwr_out_electrical
                    .get_fresh(|| format_dbg!())?
                    .get::<si::watt>()
                    / self.energy_capacity.get::<si::watt_hour>(),
                state.soc.get_stale(|| format_dbg!())?.get::<si::ratio>(),
                te_res
                    .with_context(|| format_dbg!("Expected thermal model to be configured"))?
                    .get::<si::degree_celsius>(),
            ],
        };
        state.eff.update(
            self.eff_interp.interpolate(interp_pt)? * uc::R,
            || format_dbg!(),
        )?;

        state.pwr_out_chemical.update(
            if *state.pwr_out_electrical.get_fresh(|| format_dbg!())? > si::Power::ZERO {
                // if positive, chemical power must be greater than electrical power
                // i.e. not all chemical power can be converted to electrical power
                *state.pwr_out_electrical.get_fresh(|| format_dbg!())?
                    / *state.eff.get_fresh(|| format_dbg!())?
            } else {
                // if negative, chemical power, must be less than electrical power
                // i.e. not all electrical power can be converted back to chemical power
                *state.pwr_out_electrical.get_fresh(|| format_dbg!())?
                    * *state.eff.get_fresh(|| format_dbg!())?
            },
            || format_dbg!(),
        )?;

        state.pwr_loss.update(
            (*state.pwr_out_chemical.get_fresh(|| format_dbg!())?
                - *state.pwr_out_electrical.get_fresh(|| format_dbg!())?)
            .abs(),
            || format_dbg!(),
        )?;

        state.soc.update(
            *state.soc.get_stale(|| format_dbg!())?
                - *state.pwr_out_chemical.get_fresh(|| format_dbg!())? * dt / self.energy_capacity,
            || format_dbg!(),
        )?;

        Ok(())
    }

    /// Solve change in temperature and other thermal effects
    /// # Arguments
    /// - `fc_state`: [ReversibleEnergyStorage] state
    /// - `te_amb`: ambient temperature
    /// - `pwr_thrml_hvac_to_res`: thermal power flowing from [Vehicle::hvac] system to [Self::thrml]
    /// - `te_cab`: cabin temperature for heat transfer interaction with
    ///   [Self], required if [Self::thrml] is `Some`
    /// - `dt`: simulation time step size
    pub fn solve_thermal(
        &mut self,
        te_amb: si::Temperature,
        pwr_thrml_hvac_to_res: si::Power,
        te_cab: Option<si::Temperature>,
        dt: si::Time,
    ) -> anyhow::Result<()> {
        self.thrml
            .solve(&mut self.state, te_amb, pwr_thrml_hvac_to_res, te_cab, dt)
            .with_context(|| format_dbg!())
    }

    /// Sets and returns max output and max regen power based on current state
    /// # Arguments
    /// - `dt`: simulation time step size
    /// - `disch_buffer`: buffer offset from static SOC limit at which discharging is not allowed
    /// - `chrg_buffer`: buffer offset from static SOC limit at which charging is not allowed
    pub fn set_curr_pwr_out_max(
        &mut self,
        dt: si::Time,
        disch_buffer: si::Energy,
        chrg_buffer: si::Energy,
    ) -> anyhow::Result<()> {
        self.set_pwr_disch_max(dt, disch_buffer)?;
        self.set_pwr_charge_max(dt, chrg_buffer)?;

        Ok(())
    }

    pub fn get_curr_pwr_prop_out_max(&self) -> anyhow::Result<(si::Power, si::Power)> {
        Ok((
            *self.state.pwr_prop_max.get_fresh(|| format_dbg!())?,
            *self.state.pwr_regen_max.get_fresh(|| format_dbg!())?,
        ))
    }

    /// # Arguments
    /// - `dt`: simulation time step size
    /// - `buffer`: buffer below static maximum SOC above which charging is disabled
    pub fn set_pwr_charge_max(
        &mut self,
        dt: si::Time,
        chrg_buffer: si::Energy,
    ) -> anyhow::Result<()> {
        // to protect against excessive topping off of the battery
        let soc_buffer_delta = (chrg_buffer
            / (self.energy_capacity * (self.max_soc - self.min_soc)))
            .max(si::Ratio::ZERO);
        ensure!(soc_buffer_delta >= si::Ratio::ZERO, "{}", format_dbg!());
        self.state
            .soc_regen_buffer
            .update(self.max_soc - soc_buffer_delta, || format_dbg!())?;
        let pwr_max_for_dt = ((self.max_soc - *self.state.soc.get_stale(|| format_dbg!())?)
            * self.energy_capacity
            / dt)
            .max(si::Power::ZERO);
        self.state.pwr_charge_max.update(
            if *self.state.soc.get_stale(|| format_dbg!())?
                <= *self.state.soc_regen_buffer.get_fresh(|| format_dbg!())?
            {
                self.pwr_out_max
            } else if *self.state.soc.get_stale(|| format_dbg!())? < self.max_soc
                && soc_buffer_delta > si::Ratio::ZERO
            {
                self.pwr_out_max * (self.max_soc - *self.state.soc.get_stale(|| format_dbg!())?)
                    / soc_buffer_delta
            } else {
                // current SOC is less than both
                si::Power::ZERO
            }
            .min(pwr_max_for_dt),
            || format_dbg!(),
        )?;

        ensure!(
            *self.state.pwr_charge_max.get_fresh(|| format_dbg!())? >= si::Power::ZERO,
            "{}\n`{}` ({} W) must be greater than or equal to zero\n{}",
            format_dbg!(),
            stringify!(self.state.pwr_charge_max),
            self.state
                .pwr_charge_max
                .get_fresh(|| format_dbg!())?
                .get::<si::watt>()
                .format_eng(None),
            format_dbg!(soc_buffer_delta)
        );

        Ok(())
    }

    /// # Arguments
    /// - `dt`: simulation time step size
    /// - `buffer`: buffer above static minimum SOC above which charging is disabled
    pub fn set_pwr_disch_max(
        &mut self,
        dt: si::Time,
        disch_buffer: si::Energy,
    ) -> anyhow::Result<()> {
        // to protect against excessive bottoming out of the battery
        let soc_buffer_delta = (disch_buffer / self.energy_capacity_usable()).max(si::Ratio::ZERO);
        ensure!(soc_buffer_delta >= si::Ratio::ZERO, "{}", format_dbg!());
        self.state
            .soc_disch_buffer
            .update(self.min_soc + soc_buffer_delta, || format_dbg!())?;
        let pwr_max_for_dt = ((*self.state.soc.get_stale(|| format_dbg!())? - self.min_soc)
            * self.energy_capacity
            / dt)
            .max(si::Power::ZERO);
        self.state.pwr_disch_max.update(
            if *self.state.soc.get_stale(|| format_dbg!())?
                > *self.state.soc_disch_buffer.get_fresh(|| format_dbg!())?
            {
                self.pwr_out_max
            } else if *self.state.soc.get_stale(|| format_dbg!())? > self.min_soc
                && soc_buffer_delta > si::Ratio::ZERO
            {
                self.pwr_out_max * (*self.state.soc.get_stale(|| format_dbg!())? - self.min_soc)
                    / soc_buffer_delta
            } else {
                // current SOC is less than both
                si::Power::ZERO
            }
            .min(pwr_max_for_dt),
            || format_dbg!(),
        )?;

        ensure!(
            *self.state.pwr_disch_max.get_fresh(|| format_dbg!())? >= si::Power::ZERO,
            "{}\n`{}` ({} W) must be greater than or equal to zero\n{}",
            format_dbg!(),
            stringify!(self.state.pwr_disch_max),
            self.state
                .pwr_disch_max
                .get_fresh(|| format_dbg!())?
                .get::<si::watt>()
                .format_eng(None),
            format_dbg!(soc_buffer_delta)
        );

        Ok(())
    }

    /// Set current maximum power available for propulsion
    /// # Arguments
    /// - `pwr_aux`: aux power demand on `ReversibleEnergyStorage`
    pub fn set_curr_pwr_prop_max(&mut self, pwr_aux: si::Power) -> anyhow::Result<()> {
        let state = &mut self.state;
        state.pwr_aux.update(pwr_aux, || format_dbg!())?;
        state.pwr_prop_max.update(
            *state.pwr_disch_max.get_fresh(|| format_dbg!())? - pwr_aux,
            || format_dbg!(),
        )?;
        state.pwr_regen_max.update(
            *state.pwr_charge_max.get_fresh(|| format_dbg!())? + pwr_aux,
            || format_dbg!(),
        )?;

        ensure!(
            pwr_aux <= *state.pwr_disch_max.get_fresh(|| format_dbg!())?,
            "{}\n`{}` ({} W) must always be less than or equal to {} ({} W)\n`state.soc`:{}
`soc_disch_buffer`: {}",
            format_dbg!(),
            stringify!(pwr_aux),
            pwr_aux.get::<si::watt>().format_eng(None),
            stringify!(state.pwr_disch_max),
            state
                .pwr_disch_max
                .get_fresh(|| format_dbg!())?
                .get::<si::watt>()
                .format_eng(None),
            state
                .soc
                .get_stale(|| format_dbg!())?
                .get::<si::ratio>()
                .format_eng(None),
            state
                .soc_disch_buffer
                .get_fresh(|| format_dbg!())?
                .get::<si::ratio>()
                .format_eng(None)
        );
        ensure!(
            *state.pwr_prop_max.get_fresh(|| format_dbg!())? >= si::Power::ZERO,
            "{}\n`{}` ({} W) must be greater than or equal to zero",
            format_dbg!(),
            stringify!(state.pwr_prop_max),
            state
                .pwr_prop_max
                .get_fresh(|| format_dbg!())?
                .get::<si::watt>()
                .format_eng(None)
        );
        ensure!(
            *state.pwr_regen_max.get_fresh(|| format_dbg!())? >= si::Power::ZERO,
            "{}\n`{}` ({} W) must be greater than or equal to zero",
            format_dbg!(),
            stringify!(state.pwr_regen_max),
            state
                .pwr_regen_max
                .get_fresh(|| format_dbg!())?
                .get::<si::watt>()
                .format_eng(None)
        );

        Ok(())
    }

    /// Sets specific energy and either mass or energy capacity of battery
    /// # Arguments
    /// - `specific_energy`: specific energy of battery
    /// - `side_effect`: whether to update mass or energy capacity
    pub fn set_specific_energy(
        mut self,
        specific_energy: si::SpecificEnergy,
        side_effect: SpecificEnergySideEffect,
    ) -> anyhow::Result<()> {
        self.specific_energy = Some(specific_energy);
        match side_effect {
            SpecificEnergySideEffect::Mass => self.set_mass(
                Some(self.energy_capacity / specific_energy),
                MassSideEffect::Intensive,
            )?,
            SpecificEnergySideEffect::Energy => {
                self.energy_capacity = specific_energy
                    * self.mass.with_context(|| {
                        format_dbg!("Expected `ReversibleEnergyStorage::mass` to have been set.")
                    })?;
            }
        }
        Ok(())
    }

    /// Returns max value of [Self::eff_interp]
    pub fn get_eff_max(&self) -> anyhow::Result<f64> {
        Ok(*self.eff_interp.max()?)
    }

    /// Scales eff_interp by ratio of new `eff_max` per current calculated
    /// max linearly (Note: this may change eff_min)
    pub fn set_eff_max(
        &mut self,
        eff_max: f64,
        scaling: Option<ScalingMethods>,
    ) -> anyhow::Result<()> {
        self.eff_interp.set_max(eff_max, scaling)
    }

    /// Returns min value of [Self::eff_interp]
    pub fn get_eff_min(&self) -> anyhow::Result<&f64> {
        self.eff_interp.min()
    }

    /// Scales eff_interp by ratio of new `eff_min` per current calculated
    /// min linearly (Note: this may change eff_max)
    pub fn set_eff_min(
        &mut self,
        eff_min: f64,
        scaling: Option<ScalingMethods>,
    ) -> anyhow::Result<()> {
        self.eff_interp.set_min(eff_min, scaling)
    }

    /// Max value of `eff_interp` minus min value of `eff_interp`.
    pub fn get_eff_range(&self) -> anyhow::Result<f64> {
        self.eff_interp.range()
    }

    /// Scales values of `eff_interp` without changing max such that max - min
    /// is equal to new range.  Will change max if needed to ensure no values are
    /// less than zero.
    pub fn set_eff_range(&mut self, eff_range: f64) -> anyhow::Result<()> {
        self.eff_interp.set_range(eff_range)
    }

    /// Usable energy capacity, accounting for SOC limits
    pub fn energy_capacity_usable(&self) -> si::Energy {
        self.energy_capacity * (self.max_soc - self.min_soc)
    }

    /// Sets the ReversibleEnergyStorage eff_interp Interpolator to be a 1D
    /// interpolator with the default x and f_x arrays  
    /// # Source of default efficiency values  
    /// - `x`: values in the third sub-array (corresponding to power) in ALTRIOS's
    ///   eta_interp_grid  
    /// - `f_x`: efficiency array as a function of power at constant 50% SOC and 23
    ///   °C corresponds to `eta_interp_values[0][5]` in ALTRIOS
    #[cfg(all(feature = "yaml", feature = "resources"))]
    pub fn set_default_pwr_interp(&mut self) -> anyhow::Result<()> {
        if let InterpolatorEnum::Interp1D(interp1d) =
            InterpolatorEnum::from_resource("res/default_pwr.yaml", false)?
        {
            self.eff_interp = RESEfficiency::CRate(interp1d);
        } else {
            bail!("Invalid interpolator format. Expected `Interp1D`")
        }
        Ok(())
    }

    /// Sets the ReversibleEnergyStorage eff_interp Interpolator to be a 2D
    /// interpolator with the default x, y, and f_xy arrays  
    /// # Source of default efficiency values  
    /// - `x`: values in the third sub-array (corresponding to power) in ALTRIOS's
    ///   eta_interp_grid  
    /// - `y`: values in the second sub-array (corresponding to SOC) in
    ///   ALTRIOS's eta_interp_grid  
    /// - `f_xy`: efficiency array as a function of power and SOC at constant 23
    ///   °C corresponds to `eta_interp_values[0]` in ALTRIOS, transposed so
    ///   that the outermost layer is now power and the innermost layer SOC (in
    ///   ALTRIOS, the outermost layer is SOC and innermost is power)
    #[cfg(all(feature = "yaml", feature = "resources"))]
    pub fn set_default_pwr_and_soc_interp(&mut self) -> anyhow::Result<()> {
        if let InterpolatorEnum::Interp2D(interp2d) =
            InterpolatorEnum::from_resource("res/default_pwr_and_soc.yaml", false)?
        {
            self.eff_interp = RESEfficiency::CRateSOC(interp2d);
        } else {
            bail!("Invalid interpolator format. Expected `Interp2D`")
        }
        Ok(())
    }

    /// - `f_xy`: efficiency array as a function of power and temperature at
    ///   constant 50% SOC
    #[cfg(all(feature = "yaml", feature = "resources"))]
    pub fn set_default_pwr_and_temp_interp(&mut self) -> anyhow::Result<()> {
        if let InterpolatorEnum::Interp2D(interp2d) =
            InterpolatorEnum::from_resource("res/default_pwr_and_temp.yaml", false)?
        {
            self.eff_interp = RESEfficiency::CRateTemperature(interp2d);
        } else {
            bail!("Invalid interpolator format. Expected `Interp2D`")
        }
        Ok(())
    }

    /// Sets the ReversibleEnergyStorage eff_interp Interpolator to be a 3D
    /// interpolator with the default x, y, z, and f_xyz arrays  
    /// # Source of default efficiency values  
    /// - `x`: values in the third sub-array (corresponding to power) in ALTRIOS's
    ///   eta_interp_grid  
    /// - `y`: values in the second sub-array (corresponding to SOC) in ALTRIOS's
    ///   eta_interp_grid  
    /// - `z`: values in the first sub-array (corresponding to temperature) in
    ///   ALTRIOS's eta_interp_grid  
    /// - `f_xyz`: efficiency array as a function of power, SOC, and temperature
    ///   corresponds to eta_interp_values in ALTRIOS, transposed so that the
    ///   outermost layer is now power, and the innermost layer temperature (in
    ///   ALTRIOS, the outermost layer is temperature and innermost is power)
    #[cfg(all(feature = "yaml", feature = "resources"))]
    pub fn set_default_pwr_soc_and_temp_interp(&mut self) -> anyhow::Result<()> {
        if let InterpolatorEnum::Interp3D(interp3d) =
            InterpolatorEnum::from_resource("res/default_pwr_soc_and_temp.yaml", false)?
        {
            self.eff_interp = RESEfficiency::CRateSOCTemperature(interp3d);
        } else {
            bail!("Invalid interpolator format. Expected `Interp2D`")
        }
        Ok(())
    }

    /// If thermal model is appropriately configured, returns current lumped [Self] temperature
    pub fn res_thrml_state(&self) -> Option<&RESLumpedThermalState> {
        match &self.thrml {
            RESThermalOption::RESLumpedThermal(rest) => Some(&rest.state),
            RESThermalOption::None => None,
        }
    }

    /// If thermal model is appropriately configured, returns current lumped [Self] temperature
    pub fn res_thrml_state_mut(&mut self) -> Option<&mut RESLumpedThermalState> {
        match &mut self.thrml {
            RESThermalOption::RESLumpedThermal(rest) => Some(&mut rest.state),
            RESThermalOption::None => None,
        }
    }

    /// If thermal model is appropriately configured, returns current lumped [Self] temperature
    pub fn temperature(&self) -> anyhow::Result<Option<si::Temperature>> {
        match &self.thrml {
            RESThermalOption::RESLumpedThermal(rest) => {
                Some(rest.state.temperature.get_fresh(|| format_dbg!()).cloned())
            }
            RESThermalOption::None => None,
        }
        .transpose()
    }
}

impl Mass for ReversibleEnergyStorage {
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
                            self.energy_capacity = self.specific_energy.ok_or_else(|| {
                                anyhow!(
                                    "{}\nExpected `self.specific_energy` to be `Some`.",
                                    format_dbg!()
                                )
                            })? * new_mass;
                        }
                        MassSideEffect::Intensive => {
                            self.specific_energy = Some(self.energy_capacity / new_mass);
                        }
                        MassSideEffect::None => {
                            self.specific_energy = None;
                        }
                    }
                }
                Some(new_mass)
            }
            (Some(new_mass), None) => Some(new_mass),
            (None, Some(dm)) => Some(dm),
            (None, None) => bail!(
                "Not all mass fields in `{}` are set and no mass was provided.",
                stringify!(ReversibleEnergyStorage)
            ),
        };
        ensure!(
            self.mass > Some(0.0 * uc::KG),
            "{} mass must be positive",
            stringify!(ReversibleEnergyStorage)
        );
        Ok(())
    }

    fn derived_mass(&self) -> anyhow::Result<Option<si::Mass>> {
        Ok(self
            .specific_energy
            .map(|specific_energy| self.energy_capacity / specific_energy))
    }

    fn expunge_mass_fields(&mut self) {
        self.mass = None;
        self.specific_energy = None;
    }
}

impl SerdeAPI for ReversibleEnergyStorage {}
impl Init for ReversibleEnergyStorage {
    fn init(&mut self) -> Result<(), Error> {
        let _ = self
            .mass()
            .map_err(|err| Error::InitError(format_dbg!(err)))?;
        self.eff_interp
            .validate()
            .map_err(|err| Error::InitError(format_dbg!(err)))?;
        self.state
            .init()
            .map_err(|err| Error::InitError(format_dbg!(err)))?;
        // TODO: make some kind of data validation framework to replace this code.
        if self.max_soc <= self.min_soc {
            return Err(Error::InitError(format!(
                "{}\n`max_soc`: {} must be greater than `min_soc`: {}`",
                format_dbg!(),
                self.max_soc.get::<si::ratio>(),
                self.min_soc.get::<si::ratio>(),
            )));
        };
        Ok(())
    }
}
impl HistoryMethods for ReversibleEnergyStorage {
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

#[derive(
    Clone, Debug, Serialize, Deserialize, PartialEq, IsVariant, derive_more::From, TryInto,
)]
/// Controls which parameter to update when setting specific energy
pub enum SpecificEnergySideEffect {
    /// update mass
    Mass,
    /// update energy
    Energy,
}

#[serde_api]
#[derive(
    Clone, Debug, Deserialize, Serialize, PartialEq, HistoryVec, StateMethods, SetCumulative,
)]
#[non_exhaustive]
#[serde(deny_unknown_fields)]
#[cfg_attr(feature = "pyo3", pyclass(module = "fastsim", subclass, eq))]
#[serde(default)]
/// ReversibleEnergyStorage state variables
#[is_state]
pub struct ReversibleEnergyStorageState {
    // limits
    /// max output power for propulsion during positive traction
    pub pwr_prop_max: TrackedState<si::Power>,
    /// max regen power for propulsion during negative traction
    pub pwr_regen_max: TrackedState<si::Power>,
    /// max discharge power total
    pub pwr_disch_max: TrackedState<si::Power>,
    /// max charge power on the output side
    pub pwr_charge_max: TrackedState<si::Power>,

    /// time step index
    pub i: TrackedState<usize>,

    /// state of charge (SOC)
    pub soc: TrackedState<si::Ratio>,
    /// SOC at which [ReversibleEnergyStorage] regen power begins linearly
    /// derating as it approaches maximum SOC
    pub soc_regen_buffer: TrackedState<si::Ratio>,
    /// SOC at which [ReversibleEnergyStorage] discharge power begins linearly
    /// derating as it approaches minimum SOC
    pub soc_disch_buffer: TrackedState<si::Ratio>,
    /// Chemical <-> Electrical conversion efficiency based on current power demand
    pub eff: TrackedState<si::Ratio>,
    /// State of Health (SOH)
    pub soh: TrackedState<f64>,

    // TODO: add `pwr_out_neg_electrical` and `pwr_out_pos_electrical` and corresponding energies
    // powers to separately pin negative- and positive-power operation
    /// total electrical power; positive is discharging
    pub pwr_out_electrical: TrackedState<si::Power>,
    /// electrical power going to propulsion
    pub pwr_out_prop: TrackedState<si::Power>,
    /// electrical power going to aux loads
    pub pwr_aux: TrackedState<si::Power>,
    /// power dissipated as loss
    pub pwr_loss: TrackedState<si::Power>,
    /// chemical power; positive is discharging
    pub pwr_out_chemical: TrackedState<si::Power>,

    // cumulative energies
    /// cumulative total electrical energy; positive is discharging
    pub energy_out_electrical: TrackedState<si::Energy>,
    /// cumulative electrical energy going to propulsion
    pub energy_out_prop: TrackedState<si::Energy>,
    /// cumulative electrical energy going to aux loads
    pub energy_aux: TrackedState<si::Energy>,
    /// cumulative energy dissipated as loss
    pub energy_loss: TrackedState<si::Energy>,
    /// cumulative chemical energy; positive is discharging
    pub energy_out_chemical: TrackedState<si::Energy>,
}

#[pyo3_api]
impl ReversibleEnergyStorageState {}

impl Default for ReversibleEnergyStorageState {
    fn default() -> Self {
        Self {
            pwr_prop_max: Default::default(),
            pwr_regen_max: Default::default(),
            pwr_disch_max: Default::default(),
            pwr_charge_max: Default::default(),
            i: Default::default(),
            soc: TrackedState::new(uc::R * 0.5),
            soc_regen_buffer: TrackedState::new(uc::R * 1.),
            soc_disch_buffer: Default::default(),
            eff: Default::default(),
            soh: Default::default(),
            pwr_out_electrical: Default::default(),
            pwr_out_prop: Default::default(),
            pwr_aux: Default::default(),
            pwr_loss: Default::default(),
            pwr_out_chemical: Default::default(),
            energy_out_electrical: Default::default(),
            energy_out_prop: Default::default(),
            energy_aux: Default::default(),
            energy_loss: Default::default(),
            energy_out_chemical: Default::default(),
        }
    }
}
impl Init for ReversibleEnergyStorageState {}
impl SerdeAPI for ReversibleEnergyStorageState {}

#[derive(
    Clone, Default, Debug, Serialize, Deserialize, PartialEq, IsVariant, derive_more::From, TryInto,
)]
pub enum RESThermalOption {
    /// Basic thermal plant for [ReversibleEnergyStorage]
    RESLumpedThermal(Box<RESLumpedThermal>),
    /// no thermal plant for [ReversibleEnergyStorage]
    #[default]
    None,
}
impl SetCumulative for RESThermalOption {
    fn set_cumulative<F: Fn() -> String>(&mut self, dt: si::Time, loc: F) -> anyhow::Result<()> {
        match self {
            Self::RESLumpedThermal(rlt) => {
                rlt.set_cumulative(dt, || format!("{}\n{}", loc(), format_dbg!()))?
            }
            Self::None => {}
        }
        Ok(())
    }

    fn reset_cumulative<F: Fn() -> String>(&mut self, loc: F) -> anyhow::Result<()> {
        match self {
            Self::RESLumpedThermal(rlt) => {
                rlt.reset_cumulative(|| format!("{}\n{}", loc(), format_dbg!()))?
            }
            Self::None => {}
        }
        Ok(())
    }
}

impl StateMethods for RESThermalOption {}

impl SaveState for RESThermalOption {
    fn save_state<F: Fn() -> String>(&mut self, loc: F) -> anyhow::Result<()> {
        match self {
            Self::RESLumpedThermal(rlt) => rlt.save_state(loc)?,
            Self::None => {}
        }
        Ok(())
    }
}
impl TrackedStateMethods for RESThermalOption {
    fn check_and_reset<F: Fn() -> String>(&mut self, loc: F) -> anyhow::Result<()> {
        match self {
            Self::RESLumpedThermal(rlt) => {
                rlt.check_and_reset(|| format!("{}\n{}", loc(), format_dbg!()))?
            }
            Self::None => {}
        }
        Ok(())
    }

    fn mark_fresh<F: Fn() -> String>(&mut self, loc: F) -> anyhow::Result<()> {
        match self {
            Self::RESLumpedThermal(rlt) => {
                rlt.mark_fresh(|| format!("{}\n{}", loc(), format_dbg!()))?
            }
            Self::None => {}
        }
        Ok(())
    }
}
impl Step for RESThermalOption {
    fn step<F: Fn() -> String>(&mut self, loc: F) -> anyhow::Result<()> {
        match self {
            Self::RESLumpedThermal(rlt) => rlt.step(|| format!("{}\n{}", loc(), format_dbg!())),
            Self::None => Ok(()),
        }
    }

    fn reset_step<F: Fn() -> String>(&mut self, loc: F) -> anyhow::Result<()> {
        match self {
            Self::RESLumpedThermal(rlt) => {
                rlt.reset_step(|| format!("{}\n{}", loc(), format_dbg!()))
            }
            Self::None => Ok(()),
        }
    }
}
impl Init for RESThermalOption {
    fn init(&mut self) -> Result<(), Error> {
        match self {
            Self::RESLumpedThermal(rest) => rest.init()?,
            Self::None => {}
        }
        Ok(())
    }
}
impl SerdeAPI for RESThermalOption {}
impl HistoryMethods for RESThermalOption {
    fn save_interval(&self) -> anyhow::Result<Option<usize>> {
        match self {
            RESThermalOption::RESLumpedThermal(rlt) => rlt.save_interval(),
            RESThermalOption::None => Ok(None),
        }
    }
    fn set_save_interval(&mut self, save_interval: Option<usize>) -> anyhow::Result<()> {
        match self {
            RESThermalOption::RESLumpedThermal(rlt) => rlt.set_save_interval(save_interval),
            RESThermalOption::None => Ok(()),
        }
    }
    fn clear(&mut self) {
        match self {
            RESThermalOption::RESLumpedThermal(rlt) => rlt.clear(),
            RESThermalOption::None => {}
        }
    }
}
impl RESThermalOption {
    /// Solve change in temperature and other thermal effects
    /// # Arguments
    /// - `res_state`: [ReversibleEnergyStorage] state
    /// - `te_amb`: ambient temperature
    /// - `pwr_thrml_hvac_to_res`: thermal power flowing from [Vehicle::hvac]
    ///   system to [Self], required if [Self::is_none] is false
    /// - `dt`: simulation time step size
    fn solve(
        &mut self,
        res_state: &mut ReversibleEnergyStorageState,
        te_amb: si::Temperature,
        pwr_thrml_hvac_to_res: si::Power,
        te_cab: Option<si::Temperature>,
        dt: si::Time,
    ) -> anyhow::Result<()> {
        match self {
            Self::RESLumpedThermal(rest) => rest
                .solve(
                    res_state,
                    te_amb,
                    pwr_thrml_hvac_to_res,
                    te_cab.with_context(|| {
                        format_dbg!(
                            "`te_cab` must be `Some` for [RESThermalOption::RESLumpedThermal]"
                        )
                    })?,
                    dt,
                )
                .with_context(|| format_dbg!())?,
            Self::None => {
                // TODO: make sure this triggers error if appropriate
            }
        }
        Ok(())
    }
}

#[serde_api]
#[derive(Default, Deserialize, Serialize, Debug, Clone, PartialEq, StateMethods, SetCumulative)]
#[cfg_attr(feature = "pyo3", pyclass(module = "fastsim", subclass, eq))]
#[serde(deny_unknown_fields)]
/// Struct for modeling [ReversibleEnergyStorage] (e.g. battery) thermal plant
pub struct RESLumpedThermal {
    /// [ReversibleEnergyStorage] thermal capacitance
    pub heat_capacitance: si::HeatCapacity,
    /// parameter for heat transfer coeff from [ReversibleEnergyStorage::thrml] to ambient
    pub conductance_to_amb: si::ThermalConductance,
    /// parameter for heat transfer coeff from [ReversibleEnergyStorage::thrml] to cabin
    pub conductance_to_cab: si::ThermalConductance,
    /// current state
    #[serde(default)]
    #[is_state]
    pub state: RESLumpedThermalState,
    /// history of state
    #[serde(
        default,
        skip_serializing_if = "RESLumpedThermalStateHistoryVec::is_empty"
    )]
    pub history: RESLumpedThermalStateHistoryVec,
    pub save_interval: Option<usize>,
}

#[pyo3_api]
impl RESLumpedThermal {
    #[staticmethod]
    #[pyo3(name = "default")]
    fn default_py() -> Self {
        Default::default()
    }
}

impl RESLumpedThermal {
    pub fn new(
        heat_capacitance: si::HeatCapacity,
        conductance_to_amb: si::ThermalConductance,
        conductance_to_cab: si::ThermalConductance,
        save_interval: Option<usize>,
    ) -> anyhow::Result<Self> {
        let mut res_lumped_thermal = Self {
            heat_capacitance,
            conductance_to_amb,
            conductance_to_cab,
            state: RESLumpedThermalState::default(),
            history: RESLumpedThermalStateHistoryVec::default(),
            save_interval,
        };
        res_lumped_thermal.init()?;
        Ok(res_lumped_thermal)
    }
}

impl SerdeAPI for RESLumpedThermal {}
impl Init for RESLumpedThermal {}
impl HistoryMethods for RESLumpedThermal {
    fn save_interval(&self) -> anyhow::Result<Option<usize>> {
        Ok(self.save_interval)
    }
    fn set_save_interval(&mut self, save_interval: Option<usize>) -> anyhow::Result<()> {
        self.save_interval = save_interval;
        Ok(())
    }
    fn clear(&mut self) {
        self.history.clear()
    }
}
impl RESLumpedThermal {
    fn solve(
        &mut self,
        res_state: &mut ReversibleEnergyStorageState,
        te_amb: si::Temperature,
        pwr_thrml_hvac_to_res: si::Power,
        te_cab: si::Temperature,
        dt: si::Time,
    ) -> anyhow::Result<()> {
        // TODO: make sure this impacts cabin temperature
        self.state.pwr_thrml_from_cabin.update(
            self.conductance_to_cab
                * (te_cab.get::<si::degree_celsius>()
                    - self
                        .state
                        .temperature
                        .get_stale(|| format_dbg!())?
                        .get::<si::degree_celsius>())
                * uc::KELVIN_INT,
            || format_dbg!(),
        )?;
        self.state
            .pwr_thrml_hvac_to_res
            .update(pwr_thrml_hvac_to_res, || format_dbg!())?;
        self.state.pwr_thrml_from_amb.update(
            self.conductance_to_amb
                * (te_amb.get::<si::degree_celsius>()
                    - self
                        .state
                        .temperature
                        .get_stale(|| format_dbg!())?
                        .get::<si::degree_celsius>())
                * uc::KELVIN_INT,
            || format_dbg!(),
        )?;
        self.state.pwr_thrml_loss.update(
            res_state
                .pwr_out_electrical
                .get_stale(|| format_dbg!())?
                .abs()
                * (1.0 * uc::R - *res_state.eff.get_stale(|| format_dbg!())?),
            || format_dbg!(),
        )?;
        self.state.temp_prev.update(
            *self.state.temperature.get_stale(|| format_dbg!())?,
            || format_dbg!(),
        )?;
        self.state.temperature.update(
            *self.state.temperature.get_stale(|| format_dbg!())?
                + (*self
                    .state
                    .pwr_thrml_hvac_to_res
                    .get_fresh(|| format_dbg!())?
                    + *self.state.pwr_thrml_loss.get_fresh(|| format_dbg!())?
                    + *self
                        .state
                        .pwr_thrml_from_cabin
                        .get_fresh(|| format_dbg!())?
                    + *self.state.pwr_thrml_from_amb.get_fresh(|| format_dbg!())?)
                    / self.heat_capacitance
                    * dt,
            || format_dbg!(),
        )?;
        Ok(())
    }
}

#[serde_api]
#[derive(
    Clone, Debug, Deserialize, Serialize, PartialEq, HistoryVec, StateMethods, SetCumulative,
)]
#[cfg_attr(feature = "pyo3", pyclass(module = "fastsim", subclass, eq))]
#[serde(deny_unknown_fields)]
#[is_state]
pub struct RESLumpedThermalState {
    /// time step index
    pub i: TrackedState<usize>,
    /// Current thermal mass temperature
    pub temperature: TrackedState<si::Temperature>,
    /// Thermal mass temperature at start of previous time step
    pub temp_prev: TrackedState<si::Temperature>,
    /// Thermal power flow to [RESLumpedThermal] from cabin
    pub pwr_thrml_from_cabin: TrackedState<si::Power>,
    /// Cumulative thermal energy flow to [RESLumpedThermal] from cabin
    pub energy_thrml_from_cabin: TrackedState<si::Energy>,
    /// Thermal power flow to [RESLumpedThermal] from ambient
    pub pwr_thrml_from_amb: TrackedState<si::Power>,
    /// Cumulative thermal energy flow to [RESLumpedThermal] from ambient
    pub energy_thrml_from_amb: TrackedState<si::Energy>,
    /// Thermal power flow to [RESLumpedThermal] from HVAC
    pub pwr_thrml_hvac_to_res: TrackedState<si::Power>,
    /// Cumulative thermal energy flow to [RESLumpedThermal] from HVAC
    pub energy_thrml_hvac_to_res: TrackedState<si::Energy>,
    /// Thermal generation due to losses
    pub pwr_thrml_loss: TrackedState<si::Power>,
    /// Cumulative thermal energy generation due to losses
    pub energy_thrml_loss: TrackedState<si::Energy>,
}

#[pyo3_api]
impl RESLumpedThermalState {
    #[pyo3(name = "default")]
    #[staticmethod]
    fn default_py() -> Self {
        Self::default()
    }
}

impl Init for RESLumpedThermalState {}
impl SerdeAPI for RESLumpedThermalState {}
impl Default for RESLumpedThermalState {
    fn default() -> Self {
        Self {
            i: Default::default(),
            temperature: TrackedState::new(*TE_STD_AIR),
            temp_prev: TrackedState::new(*TE_STD_AIR),
            pwr_thrml_from_cabin: Default::default(),
            energy_thrml_from_cabin: Default::default(),
            pwr_thrml_from_amb: Default::default(),
            energy_thrml_from_amb: Default::default(),
            pwr_thrml_hvac_to_res: Default::default(),
            energy_thrml_hvac_to_res: Default::default(),
            pwr_thrml_loss: Default::default(),
            energy_thrml_loss: Default::default(),
        }
    }
}

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, IsVariant, TryInto)]
/// Determines what [ReversibleEnergyStorage] state variables to use in calculating efficiency
pub enum RESEfficiency {
    /// Efficiency is constant
    Constant(#[serde(serialize_with = "serialize_nested")] Interp0D<f64>),
    /// Efficiency = f(C-rate)
    CRate(
        #[serde(serialize_with = "serialize_nested")]
        Interp1D<f64, strategy::enums::Strategy1DEnum<f64>>,
    ),
    /// Efficiency = f(C-rate, soc, temperature)
    CRateSOCTemperature(
        #[serde(serialize_with = "serialize_nested")]
        Interp3D<f64, strategy::enums::Strategy3DEnum<f64>>,
    ),
    /// Efficiency = f(C-rate, temperature)
    CRateTemperature(
        #[serde(serialize_with = "serialize_nested")]
        Interp2D<f64, strategy::enums::Strategy2DEnum<f64>>,
    ),
    /// Efficiency = f(C-rate, soc)
    CRateSOC(
        #[serde(serialize_with = "serialize_nested")]
        Interp2D<f64, strategy::enums::Strategy2DEnum<f64>>,
    ),
    // TODO: finish adding possible variants
}

impl_efficiency_enum!(RESEfficiency {
    Constant,
    CRate,
    CRateSOCTemperature,
    CRateTemperature,
    CRateSOC,
});
