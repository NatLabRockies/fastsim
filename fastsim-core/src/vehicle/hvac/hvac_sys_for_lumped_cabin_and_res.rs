use super::*;

#[serde_api]
#[derive(Deserialize, Serialize, Debug, Clone, PartialEq, StateMethods, SetCumulative)]
#[cfg_attr(feature = "pyo3", pyclass(module = "fastsim", subclass, eq))]
#[serde(deny_unknown_fields)]
/// HVAC system for `LumpedCabin` and `ReversibleEnergyStorage::thrml`
pub struct HVACSystemForLumpedCabinAndRES {
    /// set point temperature
    pub te_set_cab: Option<si::Temperature>,
    /// Deadband half range.  Any cabin temperature within `te_deadband_cab` of
    /// `te_set_cab` results in no HVAC power draw
    pub te_deadband_cab: si::TemperatureInterval,
    /// HVAC proportional gain for cabin
    pub p_cabin: si::ThermalConductance,
    /// HVAC integral gain [W / K / s] for cabin, resets at zero crossing events  
    /// NOTE: `uom` crate does not have this unit, but it may be possible to make a custom unit for this
    pub i_cabin: f64,
    /// value at which state.i stops accumulating for cabin
    pub pwr_i_max_cabin: si::Power,
    /// HVAC derivative gain [W / K * s] for cabin
    /// NOTE: `uom` crate does not have this unit, but it may be possible to make a custom unit for this
    pub d_cabin: f64,
    /// set point temperature
    pub te_set_res: Option<si::Temperature>,
    /// Deadband half range.  Any res temperature within `te_deadband_res` of
    /// `te_set_res` results in no HVAC power draw
    pub te_deadband_res: si::TemperatureInterval,
    /// max HVAC thermal power
    /// HVAC proportional gain for [ReversibleEnergyStorage]
    pub p_res: si::ThermalConductance,
    /// HVAC integral gain [W / K / s] for [ReversibleEnergyStorage], resets at zero crossing events  
    /// NOTE: `uom` crate does not have this unit, but it may be possible to make a custom unit for this
    pub i_res: f64,
    /// value at which state.i stops accumulating for [ReversibleEnergyStorage]
    pub pwr_i_max_res: si::Power,
    /// HVAC derivative gain [W / K * s] for [ReversibleEnergyStorage]
    /// NOTE: `uom` crate does not have this unit, but it may be possible to make a custom unit for this
    pub d_res: f64,
    /// max HVAC thermal power
    pub pwr_thrml_max: si::Power,
    /// coefficient between 0 and 1 to calculate HVAC efficiency by multiplying by
    /// coefficient of performance (COP)
    pub frac_of_ideal_cop: f64,
    /// cabin heat source
    pub cabin_heat_source: CabinHeatSource,
    /// res heat source
    pub res_heat_source: RESHeatSource,
    /// res cooling source
    pub res_cooling_source: RESCoolingSource,
    /// max allowed aux load for cabin thermal management
    pub pwr_aux_for_hvac_cab_max: si::Power,
    /// max allowed aux load for res thermal management
    pub pwr_aux_for_hvac_res_max: si::Power,
    /// coefficient of performance of vapor compression cycle
    #[serde(default)]
    pub state: HVACSystemForLumpedCabinAndRESState,
    #[serde(
        default,
        skip_serializing_if = "HVACSystemForLumpedCabinAndRESStateHistoryVec::is_empty"
    )]
    pub history: HVACSystemForLumpedCabinAndRESStateHistoryVec,
    pub save_interval: Option<usize>,
}
#[pyo3_api]
impl HVACSystemForLumpedCabinAndRES {
    #[staticmethod]
    #[pyo3(name = "default")]
    fn default_py() -> Self {
        Default::default()
    }
}
impl Default for HVACSystemForLumpedCabinAndRES {
    fn default() -> Self {
        Self {
            te_set_cab: Some(*TE_STD_AIR),
            te_deadband_cab: 1.5 * uc::KELVIN_INT,
            p_cabin: Default::default(),
            i_cabin: Default::default(),
            d_cabin: Default::default(),
            pwr_i_max_cabin: 5. * uc::KW,
            te_set_res: Some(*TE_STD_AIR + 8.0 * uc::KELVIN_INT),
            te_deadband_res: 8.0 * uc::KELVIN_INT,
            p_res: Default::default(),
            i_res: Default::default(),
            d_res: Default::default(),
            pwr_i_max_res: 5. * uc::KW,
            pwr_thrml_max: 10. * uc::KW,
            frac_of_ideal_cop: 0.15,
            cabin_heat_source: CabinHeatSource::ResistanceHeater,
            res_heat_source: RESHeatSource::ResistanceHeater,
            res_cooling_source: RESCoolingSource::HVAC,
            pwr_aux_for_hvac_cab_max: uc::KW * 5.,
            pwr_aux_for_hvac_res_max: uc::KW * 5.,
            state: Default::default(),
            history: Default::default(),
            save_interval: Some(1),
        }
    }
}

impl Init for HVACSystemForLumpedCabinAndRES {}
impl SerdeAPI for HVACSystemForLumpedCabinAndRES {}
impl HistoryMethods for HVACSystemForLumpedCabinAndRES {
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

impl HVACSystemForLumpedCabinAndRES {
    /// # Arguments
    /// - `te_amb_air`: ambient air temperature
    /// - `te_fc`: [FuelConverter] temperature, if equipped
    /// - `cab_state`: [LumpedCabinState]
    /// - `cab_heat_cap`: [LumpedCabinState] heat capacity
    /// - `res_temp`: [ReversibleEnergyStorage] temperatures at current and previous time step
    /// - `dt`: simulation time step size
    ///
    /// # Returns
    /// - `pwr_thrml_hvac_to_cabin`: thermal power flowing from [Vehicle::hvac] system to cabin  
    /// - `pwr_thrml_fc_to_cabin`: thermal power flowing from [FuelConverter] to cabin  
    /// - `pwr_thrml_hvac_to_res`: thermal power flowing from [Vehicle::hvac] system to
    ///   [ReversibleEnergyStorage] `thrml` system  
    ///
    /// # Assumptions and Caveats
    /// - Cabin cooling never occurs concurrently with battery heating
    /// - Cabin heating never occurs concurrently with battery cooling
    /// - For real vehicles, control parameters for battery heating and cooling
    ///   are generally different during charging, and we do not currently account for
    ///   that
    #[allow(clippy::too_many_arguments)] // the order is reasonably protected by typing
    pub fn solve(
        &mut self,
        te_amb_air: si::Temperature,
        te_fc: Option<si::Temperature>,
        cab_state: &LumpedCabinState,
        cab_heat_cap: si::HeatCapacity,
        res_thrml_state: &RESLumpedThermalState,
        dt: si::Time,
    ) -> anyhow::Result<(si::Power, si::Power, si::Power)> {
        let (te_cab_delta_vs_set, te_cab_delta_vs_amb, te_res_delta_vs_set, te_res_delta_vs_amb) =
            self.get_te_deltas(te_amb_air, cab_state, res_thrml_state)?;

        ensure!(
            te_res_delta_vs_set.is_none() || !(self.res_cooling_source.is_none() && self.res_heat_source.is_none()),
            format!(
                "{}\n{}",
                format_dbg!(),
                "If RES set temperature is provided, either `res_cooling_source` and/or `res_heat_source` must not be None"
            )
        );

        let (te_ref, te_ref_delta_vs_set, te_ref_delta_vs_amb) = {
            self.state.set_mode_and_get_te_for_cop(
                cab_state,
                res_thrml_state,
                (
                    te_cab_delta_vs_set,
                    te_cab_delta_vs_amb,
                    te_res_delta_vs_set,
                    te_res_delta_vs_amb,
                ),
                (self.cabin_heat_source, self.te_deadband_cab),
                (
                    self.res_heat_source,
                    self.res_cooling_source,
                    self.te_deadband_res,
                ),
            )?
        };

        self.state.te_ref.update(te_ref, || format_dbg!())?;
        let cop = self
            .get_cop_ideal_vcs(te_ref, te_ref_delta_vs_set, te_ref_delta_vs_amb)?
            .map(|cop_ideal| cop_ideal * self.frac_of_ideal_cop);
        self.state.cop.update(cop, || format_dbg!())?;

        self.solve_for_cabin(te_fc, cab_state, cab_heat_cap, dt)
            .with_context(|| format_dbg!())?;
        self.solve_for_res(res_thrml_state, dt)
            .with_context(|| format_dbg!())?;

        Ok((
            *self
                .state
                .pwr_thrml_hvac_to_cabin
                .get_fresh(|| format_dbg!())?,
            *self
                .state
                .pwr_thrml_fc_to_cabin
                .get_fresh(|| format_dbg!())?,
            *self
                .state
                .pwr_thrml_hvac_to_res
                .get_fresh(|| format_dbg!())?,
        ))
    }

