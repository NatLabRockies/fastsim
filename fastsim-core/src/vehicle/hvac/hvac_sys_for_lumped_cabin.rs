use super::*;

#[serde_api]
#[derive(Deserialize, Serialize, Debug, Clone, PartialEq, StateMethods, SetCumulative)]
#[cfg_attr(feature = "pyo3", pyclass(module = "fastsim", subclass, eq))]
#[serde(deny_unknown_fields)]
/// HVAC system for [LumpedCabin]
pub struct HVACSystemForLumpedCabin {
    /// set point temperature, `None` means HVAC is inactive
    pub te_set: Option<si::Temperature>,
    /// deadband range.  any cabin temperature within this range of
    /// `te_set` results in no HVAC power draw
    pub te_deadband: si::TemperatureInterval,
    /// HVAC proportional gain
    pub p: si::ThermalConductance,
    /// HVAC integral gain [W / K / s], resets at zero crossing events
    /// NOTE: `uom` crate does not have this unit, but it may be possible to make a custom unit for this
    pub i: f64,
    /// value at which state.i stops accumulating
    pub pwr_i_max: si::Power,
    /// HVAC derivative gain [W / K * s]  
    /// NOTE: `uom` crate does not have this unit, but it may be possible to make a custom unit for this
    pub d: f64,
    /// max HVAC thermal power
    pub pwr_thrml_max: si::Power,
    /// coefficient between 0 and 1 to calculate HVAC efficiency by multiplying by
    /// coefficient of performance (COP)
    pub frac_of_ideal_cop: f64,
    /// heat source
    pub heat_source: CabinHeatSource,
    /// max allowed aux load for HVAC
    pub pwr_aux_for_hvac_max: si::Power,
    /// coefficient of performance of vapor compression cycle
    #[serde(default)]
    #[is_state]
    pub state: HVACSystemForLumpedCabinState,
    #[serde(
        default,
        skip_serializing_if = "HVACSystemForLumpedCabinStateHistoryVec::is_empty"
    )]
    pub history: HVACSystemForLumpedCabinStateHistoryVec,
    pub save_interval: Option<usize>,
}