    fn get_te_deltas(
        &mut self,
        te_amb_air: si::Temperature,
        cab_state: &LumpedCabinState,
        res_thrml_state: &RESLumpedThermalState,
    ) -> anyhow::Result<(
        Option<si::TemperatureInterval>,
        si::TemperatureInterval,
        Option<si::TemperatureInterval>,
        si::TemperatureInterval,
    )> {
        let te_cab_delta_vs_set: Option<si::TemperatureInterval> = match self.te_set_cab {
            Some(te_set_cab) => Some(
                (cab_state
                    .temperature
                    .get_stale(|| format_dbg!())?
                    .get::<si::degree_celsius>()
                    - te_set_cab.get::<si::degree_celsius>())
                    * uc::KELVIN_INT,
            ),
            None => None,
        };

        let te_cab_delta_vs_amb: si::TemperatureInterval = (cab_state
            .temperature
            .get_stale(|| format_dbg!())?
            .get::<si::degree_celsius>()
            - te_amb_air.get::<si::degree_celsius>())
            * uc::KELVIN_INT;

        let te_res_delta_vs_set: Option<si::TemperatureInterval> = match self.te_set_res {
            Some(te_set_res) => Some(
                (res_thrml_state
                    .temperature
                    .get_stale(|| format_dbg!())?
                    .get::<si::degree_celsius>()
                    - te_set_res.get::<si::degree_celsius>())
                    * uc::KELVIN_INT,
            ),
            None => None,
        };

        let te_res_delta_vs_amb: si::TemperatureInterval = (res_thrml_state
            .temperature
            .get_stale(|| format_dbg!())?
            .get::<si::degree_celsius>()
            - te_amb_air.get::<si::degree_celsius>())
            * uc::KELVIN_INT;
        Ok((
            te_cab_delta_vs_set,
            te_cab_delta_vs_amb,
            te_res_delta_vs_set,
            te_res_delta_vs_amb,
        ))
    }

    /// Returns ideal coefficient of performance (COP) for vapor compression system (VCS)
    fn get_cop_ideal_vcs(
        &mut self,
        te_ref: Option<si::Temperature>,
        te_ref_delta_vs_set: Option<si::TemperatureInterval>,
        te_ref_delta_vs_amb: Option<si::TemperatureInterval>,
    ) -> anyhow::Result<Option<si::Ratio>> {
        // ideal COP if vapor compression sytem is active
        let cop_ideal_vcs =
            if let (Some(te_ref), Some(te_ref_delta_vs_set), Some(te_ref_delta_vs_amb)) =
                (te_ref, te_ref_delta_vs_set, te_ref_delta_vs_amb)
            {
                ensure!(
                    te_ref > si::Temperature::ZERO,
                    format!(
                        "{}\n`te_ref`: {} K",
                        format_dbg!(),
                        te_ref.get::<si::kelvin_abs>()
                    )
                );
                if te_ref_delta_vs_set > si::TemperatureInterval::ZERO {
                    // COOLING MODE; reference component is hotter than set point

                    // https://en.wikipedia.org/wiki/Coefficient_of_performance#Theoretical_performance_limits
                    // cop_ideal is t_h / (t_h - t_c) for heating
                    // cop_ideal is t_c / (t_h - t_c) for cooling

                    // divide-by-zero protection and realistic limit on COP
                    let cop_ideal = if -te_ref_delta_vs_amb < 5.0 * uc::KELVIN_INT {
                        // cabin is cooler than ambient + threshold
                        // TODO: make this `5.0` not hardcoded
                        te_ref / (5.0 * uc::KELVIN)
                    } else {
                        te_ref / te_ref_delta_vs_amb.abs()
                    };
                    ensure!(cop_ideal > 0.0 * uc::R, format_dbg!(cop_ideal));
                    Some(cop_ideal)
                } else {
                    // HEATING MODE; cabin is colder than set point

                    // https://en.wikipedia.org/wiki/Coefficient_of_performance#Theoretical_performance_limits
                    // cop_ideal is t_h / (t_h - t_c) for heating
                    // cop_ideal is t_c / (t_h - t_c) for cooling

                    // divide-by-zero protection and realistic limit on COP
                    let cop_ideal = if te_ref_delta_vs_amb < 5.0 * uc::KELVIN_INT {
                        // cabin is cooler than ambient + threshold
                        // TODO: make this `5.0` not hardcoded
                        te_ref / (5.0 * uc::KELVIN)
                    } else {
                        te_ref / te_ref_delta_vs_amb.abs()
                    };
                    ensure!(cop_ideal > 0.0 * uc::R, format_dbg!(cop_ideal));
                    Some(cop_ideal)
                }
            } else {
                None
            };
        Ok(cop_ideal_vcs)
    }

    /// Solve for thermal power for [LumpedCabin]
    fn solve_for_cabin(
        &mut self,
        te_fc: Option<si::Temperature>,
        cab_state: &LumpedCabinState,
        cab_heat_cap: si::HeatCapacity,
        dt: si::Time,
    ) -> anyhow::Result<()> {
        match self.te_set_cab {
            Some(te_set_cab) => {
                match self.state.cabin_mode.get_fresh(|| format_dbg!())? {
                    HvacMode::InsideDeadband => {
                        self.state
                            .pwr_i_cab
                            .update(si::Power::ZERO, || format_dbg!())?; // reset to 0.0
                        self.state
                            .pwr_p_cab
                            .update(si::Power::ZERO, || format_dbg!())?;
                        self.state
                            .pwr_d_cab
                            .update(si::Power::ZERO, || format_dbg!())?;
                        self.state
                            .pwr_aux_for_cab_hvac_req
                            .update(si::Power::ZERO, || format_dbg!())?;
                        self.state
                            .pwr_aux_for_cab_hvac
                            .update(si::Power::ZERO, || format_dbg!())?;
                        self.state
                            .pwr_thrml_to_cab_req
                            .update(si::Power::ZERO, || format_dbg!())?;
                        self.state
                            .pwr_thrml_hvac_to_cabin
                            .update(si::Power::ZERO, || format_dbg!())?;
                        self.state
                            .pwr_thrml_fc_to_cabin
                            .update(si::Power::ZERO, || format_dbg!())?;
                    }
                    HvacMode::Cooling => {
                        self.solve_cab_cooling(cab_state, dt, te_set_cab)?;
                    }
                    HvacMode::Heating => {
                        self.solve_cab_heating(te_fc, cab_state, cab_heat_cap, dt, te_set_cab)?;
                    }
                    HvacMode::Inactive => {
                        self.state
                            .pwr_i_cab
                            .update(si::Power::ZERO, || format_dbg!())?;
                        self.state
                            .pwr_p_cab
                            .update(si::Power::ZERO, || format_dbg!())?;
                        self.state
                            .pwr_d_cab
                            .update(si::Power::ZERO, || format_dbg!())?;
                        self.state
                            .pwr_aux_for_cab_hvac_req
                            .update(si::Power::ZERO, || format_dbg!())?;
                        self.state
                            .pwr_aux_for_cab_hvac
                            .update(si::Power::ZERO, || format_dbg!())?;
                        self.state
                            .pwr_thrml_hvac_to_cabin
                            .update(si::Power::ZERO, || format_dbg!())?;
                        self.state
                            .pwr_thrml_to_cab_req
                            .update(si::Power::ZERO, || format_dbg!())?;
                        self.state
                            .pwr_thrml_fc_to_cabin
                            .update(si::Power::ZERO, || format_dbg!())?;
                    }
                };
            }
            None => {
                self.state
                    .pwr_i_cab
                    .update(si::Power::ZERO, || format_dbg!())?;
                self.state
                    .pwr_p_cab
                    .update(si::Power::ZERO, || format_dbg!())?;
                self.state
                    .pwr_d_cab
                    .update(si::Power::ZERO, || format_dbg!())?;
                self.state
                    .pwr_aux_for_cab_hvac_req
                    .update(si::Power::ZERO, || format_dbg!())?;
                self.state
                    .pwr_aux_for_cab_hvac
                    .update(si::Power::ZERO, || format_dbg!())?;
                self.state
                    .pwr_thrml_to_cab_req
                    .update(si::Power::ZERO, || format_dbg!())?;
                self.state
                    .pwr_thrml_hvac_to_cabin
                    .update(si::Power::ZERO, || format_dbg!())?;
                self.state
                    .pwr_thrml_fc_to_cabin
                    .update(si::Power::ZERO, || format_dbg!())?;
            }
        }
        Ok(())
    }

    fn solve_cab_cooling(
        &mut self,
        cab_state: &LumpedCabinState,
        dt: si::Time,
        te_set_cab: si::Temperature,
    ) -> Result<(), anyhow::Error> {
        self.set_cab_cntrl_state(cab_state, dt, te_set_cab, HvacMode::Cooling)?;
        self.state.pwr_thrml_to_cab_req.update(
            {
                let pwr_thrml_hvac_to_cab_req: si::Power =
                    (*self.state.pwr_p_cab.get_fresh(|| format_dbg!())?
                        + *self.state.pwr_i_cab.get_fresh(|| format_dbg!())?
                        + *self.state.pwr_d_cab.get_fresh(|| format_dbg!())?)
                    .max(-self.pwr_thrml_max);
                ensure!(
                    pwr_thrml_hvac_to_cab_req <= si::Power::ZERO,
                    "HVAC should be cooling cabin\n{}\n{}\n{}",
                    format_dbg!(pwr_thrml_hvac_to_cab_req),
                    format_dbg!(*self
                        .state
                        .pwr_aux_for_cab_hvac
                        .get_fresh(|| format_dbg!())?),
                    format_dbg!(*self.state.cop.get_fresh(|| format_dbg!())?)
                );
                pwr_thrml_hvac_to_cab_req
            },
            || format_dbg!(),
        )?;
        self.state.pwr_aux_for_cab_hvac_req.update(
            -*self
                .state
                .pwr_thrml_to_cab_req
                .get_fresh(|| format_dbg!())?
                / self
                    .state
                    .cop
                    .get_fresh(|| format_dbg!())?
                    .with_context(|| {
                        format!(
                            "{}\nExpected `self.state.cop` to be Some.",
                            format_dbg!(self.state.cop)
                        )
                    })?,
            || format_dbg!(),
        )?;
        if *self
            .state
            .pwr_aux_for_cab_hvac_req
            .get_fresh(|| format_dbg!())?
            > self.pwr_aux_for_hvac_cab_max
        {
            self.state
                .pwr_aux_for_cab_hvac
                .update(self.pwr_aux_for_hvac_cab_max, || format_dbg!())?;
            self.state.pwr_thrml_hvac_to_cabin.update(
                -*self
                    .state
                    .pwr_aux_for_cab_hvac
                    .get_fresh(|| format_dbg!())?
                    * self
                        .state
                        .cop
                        .get_fresh(|| format_dbg!())?
                        .with_context(|| {
                            format!(
                                "{}\nExpected `self.state.cop` to be Some.",
                                format_dbg!(self.state.cop)
                            )
                        })?,
                || format_dbg!(),
            )?;
        } else {
            self.state.pwr_aux_for_cab_hvac.update(
                *self
                    .state
                    .pwr_aux_for_cab_hvac_req
                    .get_fresh(|| format_dbg!())?,
                || format_dbg!(),
            )?;
            self.state.pwr_thrml_hvac_to_cabin.update(
                *self
                    .state
                    .pwr_thrml_to_cab_req
                    .get_fresh(|| format_dbg!())?,
                || format_dbg!(),
            )?;
        }
        self.state
            .pwr_thrml_fc_to_cabin
            .update(si::Power::ZERO, || format_dbg!())?;
        Ok(())
    }

    fn solve_cab_heating(
        &mut self,
        te_fc: Option<si::Temperature>,
        cab_state: &LumpedCabinState,
        cab_heat_cap: si::HeatCapacity,
        dt: si::Time,
        te_set_cab: si::Temperature,
    ) -> Result<(), anyhow::Error> {
        self.set_cab_cntrl_state(cab_state, dt, te_set_cab, HvacMode::Heating)?;
        self.state.pwr_thrml_to_cab_req.update(
            {
                let mut pwr_thrml_to_cab_req =
                    (*self.state.pwr_p_cab.get_fresh(|| format_dbg!())?
                        + *self.state.pwr_i_cab.get_fresh(|| format_dbg!())?
                        + *self.state.pwr_d_cab.get_fresh(|| format_dbg!())?)
                    .min(self.pwr_thrml_max);
                if self.cabin_heat_source.is_fuel_converter() {
                    // limit heat transfer to be substantially less than what is physically possible
                    // i.e. the engine can't drop below cabin temperature to heat the cabin
                    pwr_thrml_to_cab_req = pwr_thrml_to_cab_req
                        .min(
                            cab_heat_cap *
                        (te_fc.unwrap().get::<si::degree_celsius>()
                            - cab_state.temperature.get_stale(|| format_dbg!())?
                                .get::<si::degree_celsius>()) * uc::KELVIN_INT
                                * 0.1 // so that it's substantially less
                                / dt,
                        )
                        .max(si::Power::ZERO);
                }
                pwr_thrml_to_cab_req
            },
            || format_dbg!(),
        )?;
        ensure!(
            *self
                .state
                .pwr_thrml_to_cab_req
                .get_fresh(|| format_dbg!())?
                >= si::Power::ZERO,
            "{}\nHVAC should be heating cabin\n{}\n{}\n{}\n{}",
            format_dbg!(self.state.pwr_thrml_to_cab_req),
            format!(
                "{}: {} W",
                stringify!(self.state.pwr_p_cab),
                self.state
                    .pwr_p_cab
                    .get_fresh(|| format_dbg!())?
                    .get::<si::watt>()
                    .format_eng(None)
            ),
            format!(
                "{}: {} W",
                stringify!(self.state.pwr_i_cab),
                self.state
                    .pwr_i_cab
                    .get_fresh(|| format_dbg!())?
                    .get::<si::watt>()
                    .format_eng(None)
            ),
            format!(
                "{}: {} W",
                stringify!(self.state.pwr_d_cab),
                self.state
                    .pwr_d_cab
                    .get_fresh(|| format_dbg!())?
                    .get::<si::watt>()
                    .format_eng(None)
            ),
            format!(
                "{}: {}*C",
                stringify!(cab_state.temperature),
                cab_state
                    .temperature
                    .get_stale(|| format_dbg!())?
                    .get::<si::degree_celsius>()
                    .format_eng(None)
            )
        );
        match self.cabin_heat_source {
            CabinHeatSource::FuelConverter => {
                // NOTE: should make this scale with power demand because it does require blower
                ensure!(
                    te_fc.is_some(),
                    "{}\nExpected vehicle with [FuelConverter] with thermal plant model.",
                    format_dbg!()
                );
                self.state
                    .pwr_aux_for_cab_hvac_req
                    .update(si::Power::ZERO, || format_dbg!())?;
            }
            CabinHeatSource::ResistanceHeater => {
                self.state.pwr_aux_for_cab_hvac_req.update(
                    *self
                        .state
                        .pwr_thrml_to_cab_req
                        .get_fresh(|| format_dbg!())?,
                    || format_dbg!(),
                )?;
            }
            CabinHeatSource::HeatPump => self.state.pwr_aux_for_cab_hvac_req.update(
                *self
                    .state
                    .pwr_thrml_to_cab_req
                    .get_fresh(|| format_dbg!())?
                    / self
                        .state
                        .cop
                        .get_fresh(|| format_dbg!())?
                        .with_context(|| {
                            format!(
                                "{}\nExpected `self.state.cop` to be Some.",
                                format_dbg!(self.state.cop)
                            )
                        })?,
                || format_dbg!(),
            )?,
        }
        if *self
            .state
            .pwr_aux_for_cab_hvac_req
            .get_fresh(|| format_dbg!())?
            > self.pwr_aux_for_hvac_cab_max
        {
            self.state
                .pwr_aux_for_cab_hvac
                .update(self.pwr_aux_for_hvac_cab_max, || format_dbg!())?;
            self.state.pwr_thrml_hvac_to_cabin.update(
                match self.cabin_heat_source {
                    CabinHeatSource::FuelConverter => {
                        bail!("{}\nThis should be unreachable", format_dbg!());
                    }
                    CabinHeatSource::ResistanceHeater => *self
                        .state
                        .pwr_aux_for_cab_hvac
                        .get_fresh(|| format_dbg!())?,
                    CabinHeatSource::HeatPump => {
                        *self
                            .state
                            .pwr_aux_for_cab_hvac
                            .get_fresh(|| format_dbg!())?
                            * self
                                .state
                                .cop
                                .get_fresh(|| format_dbg!())?
                                .with_context(|| {
                                    format!(
                                        "{}\nExpected `self.state.cop` to be Some.",
                                        format_dbg!(self.state.cop)
                                    )
                                })?
                    }
                },
                || format_dbg!(),
            )?;
        } else {
            self.state.pwr_aux_for_cab_hvac.update(
                *self
                    .state
                    .pwr_aux_for_cab_hvac_req
                    .get_fresh(|| format_dbg!())?,
                || format_dbg!(),
            )?;
            self.state.pwr_thrml_hvac_to_cabin.update(
                *self
                    .state
                    .pwr_thrml_to_cab_req
                    .get_fresh(|| format_dbg!())?,
                || format_dbg!(),
            )?;
        }
        if self.cabin_heat_source.is_fuel_converter() {
            self.state.pwr_thrml_fc_to_cabin.update(
                *self
                    .state
                    .pwr_thrml_hvac_to_cabin
                    .get_fresh(|| format_dbg!())?,
                || format_dbg!(),
            )?;
        } else {
            self.state
                .pwr_thrml_fc_to_cabin
                .update(si::Power::ZERO, || format_dbg!())?;
        };
        Ok(())
    }