#[pyo3_api]
impl HVACSystemForLumpedCabin {
    #[staticmethod]
    #[pyo3(name = "default")]
    fn default_py() -> Self {
        Default::default()
    }
}
impl Default for HVACSystemForLumpedCabin {
    fn default() -> Self {
        Self {
            te_set: Some(*TE_STD_AIR),
            te_deadband: 0.5 * uc::KELVIN_INT,
            p: Default::default(),
            i: Default::default(),
            d: Default::default(),
            pwr_i_max: 5. * uc::KW,
            pwr_thrml_max: 10. * uc::KW,
            frac_of_ideal_cop: 0.15,
            heat_source: CabinHeatSource::ResistanceHeater,
            pwr_aux_for_hvac_max: uc::KW * 5.,
            state: Default::default(),
            history: Default::default(),
            save_interval: Some(1),
        }
    }
}
impl Init for HVACSystemForLumpedCabin {}
impl SerdeAPI for HVACSystemForLumpedCabin {}
impl HistoryMethods for HVACSystemForLumpedCabin {
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
impl HVACSystemForLumpedCabin {
    /// # Arguments
    /// - `te_amb_air`: ambient air temperature
    /// - `te_fc`: [FuelConverter] temperature, if equipped
    /// - `cab_state`: [LumpedCabinState]
    /// - `cab_heat_cap`: cabin heat capacitance
    /// - `dt`: simulation time step size
    /// # Returns
    /// - `pwr_thrml_hvac_to_cabin`: thermal power flow from [Vehicle::hvac] system to cabin
    /// - `pwr_thrml_fc_to_cabin`: thermal power flow from [FuelConverter] to cabin via HVAC system
    pub fn solve(
        &mut self,
        te_amb_air: si::Temperature,
        te_fc: Option<si::Temperature>,
        cab_state: &LumpedCabinState,
        cab_heat_cap: si::HeatCapacity,
        dt: si::Time,
    ) -> anyhow::Result<(si::Power, si::Power)> {
        match self.te_set {
            None => {
                self.state.pwr_i.update(si::Power::ZERO, || format_dbg!())?; // reset to 0.0
                self.state.pwr_p.update(si::Power::ZERO, || format_dbg!())?;
                self.state.pwr_d.update(si::Power::ZERO, || format_dbg!())?;
                self.state.cop.update(None, || format_dbg!())?;
                self.state
                    .pwr_thrml_hvac_to_cabin
                    .update(si::Power::ZERO, || format_dbg!())?;
                self.state
                    .pwr_thrml_fc_to_cabin
                    .update(si::Power::ZERO, || format_dbg!())?;
            }
            Some(te_set) => match (
                *cab_state.temperature.get_stale(|| format_dbg!())? > te_set + self.te_deadband,
                *cab_state.temperature.get_stale(|| format_dbg!())? < te_set - self.te_deadband,
            ) {
                (false, false) => {
                    // inside deadband; no hvac power is needed

                    self.state.pwr_i.update(si::Power::ZERO, || format_dbg!())?; // reset to 0.0
                    self.state.pwr_p.update(si::Power::ZERO, || format_dbg!())?;
                    self.state.pwr_d.update(si::Power::ZERO, || format_dbg!())?;
                    self.state
                        .pwr_thrml_hvac_to_cabin
                        .update(si::Power::ZERO, || format_dbg!())?;
                    self.state
                        .pwr_thrml_fc_to_cabin
                        .update(si::Power::ZERO, || format_dbg!())?;
                    self.state
                        .pwr_aux_for_hvac
                        .update(si::Power::ZERO, || format_dbg!())?;
                    self.state.cop.update(None, || format_dbg!())?;
                }
                (true, false) => {
                    // outside deadband and cooling cabin
                    let te_delta_vs_set = (cab_state
                        .temperature
                        .get_stale(|| format_dbg!())?
                        .get::<si::degree_celsius>()
                        - te_set.get::<si::degree_celsius>())
                        * uc::KELVIN_INT;
                    let te_delta_vs_amb: si::TemperatureInterval = (cab_state
                        .temperature
                        .get_stale(|| format_dbg!())?
                        .get::<si::degree_celsius>()
                        - te_amb_air.get::<si::degree_celsius>())
                        * uc::KELVIN_INT;

                    self.state
                        .pwr_p
                        .update(-self.p * te_delta_vs_set, || format_dbg!())?;
                    ensure!(
                        *self.state.pwr_p.get_fresh(|| format_dbg!())? != si::Power::ZERO,
                        format_dbg!()
                    );
                    self.state.pwr_i.increment(
                        (-self.i * uc::W / uc::KELVIN / uc::S * te_delta_vs_set * dt)
                            .max(-self.pwr_i_max)
                            .min(self.pwr_i_max),
                        || format_dbg!(),
                    )?;
                    ensure!(
                        *self.state.pwr_i.get_fresh(|| format_dbg!())? != si::Power::ZERO,
                        format_dbg!()
                    );
                    self.state.pwr_d.update(
                        -self.d * uc::J / uc::KELVIN
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

                    // https://en.wikipedia.org/wiki/Coefficient_of_performance#Theoretical_performance_limits
                    // cop_ideal is t_h / (t_h - t_c) for heating
                    // cop_ideal is t_c / (t_h - t_c) for cooling

                    // divide-by-zero protection and realistic limit on COP
                    let cop_ideal = if -te_delta_vs_amb < 5.0 * uc::KELVIN_INT {
                        // cabin is cooler than ambient + threshold
                        // TODO: make this `5.0` not hardcoded
                        *cab_state.temperature.get_stale(|| format_dbg!())? / (5.0 * uc::KELVIN)
                    } else {
                        *cab_state.temperature.get_stale(|| format_dbg!())? / te_delta_vs_amb.abs()
                    };
                    let cop = cop_ideal * self.frac_of_ideal_cop;
                    ensure!(cop > 0.0 * uc::R, format_dbg!(cop));

                    if *self.state.pwr_i.get_fresh(|| format_dbg!())? > si::Power::ZERO {
                        // If `pwr_i` is greater than zero, reset to switch from heating to cooling
                        self.state
                            .pwr_i
                            .update_unchecked(si::Power::ZERO, || format_dbg!())?;
                    }
                    let mut pwr_thrml_hvac_to_cab =
                        (*self.state.pwr_p.get_fresh(|| format_dbg!())?
                            + *self.state.pwr_i.get_fresh(|| format_dbg!())?
                            + *self.state.pwr_d.get_fresh(|| format_dbg!())?)
                        .max(-self.pwr_thrml_max);

                    ensure!(
                        pwr_thrml_hvac_to_cab < si::Power::ZERO,
                        "{}\nHVAC should be cooling cabin",
                        format_dbg!(pwr_thrml_hvac_to_cab)
                    );

                    if (pwr_thrml_hvac_to_cab * cop).abs() > self.pwr_aux_for_hvac_max {
                        self.state
                            .pwr_aux_for_hvac
                            .update(self.pwr_aux_for_hvac_max, || format_dbg!())?;
                        // correct if limit is exceeded
                        pwr_thrml_hvac_to_cab =
                            -*self.state.pwr_aux_for_hvac.get_fresh(|| format_dbg!())? * cop;
                        ensure!(
                            pwr_thrml_hvac_to_cab < si::Power::ZERO,
                            "{}\nHVAC should be cooling cabin",
                            format_dbg!(pwr_thrml_hvac_to_cab)
                        );
                        ensure!(
                            *self.state.pwr_aux_for_hvac.get_fresh(|| format_dbg!())?
                                > si::Power::ZERO,
                            format_dbg!(self.state.pwr_aux_for_hvac)
                        );
                        ensure!(
                            !self
                                .state
                                .pwr_aux_for_hvac
                                .get_fresh(|| format_dbg!())?
                                .is_nan(),
                            format_dbg!(self.state.pwr_aux_for_hvac)
                        );
                    } else {
                        self.state
                            .pwr_aux_for_hvac
                            .update(-pwr_thrml_hvac_to_cab / cop, || format_dbg!())?;
                        ensure!(
                            *self.state.pwr_aux_for_hvac.get_fresh(|| format_dbg!())?
                                > si::Power::ZERO,
                            format_dbg!(self.state.pwr_aux_for_hvac)
                        );
                        ensure!(
                            !self
                                .state
                                .pwr_aux_for_hvac
                                .get_fresh(|| format_dbg!())?
                                .is_nan(),
                            format_dbg!(self.state.pwr_aux_for_hvac)
                        );
                    }
                    self.state
                        .pwr_thrml_hvac_to_cabin
                        .update(pwr_thrml_hvac_to_cab, || format_dbg!())?;
                    self.state
                        .pwr_thrml_fc_to_cabin
                        .update(si::Power::ZERO, || format_dbg!())?;
                    self.state.cop.update(Some(cop), || format_dbg!())?;
                }
                (false, true) => {
                    // outside deadband and heating cabin
                    let te_delta_vs_set = (cab_state
                        .temperature
                        .get_stale(|| format_dbg!())?
                        .get::<si::degree_celsius>()
                        - te_set.get::<si::degree_celsius>())
                        * uc::KELVIN_INT;
                    let te_delta_vs_amb: si::TemperatureInterval = (cab_state
                        .temperature
                        .get_stale(|| format_dbg!())?
                        .get::<si::degree_celsius>()
                        - te_amb_air.get::<si::degree_celsius>())
                        * uc::KELVIN_INT;

                    self.state
                        .pwr_p
                        .update(-self.p * te_delta_vs_set, || format_dbg!())?;
                    ensure!(
                        *self.state.pwr_p.get_fresh(|| format_dbg!())? != si::Power::ZERO,
                        format_dbg!()
                    );
                    self.state.pwr_i.increment(
                        (-self.i * uc::W / uc::KELVIN / uc::S * te_delta_vs_set * dt)
                            .max(-self.pwr_i_max)
                            .min(self.pwr_i_max),
                        || format_dbg!(),
                    )?;
                    ensure!(
                        *self.state.pwr_i.get_fresh(|| format_dbg!())? != si::Power::ZERO,
                        format_dbg!()
                    );
                    self.state.pwr_d.update(
                        -self.d * uc::J / uc::KELVIN
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

                    // HEATING MODE; cabin is colder than set point

                    if *self.state.pwr_i.get_fresh(|| format_dbg!())? < si::Power::ZERO {
                        // If `pwr_i` is less than zero reset to switch from cooling to heating
                        self.state
                            .pwr_i
                            .update_unchecked(si::Power::ZERO, || format_dbg!())?;
                    }
                    let mut pwr_thrml_hvac_to_cab =
                        (*self.state.pwr_p.get_fresh(|| format_dbg!())?
                            + *self.state.pwr_i.get_fresh(|| format_dbg!())?
                            + *self.state.pwr_d.get_fresh(|| format_dbg!())?)
                        .min(self.pwr_thrml_max);
                    ensure!(
                        pwr_thrml_hvac_to_cab > si::Power::ZERO,
                        "{}\nHVAC should be heating cabin",
                        format_dbg!(pwr_thrml_hvac_to_cab)
                    );

                    // Assumes blower has negligible impact on aux load, may want to revise later
                    let (pwr_thrml_fc_to_cabin, cop) = self
                        .handle_heat_source(
                            te_fc,
                            te_delta_vs_amb,
                            &mut pwr_thrml_hvac_to_cab,
                            cab_heat_cap,
                            cab_state,
                            dt,
                        )
                        .with_context(|| format_dbg!())?;
                    ensure!(
                        pwr_thrml_hvac_to_cab >= si::Power::ZERO,
                        "{}\nHVAC should be heating cabin",
                        format_dbg!(pwr_thrml_hvac_to_cab)
                    );
                    self.state
                        .pwr_thrml_hvac_to_cabin
                        .update(pwr_thrml_hvac_to_cab, || format_dbg!())?;
                    self.state
                        .pwr_thrml_fc_to_cabin
                        .update(pwr_thrml_fc_to_cabin, || format_dbg!())?;
                    self.state.cop.update(cop, || format_dbg!())?;
                }
                (true, true) => {
                    unreachable!()
                }
            },
        };

        Ok((
            *self
                .state
                .pwr_thrml_hvac_to_cabin
                .get_fresh(|| format_dbg!())?,
            *self
                .state
                .pwr_thrml_fc_to_cabin
                .get_fresh(|| format_dbg!())?,
        ))
    }

    fn handle_heat_source(
        &mut self,
        te_fc: Option<si::Temperature>,
        te_delta_vs_amb: si::TemperatureInterval,
        pwr_thrml_hvac_to_cab: &mut si::Power,
        cab_heat_cap: si::HeatCapacity,
        cab_state: &LumpedCabinState,
        dt: si::Time,
    ) -> anyhow::Result<(si::Power, Option<si::Ratio>)> {
        let (pwr_thrml_fc_to_cabin, cop) = match self.heat_source {
            CabinHeatSource::FuelConverter => {
                ensure!(
                    te_fc.is_some(),
                    "{}\nExpected vehicle with [FuelConverter] with thermal plant model.",
                    format_dbg!()
                );
                ensure!(
                    *pwr_thrml_hvac_to_cab > si::Power::ZERO,
                    "{}\nHVAC should be heating cabin",
                    format_dbg!(pwr_thrml_hvac_to_cab)
                );
                // limit heat transfer to be substantially less (hence the 0.1)
                // than what is physically possible i.e. the engine can't drop
                // below cabin temperature to heat the cabin
                *pwr_thrml_hvac_to_cab = pwr_thrml_hvac_to_cab.min(
                    (cab_heat_cap
                        * (te_fc
                            .with_context(|| {
                                format!("{}\nExpected `te_fc` to be Some", format_dbg!())
                            })?
                            .get::<si::degree_celsius>()
                            - cab_state
                                .temperature
                                .get_stale(|| format_dbg!())?
                                .get::<si::degree_celsius>())
                        * uc::KELVIN_INT
                        * 0.1
                        / dt)
                        .max(si::Power::ZERO),
                );
                ensure!(
                    *pwr_thrml_hvac_to_cab >= si::Power::ZERO,
                    "{}\nHVAC should be heating cabin",
                    format_dbg!(pwr_thrml_hvac_to_cab)
                );
                let cop = None;
                let pwr_thrml_fc_to_cabin = *pwr_thrml_hvac_to_cab;
                // Assumes aux power needed for heating is incorporated into based aux load.
                // TODO: refine this, perhaps by making aux power
                // proportional to heating power, to account for blower power
                self.state
                    .pwr_aux_for_hvac
                    .update(si::Power::ZERO, || format_dbg!())?;
                (pwr_thrml_fc_to_cabin, cop)
            }
            CabinHeatSource::ResistanceHeater => {
                let cop = Some(uc::R);
                self.state
                    .pwr_aux_for_hvac
                    .update(*pwr_thrml_hvac_to_cab, || format_dbg!())?; // COP is 1 so does not matter
                ensure!(
                    *self.state.pwr_aux_for_hvac.get_fresh(|| format_dbg!())? > si::Power::ZERO,
                    format_dbg!(self.state.pwr_aux_for_hvac)
                );
                #[allow(clippy::let_and_return)] // for readability
                let pwr_thrml_fc_to_cabin = si::Power::ZERO;
                (pwr_thrml_fc_to_cabin, cop)
            }
            CabinHeatSource::HeatPump => {
                // https://en.wikipedia.org/wiki/Coefficient_of_performance#Theoretical_performance_limits
                // cop_ideal is t_h / (t_h - t_c) for heating
                // cop_ideal is t_c / (t_h - t_c) for cooling

                // divide-by-zero protection and realistic limit on COP
                // TODO: make sure this is consist with above commented equation for heating!
                let cop_ideal = if te_delta_vs_amb < 5.0 * uc::KELVIN_INT {
                    // cabin is cooler than ambient + threshold
                    // TODO: make this `5.0` not hardcoded
                    *cab_state.temperature.get_stale(|| format_dbg!())? / (5.0 * uc::KELVIN)
                } else {
                    *cab_state.temperature.get_stale(|| format_dbg!())? / te_delta_vs_amb.abs()
                };
                let cop = cop_ideal * self.frac_of_ideal_cop;
                ensure!(cop > 0.0 * uc::R, format_dbg!(cop));
                if (*pwr_thrml_hvac_to_cab / cop) > self.pwr_aux_for_hvac_max {
                    self.state
                        .pwr_aux_for_hvac
                        .update(self.pwr_aux_for_hvac_max, || format_dbg!())?;
                    // correct if limit is exceeded
                    *pwr_thrml_hvac_to_cab =
                        -*self.state.pwr_aux_for_hvac.get_fresh(|| format_dbg!())? * cop;
                } else {
                    self.state
                        .pwr_aux_for_hvac
                        .update(*pwr_thrml_hvac_to_cab / cop, || format_dbg!())?;
                }
                #[allow(clippy::let_and_return)] // for readability
                let pwr_thrml_fc_to_cabin = si::Power::ZERO;
                (pwr_thrml_fc_to_cabin, Some(cop))
            }
        };
        Ok((pwr_thrml_fc_to_cabin, cop))
    }
}

#[derive(
    Clone, Copy, Debug, Deserialize, Serialize, PartialEq, IsVariant, derive_more::From, TryInto,
)]
pub enum CabinHeatSource {
    /// [FuelConverter], if applicable, provides heat for HVAC system
    FuelConverter,
    /// Resistance heater provides heat for HVAC system
    ResistanceHeater,
    /// Heat pump provides heat for HVAC system
    HeatPump,
}
impl Init for CabinHeatSource {}
impl SerdeAPI for CabinHeatSource {}

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
#[cfg_attr(feature = "pyo3", pyclass(module = "fastsim", subclass, eq))]
#[serde(default)]
#[serde(deny_unknown_fields)]
#[is_state]
pub struct HVACSystemForLumpedCabinState {
    /// time step counter
    pub i: TrackedState<usize>,
    /// portion of total HVAC cooling/heating (negative/positive) power due to proportional gain
    pub pwr_p: TrackedState<si::Power>,
    /// portion of total HVAC cooling/heating (negative/positive) cumulative energy due to proportional gain
    pub energy_p: TrackedState<si::Energy>,
    /// portion of total HVAC cooling/heating (negative/positive) power due to integral gain
    pub pwr_i: TrackedState<si::Power>,
    /// portion of total HVAC cooling/heating (negative/positive) cumulative energy due to integral gain
    pub energy_i: TrackedState<si::Energy>,
    /// portion of total HVAC cooling/heating (negative/positive) power due to derivative gain
    pub pwr_d: TrackedState<si::Power>,
    /// portion of total HVAC cooling/heating (negative/positive) cumulative energy due to derivative gain
    pub energy_d: TrackedState<si::Energy>,
    /// coefficient of performance (i.e. efficiency) of vapor compression cycle
    pub cop: TrackedState<Option<si::Ratio>>,
    /// Aux power demand from [Vehicle::hvac] system
    pub pwr_aux_for_hvac: TrackedState<si::Power>,
    /// Cumulative aux energy for HVAC system
    pub energy_aux_for_hvac: TrackedState<si::Energy>,
    /// Thermal power from HVAC system to cabin, positive is heating the cabin
    pub pwr_thrml_hvac_to_cabin: TrackedState<si::Power>,
    /// Cumulative thermal energy from HVAC system to cabin, positive is heating the cabin
    pub energy_thrml_hvac_to_cabin: TrackedState<si::Energy>,
    /// Thermal power from [FuelConverter] to [Cabin]
    pub pwr_thrml_fc_to_cabin: TrackedState<si::Power>,
    /// Cumulative thermal energy from [FuelConverter] to [Cabin]
    pub energy_thrml_fc_to_cabin: TrackedState<si::Energy>,
}
#[pyo3_api]
impl HVACSystemForLumpedCabinState {
    #[pyo3(name = "default")]
    #[staticmethod]
    fn default_py() -> Self {
        Self::default()
    }
}
impl Init for HVACSystemForLumpedCabinState {}
impl SerdeAPI for HVACSystemForLumpedCabinState {}