    fn set_cab_cntrl_state(
        &mut self,
        cab_state: &LumpedCabinState,
        dt: si::Time,
        te_set_cab: si::Temperature,
        hvac_mode: HvacMode,
    ) -> anyhow::Result<()> {
        let te_delta_vs_set_cab = (cab_state
            .temperature
            .get_stale(|| format_dbg!())?
            .get::<si::degree_celsius>()
            - te_set_cab.get::<si::degree_celsius>())
            * uc::KELVIN_INT;

        self.state
            .pwr_p_cab
            .update(-self.p_cabin * te_delta_vs_set_cab, || format_dbg!())?;
        let pwr_i_cab_prev = *self.state.pwr_i_cab.get_stale(|| format_dbg!())?;

        if (pwr_i_cab_prev > si::Power::ZERO && hvac_mode.is_cooling())
            || (pwr_i_cab_prev < si::Power::ZERO && hvac_mode.is_heating())
        {
            // pwr_i_cab is heating and mode is cooling
            // or
            // pwr_i_cab is cooling and mode is heating
            self.state
                .pwr_i_cab
                .update(si::Power::ZERO, || format_dbg!())?;
        } else {
            // pwr_i_cab is cooling and mode is cooling
            // or
            // pwr_i_cab is heating and mode is heating
            let pwr_i_cab_new = (pwr_i_cab_prev
                + -self.i_cabin * uc::W / uc::KELVIN / uc::S * te_delta_vs_set_cab * dt)
                .max(-self.pwr_i_max_cabin)
                .min(self.pwr_i_max_cabin);
            self.state
                .pwr_i_cab
                .update(pwr_i_cab_new, || format_dbg!())?;
        }

        self.state.pwr_d_cab.update(
            -self.d_cabin * uc::J / uc::KELVIN
                * ((cab_state
                    .temperature
                    .get_stale(|| format_dbg!())?
                    .get::<si::degree_celsius>()
                    - cab_state
                        .temp_prev
                        .get_stale(|| format_dbg!())?
                        .get::<si::degree_celsius>())
                    * uc::KELVIN_INT
                    / dt),
            || format_dbg!(),
        )?;
        Ok(())
    }

    fn set_res_cntrl_state(
        &mut self,
        res_thrml_state: &RESLumpedThermalState,
        dt: si::Time,
        te_set_res: si::Temperature,
        hvac_mode: HvacMode,
    ) -> anyhow::Result<()> {
        let te_delta_vs_set_res = (res_thrml_state
            .temperature
            .get_stale(|| format_dbg!())?
            .get::<si::degree_celsius>()
            - te_set_res.get::<si::degree_celsius>())
            * uc::KELVIN_INT;

        self.state
            .pwr_p_res
            .update(-self.p_res * te_delta_vs_set_res, || format_dbg!())?;
        let pwr_i_res_prev = *self.state.pwr_i_res.get_stale(|| format_dbg!())?;
        if (pwr_i_res_prev > si::Power::ZERO && hvac_mode.is_cooling())
            || (pwr_i_res_prev < si::Power::ZERO && hvac_mode.is_heating())
        {
            // pwr_i_res is heating and mode is cooling
            // or
            // pwr_i_res is cooling and mode is heating
            self.state
                .pwr_i_res
                .update(si::Power::ZERO, || format_dbg!())?;
        } else {
            // pwr_i_res is cooling and mode is cooling
            // or
            // pwr_i_res is heating and mode is heating
            self.state.pwr_i_res.increment(
                (-self.i_res * uc::W / uc::KELVIN / uc::S * te_delta_vs_set_res * dt)
                    .max(-self.pwr_i_max_res)
                    .min(self.pwr_i_max_res),
                || format_dbg!(),
            )?;
        }
        self.state.pwr_d_res.update(
            -self.d_res * uc::J / uc::KELVIN
                * ((res_thrml_state
                    .temperature
                    .get_stale(|| format_dbg!())?
                    .get::<si::degree_celsius>()
                    - res_thrml_state
                        .temp_prev
                        .get_stale(|| format_dbg!())?
                        .get::<si::degree_celsius>())
                    * uc::KELVIN_INT
                    / dt),
            || format_dbg!(),
        )?;
        Ok(())
    }

    // Solve for thermal power for [RESLumpedThermal]
    fn solve_for_res(
        &mut self,
        res_thrml_state: &RESLumpedThermalState,
        dt: si::Time,
    ) -> anyhow::Result<()> {
        match self.te_set_res {
            Some(te_set_res) => {
                match self.state.res_mode.get_fresh(|| format_dbg!())? {
                    HvacMode::InsideDeadband => {
                        // inside deadband; no hvac power is needed
                        self.state
                            .pwr_i_res
                            .update(si::Power::ZERO, || format_dbg!())?;
                        self.state
                            .pwr_p_res
                            .update(si::Power::ZERO, || format_dbg!())?;
                        self.state
                            .pwr_d_res
                            .update(si::Power::ZERO, || format_dbg!())?;
                        self.state
                            .pwr_aux_for_res_hvac_req
                            .update(si::Power::ZERO, || format_dbg!())?;
                        self.state
                            .pwr_aux_for_res_hvac
                            .update(si::Power::ZERO, || format_dbg!())?;
                        self.state
                            .pwr_thrml_to_res_req
                            .update(si::Power::ZERO, || format_dbg!())?;
                        self.state
                            .pwr_thrml_hvac_to_res
                            .update(si::Power::ZERO, || format_dbg!())?;
                    }
                    HvacMode::Cooling => {
                        self.set_res_cntrl_state(
                            res_thrml_state,
                            dt,
                            te_set_res,
                            HvacMode::Cooling,
                        )?;

                        if *self.state.pwr_i_res.get_fresh(|| format_dbg!())? > si::Power::ZERO {
                            // If `pwr_i_res` is greater than zero, reset to switch from heating to cooling
                            self.state
                                .pwr_i_res
                                .update_unchecked(si::Power::ZERO, || format_dbg!())?;
                        }
                        self.state.pwr_thrml_to_res_req.update(
                            (*self.state.pwr_p_res.get_fresh(|| format_dbg!())?
                                + *self.state.pwr_i_res.get_fresh(|| format_dbg!())?
                                + *self.state.pwr_d_res.get_fresh(|| format_dbg!())?)
                            .max(-self.pwr_thrml_max),
                            || format_dbg!(),
                        )?;
                        ensure!(
                            *self
                                .state
                                .pwr_thrml_to_res_req
                                .get_fresh(|| format_dbg!())?
                                < si::Power::ZERO,
                            "{}\nHVAC should be cooling RES",
                            format_dbg!(*self
                                .state
                                .pwr_thrml_to_res_req
                                .get_fresh(|| format_dbg!())?)
                        );
                        ensure!(
                            *self
                                .state
                                .pwr_thrml_to_res_req
                                .get_fresh(|| format_dbg!())?
                                < si::Power::ZERO,
                            "HVAC should be cooling RES\n{}\n{}\n{}",
                            format_dbg!(*self
                                .state
                                .pwr_thrml_to_res_req
                                .get_fresh(|| format_dbg!())?),
                            format_dbg!(*self
                                .state
                                .pwr_aux_for_res_hvac
                                .get_fresh(|| format_dbg!())?),
                            format_dbg!(*self.state.cop.get_fresh(|| format_dbg!())?)
                        );
                        self.state.pwr_aux_for_res_hvac_req.update(
                            match self.res_cooling_source {
                                RESCoolingSource::HVAC => {
                                    -*self
                                        .state
                                        .pwr_thrml_to_res_req
                                        .get_fresh(|| format_dbg!())?
                                        / self.state.cop.get_fresh(|| format_dbg!())?.with_context(
                                            || {
                                                format!(
                                                    "{}\nExpected `self.state.cop` to be Some.",
                                                    format_dbg!(self.state.cop)
                                                )
                                            },
                                        )?
                                }
                                RESCoolingSource::None => si::Power::ZERO,
                            },
                            || format_dbg!(),
                        )?;

                        // Correct aux power components to account for any limit violations
                        if *self
                            .state
                            .pwr_aux_for_res_hvac_req
                            .get_fresh(|| format_dbg!())?
                            > self.pwr_aux_for_hvac_res_max
                        {
                            self.state
                                .pwr_aux_for_res_hvac
                                .update(self.pwr_aux_for_hvac_res_max, || format_dbg!())?;
                            self.state.pwr_thrml_hvac_to_res.update(
                                match self.res_cooling_source {
                                    RESCoolingSource::HVAC => {
                                        -*self
                                            .state
                                            .pwr_aux_for_res_hvac
                                            .get_fresh(|| format_dbg!())?
                                            * self
                                                .state
                                                .cop
                                                .get_fresh(|| format_dbg!())?
                                                .with_context(|| {
                                                    format!(
                                                        "{}\nExpected `self.state.cop` to be Some.",
                                                        format_dbg!(self.state.cop)
                                                    )
                                                })?
                                    }
                                    RESCoolingSource::None => {
                                        bail!("{}\nThis should be unreachable", format_dbg!());
                                    }
                                },
                                || format_dbg!(),
                            )?;
                        } else {
                            self.state.pwr_aux_for_res_hvac.update(
                                *self
                                    .state
                                    .pwr_aux_for_res_hvac_req
                                    .get_fresh(|| format_dbg!())?,
                                || format_dbg!(),
                            )?;
                            self.state.pwr_thrml_hvac_to_res.update(
                                *self
                                    .state
                                    .pwr_thrml_to_res_req
                                    .get_fresh(|| format_dbg!())?,
                                || format_dbg!(),
                            )?;
                        };
                    }
                    HvacMode::Heating => {
                        self.set_res_cntrl_state(
                            res_thrml_state,
                            dt,
                            te_set_res,
                            HvacMode::Heating,
                        )?;

                        self.state.pwr_thrml_to_res_req.update(
                            {
                                if *self.state.pwr_i_res.get_fresh(|| format_dbg!())?
                                    < si::Power::ZERO
                                {
                                    // If `pwr_i_res` is less than zero reset to switch from cooling to heating
                                    self.state
                                        .pwr_i_res
                                        .update_unchecked(si::Power::ZERO, || format_dbg!())?;
                                }
                                (*self.state.pwr_p_res.get_fresh(|| format_dbg!())?
                                    + *self.state.pwr_i_res.get_fresh(|| format_dbg!())?
                                    + *self.state.pwr_d_res.get_fresh(|| format_dbg!())?)
                                .min(self.pwr_thrml_max)
                            },
                            || format_dbg!(),
                        )?;
                        ensure!(
                            *self
                                .state
                                .pwr_thrml_to_res_req
                                .get_fresh(|| format_dbg!())?
                                > si::Power::ZERO,
                            "{}\nHVAC should be heating RES",
                            format_dbg!(self.state.pwr_thrml_to_res_req)
                        );
                        ensure!(
                            *self
                                .state
                                .pwr_thrml_to_res_req
                                .get_fresh(|| format_dbg!())?
                                > si::Power::ZERO,
                            "HVAC should be heating RES\n{}\n{}\n{}",
                            format_dbg!(self.state.pwr_thrml_to_res_req),
                            format_dbg!(self.state.pwr_aux_for_res_hvac),
                            format_dbg!(self.state.cop)
                        );
                        self.state.pwr_aux_for_res_hvac_req.update(
                            match self.res_heat_source {
                                RESHeatSource::ResistanceHeater => *self
                                    .state
                                    .pwr_thrml_to_res_req
                                    .get_fresh(|| format_dbg!())?,
                                RESHeatSource::HeatPump => {
                                    *self
                                        .state
                                        .pwr_thrml_to_res_req
                                        .get_fresh(|| format_dbg!())?
                                        / self.state.cop.get_fresh(|| format_dbg!())?.with_context(
                                            || {
                                                format!(
                                                    "{}\nExpected `*self.state.cop` to be Some.",
                                                    format_dbg!(self.state.cop)
                                                )
                                            },
                                        )?
                                }
                                RESHeatSource::None => si::Power::ZERO,
                            },
                            || format_dbg!(),
                        )?;

                        // Correct aux power components to account for any limit violations
                        if *self
                            .state
                            .pwr_aux_for_res_hvac_req
                            .get_fresh(|| format_dbg!())?
                            > self.pwr_aux_for_hvac_res_max
                        {
                            self.state
                                .pwr_aux_for_res_hvac
                                .update(self.pwr_aux_for_hvac_res_max, || format_dbg!())?;
                            self.state.pwr_thrml_hvac_to_res.update(
                                match self.res_heat_source {
                                    RESHeatSource::ResistanceHeater => *self
                                        .state
                                        .pwr_aux_for_res_hvac
                                        .get_fresh(|| format_dbg!())?,
                                    RESHeatSource::HeatPump => {
                                        *self
                                            .state
                                            .pwr_aux_for_res_hvac
                                            .get_fresh(|| format_dbg!())?
                                            * self
                                                .state
                                                .cop
                                                .get_fresh(|| format_dbg!())?
                                                .with_context(|| {
                                                    format!(
                                                        "{}\nExpected `self.state.cop` to be Some.",
                                                        format_dbg!(self.state.cop)
                                                    )
                                                })?
                                    }
                                    RESHeatSource::None => {
                                        bail!("{}\nThis should be unreachable", format_dbg!());
                                    }
                                },
                                || format_dbg!(),
                            )?
                        } else {
                            self.state.pwr_aux_for_res_hvac.update(
                                *self
                                    .state
                                    .pwr_aux_for_res_hvac_req
                                    .get_fresh(|| format_dbg!())?,
                                || format_dbg!(),
                            )?;
                            self.state.pwr_thrml_hvac_to_res.update(
                                *self
                                    .state
                                    .pwr_thrml_to_res_req
                                    .get_fresh(|| format_dbg!())?,
                                || format_dbg!(),
                            )?;
                        };
                    }
                    HvacMode::Inactive => {
                        self.state
                            .pwr_i_res
                            .update(si::Power::ZERO, || format_dbg!())?;
                        self.state
                            .pwr_p_res
                            .update(si::Power::ZERO, || format_dbg!())?;
                        self.state
                            .pwr_d_res
                            .update(si::Power::ZERO, || format_dbg!())?;
                        self.state
                            .pwr_aux_for_res_hvac_req
                            .update(si::Power::ZERO, || format_dbg!())?;
                        self.state
                            .pwr_aux_for_res_hvac
                            .update(si::Power::ZERO, || format_dbg!())?;
                        self.state
                            .pwr_thrml_to_res_req
                            .update(si::Power::ZERO, || format_dbg!())?;
                        self.state
                            .pwr_thrml_hvac_to_res
                            .update(si::Power::ZERO, || format_dbg!())?;
                    }
                }
            }
            None => {
                self.state
                    .pwr_i_res
                    .update(si::Power::ZERO, || format_dbg!())?;
                self.state
                    .pwr_p_res
                    .update(si::Power::ZERO, || format_dbg!())?;
                self.state
                    .pwr_d_res
                    .update(si::Power::ZERO, || format_dbg!())?;
                self.state
                    .pwr_aux_for_res_hvac_req
                    .update(si::Power::ZERO, || format_dbg!())?;
                self.state
                    .pwr_aux_for_res_hvac
                    .update(si::Power::ZERO, || format_dbg!())?;
                self.state
                    .pwr_thrml_hvac_to_res
                    .update(si::Power::ZERO, || format_dbg!())?;
            }
        }

        Ok(())
    }
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
#[serde(default)]
#[cfg_attr(feature = "pyo3", pyclass(module = "fastsim", subclass, eq))]
#[serde(deny_unknown_fields)]
pub struct HVACSystemForLumpedCabinAndRESState {
    /// time step counter
    pub i: TrackedState<usize>,
    /// portion of total HVAC cooling/heating (negative/positive) power due to
    /// proportional gain
    pub pwr_p_cab: TrackedState<si::Power>,
    /// portion of total HVAC cooling/heating (negative/positive) cumulative energy
    /// due to proportional gain
    pub energy_p_cab: TrackedState<si::Energy>,
    /// portion of total HVAC cooling/heating (negative/positive) power due to
    /// integral gain
    pub pwr_i_cab: TrackedState<si::Power>,
    /// portion of total HVAC cooling/heating (negative/positive) cumulative energy
    /// due to integral gain
    pub energy_i_cab: TrackedState<si::Energy>,
    /// portion of total HVAC cooling/heating (negative/positive) power due to
    /// derivative gain
    pub pwr_d_cab: TrackedState<si::Power>,
    /// portion of total HVAC cooling/heating (negative/positive) cumulative energy
    /// due to derivative gain
    pub energy_d_cab: TrackedState<si::Energy>,
    /// portion of total HVAC cooling/heating (negative/positive) power to
    /// [ReversibleEnergyStorage::thrml] due to proportional gain
    pub pwr_p_res: TrackedState<si::Power>,
    /// portion of total HVAC cooling/heating (negative/positive) cumulative energy
    /// to [ReversibleEnergyStorage::thrml] due to proportional gain
    pub energy_p_res: TrackedState<si::Energy>,
    /// portion of total HVAC cooling/heating (negative/positive) power to
    /// [ReversibleEnergyStorage::thrml] due to integral gain
    pub pwr_i_res: TrackedState<si::Power>,
    /// portion of total HVAC cooling/heating (negative/positive) cumulative energy
    /// to [ReversibleEnergyStorage::thrml] due to integral gain
    pub energy_i_res: TrackedState<si::Energy>,
    /// portion of total HVAC cooling/heating (negative/positive) power to
    /// [ReversibleEnergyStorage::thrml] due to derivative gain
    pub pwr_d_res: TrackedState<si::Power>,
    /// portion of total HVAC cooling/heating (negative/positive) cumulative energy
    /// to [ReversibleEnergyStorage::thrml] due to derivative gain
    pub energy_d_res: TrackedState<si::Energy>,
    /// coefficient of performance (i.e. efficiency) of vapor compression cycle
    #[si_unit(unitless)]
    pub cop: TrackedState<Option<si::Ratio>>,
    /// Reference temperature used to calculate coefficient of performance (i.e.
    /// efficiency) of vapor compression cycle
    pub te_ref: TrackedState<Option<si::Temperature>>,
    /// Requested aux power demand from [Vehicle::hvac] system for cabin thermal
    /// managemement.
    pub pwr_aux_for_cab_hvac_req: TrackedState<si::Power>,
    /// Requested thermal power demand from [Vehicle::hvac] system for cabin thermal
    /// managemement.
    pub pwr_thrml_to_cab_req: TrackedState<si::Power>,
    /// Requested aux power demand from [Vehicle::hvac] system for res thermal
    /// managemement.
    pub pwr_aux_for_res_hvac_req: TrackedState<si::Power>,
    /// Requested thermal power demand from [Vehicle::hvac] system for res thermal
    /// managemement.
    pub pwr_thrml_to_res_req: TrackedState<si::Power>,
    /// Arbitrated power demand from [Vehicle::hvac] system for cabin thermal
    /// managemement
    ///
    /// NOTE: If battery is hot and cabin is cold or vice versa, this is an overestimate.
    pub pwr_aux_for_cab_hvac: TrackedState<si::Power>,
    /// Arbitrated cumulative energy demand from [Vehicle::hvac] system for cabin
    /// thermal managemement
    ///
    /// NOTE: If battery is hot and cabin is cold or vice versa, this is an overestimate.
    pub energy_aux_for_cab_hvac: TrackedState<si::Energy>,
    /// Arbitrated power demand from [Vehicle::hvac] system for res thermal
    /// managemement
    ///
    /// NOTE: If battery is hot and cabin is cold or vice versa, this is an overestimate.
    pub pwr_aux_for_res_hvac: TrackedState<si::Power>,
    /// Arbitrated cumulative energy demand from [Vehicle::hvac] system for res
    /// thermal managemement
    ///
    /// NOTE: If battery is hot and cabin is cold or vice versa, this is an overestimate.
    pub energy_aux_for_res_hvac: TrackedState<si::Energy>,
    /// Thermal power from HVAC system to cabin, positive is heating the cabin
    pub pwr_thrml_hvac_to_cabin: TrackedState<si::Power>,
    /// Cumulative thermal energy from HVAC system to cabin, positive is heating
    /// the cabin
    pub energy_thrml_hvac_to_cabin: TrackedState<si::Energy>,
    /// Thermal power from [FuelConverter] to [Cabin]
    pub pwr_thrml_fc_to_cabin: TrackedState<si::Power>,
    /// Cumulative thermal energy from [FuelConverter] to [Cabin]
    pub energy_thrml_fc_to_cabin: TrackedState<si::Energy>,
    /// Thermal power from HVAC to [ReversibleEnergyStorage]
    pub pwr_thrml_hvac_to_res: TrackedState<si::Power>,
    /// Cumulative thermal energy from HVAC to [ReversibleEnergyStorage]
    pub energy_thrml_hvac_to_res: TrackedState<si::Energy>,
    /// Component corresponding to [te_ref]
    pub te_ref_component: TrackedState<TeRefComp>,
    /// Current mode of cabin hvac
    pub cabin_mode: TrackedState<HvacMode>,
    /// Current mode of [ReversibleEnergyStorage] hvac
    pub res_mode: TrackedState<HvacMode>,
}
impl Init for HVACSystemForLumpedCabinAndRESState {}
impl SerdeAPI for HVACSystemForLumpedCabinAndRESState {}

#[pyo3_api]
impl HVACSystemForLumpedCabinAndRESState {}

impl HVACSystemForLumpedCabinAndRESState {
    fn set_mode_and_get_te_for_cop(
        &mut self,
        cab_state: &LumpedCabinState,
        res_thrml_state: &RESLumpedThermalState,
        te_deltas: (
            Option<si::TemperatureInterval>,
            si::TemperatureInterval,
            Option<si::TemperatureInterval>,
            si::TemperatureInterval,
        ),
        cab_params: (CabinHeatSource, si::TemperatureInterval),
        res_params: (RESHeatSource, RESCoolingSource, si::TemperatureInterval),
    ) -> anyhow::Result<(
        Option<si::Temperature>,
        Option<si::TemperatureInterval>,
        Option<si::TemperatureInterval>,
    )> {
        let (te_cab_delta_vs_set, te_cab_delta_vs_amb, te_res_delta_vs_set, te_res_delta_vs_amb) =
            te_deltas;

        let (cabin_heat_source, te_deadband_cab) = cab_params;
        let (res_heat_source, res_cooling_source, te_deadband_res) = res_params;

        // NOTE: if `_` is used, order of match statements has an effect on
        // which mode gets set because the `_` are greedy!
        match (te_res_delta_vs_set, te_cab_delta_vs_set) {
            (Some(te_res_delta_vs_set), Some(te_cab_delta_vs_set)) => {
                match (
                    te_res_delta_vs_set > si::TemperatureInterval::ZERO,
                    te_res_delta_vs_set.abs() > te_deadband_res,
                    te_cab_delta_vs_set > si::TemperatureInterval::ZERO,
                    te_cab_delta_vs_set.abs() > te_deadband_cab,
                ) {
                    (true, true, true, true) => {
                        // battery is hot and outside deadband
                        // cabin is hot and outside the deadband
                        self.cabin_mode
                            .update(HvacMode::Cooling, || format_dbg!())?;
                        let (res_mode, te_ref_component) = match res_cooling_source {
                            RESCoolingSource::HVAC => (
                                HvacMode::Cooling,
                                if te_res_delta_vs_amb.abs() > te_cab_delta_vs_amb.abs() {
                                    TeRefComp::RES
                                } else {
                                    TeRefComp::Cabin
                                },
                            ),
                            RESCoolingSource::None => (HvacMode::Inactive, TeRefComp::Cabin),
                        };
                        self.res_mode.update(res_mode, || format_dbg!())?;
                        self.te_ref_component
                            .update(te_ref_component, || format_dbg!())?;
                    }
                    (true, true, true, false) => {
                        // battery is hot and outside deadband
                        // cabin is hot and within the deadband
                        self.cabin_mode
                            .update(HvacMode::InsideDeadband, || format_dbg!())?;
                        let (res_mode, te_ref_component) = match res_cooling_source {
                            RESCoolingSource::HVAC => (HvacMode::Cooling, TeRefComp::RES),
                            RESCoolingSource::None => (HvacMode::Inactive, TeRefComp::None),
                        };
                        self.res_mode.update(res_mode, || format_dbg!())?;
                        self.te_ref_component
                            .update(te_ref_component, || format_dbg!())?;
                    }
                    (true, true, false, true) => {
                        // battery is hot and outside deadband
                        // cabin is cold and outside the deadband
                        self.cabin_mode
                            .update(HvacMode::Heating, || format_dbg!())?;
                        let (res_mode, te_ref_component) =
                            match (cabin_heat_source, res_cooling_source) {
                                (CabinHeatSource::HeatPump, RESCoolingSource::HVAC) => (
                                    HvacMode::Cooling,
                                    if te_res_delta_vs_amb.abs() > te_cab_delta_vs_amb.abs() {
                                        TeRefComp::RES
                                    } else {
                                        TeRefComp::Cabin
                                    },
                                ),
                                (CabinHeatSource::HeatPump, RESCoolingSource::None) => {
                                    (HvacMode::Inactive, TeRefComp::Cabin)
                                }
                                (CabinHeatSource::ResistanceHeater, RESCoolingSource::HVAC) => {
                                    (HvacMode::Cooling, TeRefComp::RES)
                                }
                                (CabinHeatSource::ResistanceHeater, RESCoolingSource::None) => {
                                    (HvacMode::Inactive, TeRefComp::None)
                                }
                                (CabinHeatSource::FuelConverter, RESCoolingSource::HVAC) => {
                                    (HvacMode::Cooling, TeRefComp::RES)
                                }
                                (CabinHeatSource::FuelConverter, RESCoolingSource::None) => {
                                    (HvacMode::Inactive, TeRefComp::None)
                                }
                            };
                        self.res_mode.update(res_mode, || format_dbg!())?;
                        self.te_ref_component
                            .update(te_ref_component, || format_dbg!())?;
                    }
                    (true, true, false, false) => {
                        // battery is hot and outside deadband
                        // cabin is cold and within the deadband
                        self.cabin_mode
                            .update(HvacMode::InsideDeadband, || format_dbg!())?;
                        let (res_mode, te_ref_component) = match res_cooling_source {
                            RESCoolingSource::HVAC => (HvacMode::Cooling, TeRefComp::RES),
                            RESCoolingSource::None => (HvacMode::Inactive, TeRefComp::None),
                        };
                        self.res_mode.update(res_mode, || format_dbg!())?;
                        self.te_ref_component
                            .update(te_ref_component, || format_dbg!())?;
                    }
                    (true, false, true, true) => {
                        // battery is hot and within the deadband
                        // cabin is hot and outside the deadband
                        self.res_mode.update(
                            match res_cooling_source {
                                RESCoolingSource::HVAC => HvacMode::InsideDeadband,
                                RESCoolingSource::None => HvacMode::Inactive,
                            },
                            || format_dbg!(),
                        )?;
                        self.cabin_mode
                            .update(HvacMode::Cooling, || format_dbg!())?;
                        self.te_ref_component
                            .update(TeRefComp::Cabin, || format_dbg!())?;
                    }
                    (true, false, false, true) => {
                        // battery is hot and within deadband
                        // cabin is cold and outside the deadband
                        self.res_mode.update(
                            match res_cooling_source {
                                RESCoolingSource::HVAC => HvacMode::InsideDeadband,
                                RESCoolingSource::None => HvacMode::Inactive,
                            },
                            || format_dbg!(),
                        )?;
                        self.cabin_mode
                            .update(HvacMode::Heating, || format_dbg!())?;
                        self.te_ref_component.update(
                            match cabin_heat_source {
                                CabinHeatSource::HeatPump => TeRefComp::Cabin,
                                _ => TeRefComp::None,
                            },
                            || format_dbg!(),
                        )?;
                    }
                    (true, false, true, false) => {
                        // battery is hot and within the deadband
                        // cabin is hot  and within the deadband
                        self.cabin_mode
                            .update(HvacMode::InsideDeadband, || format_dbg!())?;
                        self.res_mode.update(
                            match res_cooling_source {
                                RESCoolingSource::HVAC => HvacMode::InsideDeadband,
                                RESCoolingSource::None => HvacMode::Inactive,
                            },
                            || format_dbg!(),
                        )?;
                        self.te_ref_component
                            .update(TeRefComp::None, || format_dbg!())?;
                    }
                    (true, false, false, false) => {
                        // battery is hot and within the deadband
                        // cabin is cold and within the deadband
                        self.cabin_mode
                            .update(HvacMode::InsideDeadband, || format_dbg!())?;
                        self.res_mode.update(
                            match res_cooling_source {
                                RESCoolingSource::HVAC => HvacMode::InsideDeadband,
                                RESCoolingSource::None => HvacMode::Inactive,
                            },
                            || format_dbg!(),
                        )?;
                        self.te_ref_component
                            .update(TeRefComp::None, || format_dbg!())?;
                    }
                    (false, true, true, true) => {
                        // battery is cold and outside deadband
                        // cabin is hot and outside the deadband
                        self.cabin_mode
                            .update(HvacMode::Cooling, || format_dbg!())?;
                        let (res_mode, te_ref_component) = match res_heat_source {
                            RESHeatSource::HeatPump => (
                                HvacMode::Heating,
                                if te_res_delta_vs_amb.abs() > te_cab_delta_vs_amb.abs() {
                                    TeRefComp::RES
                                } else {
                                    TeRefComp::Cabin
                                },
                            ),
                            RESHeatSource::ResistanceHeater => {
                                (HvacMode::Heating, TeRefComp::Cabin)
                            }
                            RESHeatSource::None => (HvacMode::Inactive, TeRefComp::Cabin),
                        };
                        self.res_mode.update(res_mode, || format_dbg!())?;
                        self.te_ref_component
                            .update(te_ref_component, || format_dbg!())?;
                    }
                    (false, true, true, false) => {
                        // battery is cold and outside the deadband
                        // cabin is hot and within the deadband
                        self.cabin_mode
                            .update(HvacMode::InsideDeadband, || format_dbg!())?;
                        let (res_mode, te_ref_component) = match res_heat_source {
                            RESHeatSource::HeatPump => (HvacMode::Heating, TeRefComp::RES),
                            RESHeatSource::ResistanceHeater => (HvacMode::Heating, TeRefComp::None),
                            RESHeatSource::None => (HvacMode::Inactive, TeRefComp::None),
                        };
                        self.res_mode.update(res_mode, || format_dbg!())?;
                        self.te_ref_component
                            .update(te_ref_component, || format_dbg!())?;
                    }
                    (false, true, false, true) => {
                        // battery is cold and outside deadband
                        // cabin is cold and outside the deadband
                        self.cabin_mode
                            .update(HvacMode::Heating, || format_dbg!())?;
                        let (res_mode, te_ref_component) =
                            match (cabin_heat_source, res_heat_source) {
                                (CabinHeatSource::HeatPump, RESHeatSource::HeatPump) => (
                                    HvacMode::Heating,
                                    if te_res_delta_vs_amb.abs() > te_cab_delta_vs_amb.abs() {
                                        TeRefComp::RES
                                    } else {
                                        TeRefComp::Cabin
                                    },
                                ),
                                (CabinHeatSource::HeatPump, RESHeatSource::ResistanceHeater) => {
                                    (HvacMode::Heating, TeRefComp::Cabin)
                                }
                                (CabinHeatSource::HeatPump, RESHeatSource::None) => {
                                    (HvacMode::Inactive, TeRefComp::Cabin)
                                }
                                (CabinHeatSource::FuelConverter, RESHeatSource::HeatPump) => {
                                    (HvacMode::Heating, TeRefComp::RES)
                                }
                                (
                                    CabinHeatSource::FuelConverter,
                                    RESHeatSource::ResistanceHeater,
                                ) => (HvacMode::Heating, TeRefComp::None),
                                (CabinHeatSource::FuelConverter, RESHeatSource::None) => {
                                    (HvacMode::Inactive, TeRefComp::None)
                                }
                                (CabinHeatSource::ResistanceHeater, RESHeatSource::HeatPump) => {
                                    (HvacMode::Heating, TeRefComp::RES)
                                }
                                (
                                    CabinHeatSource::ResistanceHeater,
                                    RESHeatSource::ResistanceHeater,
                                ) => (HvacMode::Heating, TeRefComp::None),
                                (CabinHeatSource::ResistanceHeater, RESHeatSource::None) => {
                                    (HvacMode::Inactive, TeRefComp::None)
                                }
                            };
                        self.res_mode.update(res_mode, || format_dbg!())?;
                        self.te_ref_component
                            .update(te_ref_component, || format_dbg!())?;
                    }
                    (false, true, false, false) => {
                        // battery is cold and outside deadband
                        // cabin is cold and within the deadband
                        self.cabin_mode
                            .update(HvacMode::InsideDeadband, || format_dbg!())?;
                        let (res_mode, te_ref_component) = match res_heat_source {
                            RESHeatSource::HeatPump => (
                                HvacMode::Heating,
                                if te_res_delta_vs_amb.abs() > te_cab_delta_vs_amb.abs() {
                                    TeRefComp::RES
                                } else {
                                    TeRefComp::Cabin
                                },
                            ),
                            RESHeatSource::ResistanceHeater => {
                                (HvacMode::Heating, TeRefComp::Cabin)
                            }
                            RESHeatSource::None => (HvacMode::Inactive, TeRefComp::Cabin),
                        };
                        self.res_mode.update(res_mode, || format_dbg!())?;
                        self.te_ref_component
                            .update(te_ref_component, || format_dbg!())?;
                    }
                    (false, false, true, true) => {
                        // battery is cold and within deadband
                        // cabin is hot and outside the deadband
                        self.cabin_mode
                            .update(HvacMode::Cooling, || format_dbg!())?;
                        self.res_mode.update(
                            match res_heat_source {
                                RESHeatSource::HeatPump => HvacMode::InsideDeadband,
                                RESHeatSource::ResistanceHeater => HvacMode::InsideDeadband,
                                RESHeatSource::None => HvacMode::Inactive,
                            },
                            || format_dbg!(),
                        )?;
                        self.te_ref_component
                            .update(TeRefComp::Cabin, || format_dbg!())?;
                    }
                    (false, false, true, false) => {
                        // battery is cold and within deadband
                        // cabin is hot and within the deadband
                        self.cabin_mode
                            .update(HvacMode::InsideDeadband, || format_dbg!())?;
                        self.res_mode.update(
                            match res_heat_source {
                                RESHeatSource::HeatPump => HvacMode::InsideDeadband,
                                RESHeatSource::ResistanceHeater => HvacMode::InsideDeadband,
                                RESHeatSource::None => HvacMode::Inactive,
                            },
                            || format_dbg!(),
                        )?;
                        self.te_ref_component
                            .update(TeRefComp::None, || format_dbg!())?;
                    }
                    (false, false, false, true) => {
                        // battery is cold and within deadband
                        // cabin is cold and outside the deadband
                        self.res_mode.update(
                            match res_heat_source {
                                RESHeatSource::HeatPump => HvacMode::InsideDeadband,
                                RESHeatSource::ResistanceHeater => HvacMode::InsideDeadband,
                                RESHeatSource::None => HvacMode::Inactive,
                            },
                            || format_dbg!(),
                        )?;
                        let (cabin_mode, te_ref_component) = match cabin_heat_source {
                            CabinHeatSource::HeatPump => (HvacMode::Heating, TeRefComp::Cabin),
                            _ => (HvacMode::Heating, TeRefComp::None),
                        };
                        self.cabin_mode.update(cabin_mode, || format_dbg!())?;
                        self.te_ref_component
                            .update(te_ref_component, || format_dbg!())?;
                    }
                    (false, false, false, false) => {
                        // battery is cold and within deadband
                        // cabin is cold and within the deadband
                        self.cabin_mode
                            .update(HvacMode::InsideDeadband, || format_dbg!())?;
                        self.res_mode.update(
                            match res_heat_source {
                                RESHeatSource::HeatPump => HvacMode::InsideDeadband,
                                RESHeatSource::ResistanceHeater => HvacMode::InsideDeadband,
                                RESHeatSource::None => HvacMode::Inactive,
                            },
                            || format_dbg!(),
                        )?;
                        self.te_ref_component
                            .update(TeRefComp::None, || format_dbg!())?;
                    }
                }
            }
            (Some(te_res_delta_vs_set), None) => {
                // positive and outside the deadband
                self.cabin_mode
                    .update(HvacMode::Inactive, || format_dbg!())?;
                let (res_mode, te_ref_component) = if te_res_delta_vs_set > te_deadband_res {
                    // positive  - i.e. cooling mode
                    match res_cooling_source {
                        RESCoolingSource::HVAC => (HvacMode::Cooling, TeRefComp::RES),
                        RESCoolingSource::None => (HvacMode::Inactive, TeRefComp::None),
                    }
                } else if te_res_delta_vs_set < -te_deadband_res {
                    // negative -- i.e. heating mode
                    match res_heat_source {
                        RESHeatSource::HeatPump => (HvacMode::Heating, TeRefComp::RES),
                        RESHeatSource::ResistanceHeater => (HvacMode::Heating, TeRefComp::None),
                        RESHeatSource::None => (HvacMode::Inactive, TeRefComp::None),
                    }
                } else {
                    (
                        // actual temperature is within deadband range of setpoint temperature
                        HvacMode::InsideDeadband,
                        TeRefComp::None,
                    )
                };
                self.res_mode.update(res_mode, || format_dbg!())?;
                self.te_ref_component
                    .update(te_ref_component, || format_dbg!())?;
            }
            (None, Some(te_cab_delta_vs_set)) => {
                self.res_mode.update(HvacMode::Inactive, || format_dbg!())?;
                // positive and outside the deadband
                let (cabin_mode, te_ref_component) = if te_cab_delta_vs_set > te_deadband_cab {
                    (HvacMode::Cooling, TeRefComp::Cabin)
                } else if te_cab_delta_vs_set < -te_deadband_cab {
                    (
                        HvacMode::Heating,
                        // negative -- i.e. heating mode
                        match cabin_heat_source {
                            CabinHeatSource::HeatPump => TeRefComp::Cabin,
                            _ => TeRefComp::None,
                        },
                    )
                } else {
                    (
                        // actual temperature is within deadband range of setpoint temperature
                        HvacMode::InsideDeadband,
                        TeRefComp::None,
                    )
                };
                self.cabin_mode.update(cabin_mode, || format_dbg!())?;
                self.te_ref_component
                    .update(te_ref_component, || format_dbg!())?;
            }
            (None, None) => {
                // thermal management is totally inactive
                self.cabin_mode
                    .update(HvacMode::Inactive, || format_dbg!())?;
                self.res_mode.update(HvacMode::Inactive, || format_dbg!())?;
                self.te_ref_component
                    .update(TeRefComp::None, || format_dbg!())?;
            }
        }

        let (te_ref, te_ref_delta_vs_set, te_ref_delta_vs_amb): (
            Option<si::Temperature>,
            Option<si::TemperatureInterval>,
            Option<si::TemperatureInterval>,
        ) = match self.te_ref_component.get_fresh(|| format_dbg!())? {
            TeRefComp::Cabin => (
                Some(*cab_state.temperature.get_stale(|| format_dbg!())?),
                te_cab_delta_vs_set,
                Some(te_cab_delta_vs_amb),
            ),
            TeRefComp::RES => (
                Some(*res_thrml_state.temperature.get_stale(|| format_dbg!())?),
                te_res_delta_vs_set,
                Some(te_res_delta_vs_amb),
            ),
            TeRefComp::None => (None, None, None),
        };

        Ok((te_ref, te_ref_delta_vs_set, te_ref_delta_vs_amb))
    }
}

#[derive(
    Clone, Copy, Debug, Deserialize, Serialize, PartialEq, IsVariant, derive_more::From, TryInto,
)]
/// Heat source for [RESLumpedThermal]
pub enum RESHeatSource {
    /// Resistance heater provides heat for HVAC system
    ResistanceHeater,
    /// Heat pump provides heat for HVAC system
    HeatPump,
    /// The battery is not actively heated
    None,
}
impl Init for RESHeatSource {}
impl SerdeAPI for RESHeatSource {}

#[derive(
    Clone, Copy, Debug, Deserialize, Serialize, PartialEq, IsVariant, derive_more::From, TryInto,
)]
/// Cooling source for [RESLumpedThermal]
pub enum RESCoolingSource {
    /// Vapor compression system used for cabin HVAC also cools [RESLumpedThermal]
    HVAC,
    /// [RESLumpedThermal] is not actively cooled
    None,
}
impl Init for RESCoolingSource {}
impl SerdeAPI for RESCoolingSource {}

#[derive(
    Clone,
    Copy,
    Default,
    Debug,
    Deserialize,
    Serialize,
    PartialEq,
    IsVariant,
    derive_more::From,
    TryInto,
)]
/// Component used for reference temperature in calulating [HVACSystemForLumpedCabinAndRESState::cop]
pub enum TeRefComp {
    /// Cabin
    Cabin,
    /// ReversibleEnergyStorage
    RES,
    /// Vapor compression system is inactive
    #[default]
    None,
}
impl Init for TeRefComp {}
impl SerdeAPI for TeRefComp {}
