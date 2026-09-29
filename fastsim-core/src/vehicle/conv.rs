use crate::vehicle::common::StartStopControl;

use super::*;

#[serde_api]
#[derive(Clone, Debug, PartialEq, Deserialize, Serialize, StateMethods, SetCumulative)]
#[cfg_attr(feature = "pyo3", pyclass(module = "fastsim", subclass, eq))]
#[non_exhaustive]
#[serde(deny_unknown_fields)]
pub struct DfcoControls {
    /// If true DFCO is enabled, else it will never run.
    pub dfco_enabled: bool,
    /// The minimum speed at or above which which DFCO can activate.
    pub minimum_dfco_speed: si::Velocity,
    /// The minimum vehicle acceleration required for
    /// DFCO to be able to activate.
    pub minimum_dfco_deceleration: si::Acceleration,
    /// Speed threshold at or below which DFCO is considered unavailable due to near-stop operation.
    #[serde(default = "DfcoControls::def_stopped_speed_threshold")]
    pub stopped_speed_threshold: si::Velocity,
    #[serde(default)]
    /// Time step interval between saves. 1 is a good option. If None, no saving occurs.
    pub save_interval: Option<usize>,
    /// current state of control variables
    #[serde(default)]
    pub state: DfcoState,
    /// history of current state
    #[serde(default, skip_serializing_if = "DfcoStateHistoryVec::is_empty")]
    pub history: DfcoStateHistoryVec,
}

#[pyo3_api]
impl DfcoControls {}

impl DfcoControls {
    fn def_stopped_speed_threshold() -> si::Velocity {
        0.05 * uc::MPS
    }

    /// Determine if decel fuel cut-off (DFCO) is disabled based on vehicle
    /// dynamics considerations (i.e., speed, acceleration). Note: considerations
    /// related to whether the engine is too cold and such would be handled
    /// elsewhere.
    pub fn is_dfco_disabled_due_to_veh_dynamics(
        prev_speed: si::Velocity,
        speed: si::Velocity,
        dt: si::Time,
        dfco_allowed: bool,
        minimum_dfco_speed: si::Velocity,
        minimum_dfco_deceleration: si::Acceleration,
        stopped_speed_threshold: si::Velocity,
    ) -> bool {
        let decel = (speed - prev_speed) / dt;
        let is_accel = decel > si::Acceleration::ZERO;
        if !dfco_allowed {
            true
        } else if speed < minimum_dfco_speed {
            true
        } else if speed <= stopped_speed_threshold {
            true
        } else if is_accel || decel > minimum_dfco_deceleration {
            true
        } else {
            // NOTE: we **can** apply DFCO
            false
        }
    }
}

impl HistoryMethods for DfcoControls {
    fn set_save_interval(&mut self, save_interval: Option<usize>) -> anyhow::Result<()> {
        self.save_interval = save_interval;
        Ok(())
    }

    fn save_interval(&self) -> anyhow::Result<Option<usize>> {
        Ok(self.save_interval)
    }

    fn clear(&mut self) {
        self.history.clear();
    }
}

impl Init for DfcoControls {
    fn init(&mut self) -> Result<(), Error> {
        if self.minimum_dfco_deceleration > si::Acceleration::ZERO {
            Err(Error::InitError(String::from(
                "minimum_dfco_acceleration must be <= 0 m/s2",
            )))
        } else if self.minimum_dfco_speed < si::Velocity::ZERO {
            Err(Error::InitError(String::from(
                "minimum_dfco_speed must be >= 0 m/s",
            )))
        } else {
            Ok(())
        }
    }
}

impl SerdeAPI for DfcoControls {}

impl Default for DfcoControls {
    fn default() -> Self {
        Self {
            dfco_enabled: bool::default(),
            minimum_dfco_speed: si::Velocity::default(),
            minimum_dfco_deceleration: si::Acceleration::default(),
            stopped_speed_threshold: Self::def_stopped_speed_threshold(),
            save_interval: Option::default(),
            state: DfcoState::default(),
            history: DfcoStateHistoryVec::default(),
        }
    }
}

impl DfcoControls {
    pub fn new(
        dfco_enabled: bool,
        minimum_dfco_speed: si::Velocity,
        minimum_dfco_deceleration: si::Acceleration,
        save_interval: Option<usize>,
    ) -> anyhow::Result<Self> {
        let mut result = Self {
            dfco_enabled,
            minimum_dfco_speed,
            minimum_dfco_deceleration,
            stopped_speed_threshold: Self::def_stopped_speed_threshold(),
            save_interval,
            state: DfcoState::default(),
            history: DfcoStateHistoryVec::default(),
        };
        result.init()?;
        Ok(result)
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
#[non_exhaustive]
#[serde(deny_unknown_fields)]
pub struct DfcoState {
    /// time step index
    pub i: TrackedState<usize>,
    /// vehicle dynamics must support DFCO to be on
    pub vehicle_dynamics_prevent_dfco: TrackedState<bool>,
}

#[serde_api]
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, StateMethods, SetCumulative)]
#[non_exhaustive]
#[serde(deny_unknown_fields)]
#[cfg_attr(feature = "pyo3", pyclass(module = "fastsim", subclass, eq))]
/// Conventional vehicle with only a FuelConverter as a power source
pub struct ConventionalVehicle {
    pub fs: FuelStorage,
    #[has_state]
    pub fc: FuelConverter,
    #[has_state]
    pub transmission: Transmission,
    /// control strategy. Especially used for start-stop and DFCO.
    #[has_state]
    #[serde(default)]
    pub pt_cntrl: ConvPowertrainControls,
    #[has_state]
    #[serde(default)]
    pub dfco_cntrl: DfcoControls,
    /// powertrain mass
    pub(crate) mass: Option<si::Mass>,
    /// Alternator efficiency used to calculate aux mechanical power demand on engine
    #[serde(default = "default_alt_eff")]
    #[deprecated(
        since = "3.1.0",
        note = "This field will be removed in a future release. Use FuelConverter.aux_eff instead."
    )]
    #[serde(skip_serializing)]
    pub alt_eff: si::Ratio,
}

/// Defaults to 100% for backward compatibility. This field will be removed in a future release.
fn default_alt_eff() -> si::Ratio {
    1.0 * uc::R
}

#[pyo3_api]
impl ConventionalVehicle {}

impl ConventionalVehicle {
    #[allow(deprecated)]
    pub fn new(
        fs: FuelStorage,
        fc: FuelConverter,
        transmission: Transmission,
        mass: Option<si::Mass>,
        pt_cntrl: ConvPowertrainControls,
        dfco_cntrl: DfcoControls,
        alt_eff: si::Ratio,
    ) -> anyhow::Result<Self> {
        let mut conv = Self {
            fs,
            fc,
            transmission,
            pt_cntrl,
            dfco_cntrl,
            mass,
            alt_eff,
        };
        conv.init()?;
        Ok(conv)
    }
}

impl SerdeAPI for ConventionalVehicle {}
impl Init for ConventionalVehicle {
    #[allow(deprecated)]
    fn init(&mut self) -> Result<(), Error> {
        let alt_eff = self.alt_eff.get::<si::ratio>();
        if alt_eff != 1.0 {
            let AuxSupplyEfficiency::Constant(interp) = &self.fc.aux_supply_eff;
            if interp.0 == 1.0 {
                // `fc.aux_supply_eff` was left at its default, so fall back to
                // the deprecated `alt_eff` to preserve legacy simulation behavior
                //
                // Also fires for `fc.aux_supply_eff` explicitly set to 1.0
                eprintln!(
                    "Warning: deprecated field `alt_eff` = `{}` is set. Assigning it to `fc.aux_supply_eff` for backward compatibility. This field will be removed in a future release; set `fc.aux_supply_eff` directly instead.",
                    alt_eff
                );
                self.fc.aux_supply_eff = AuxSupplyEfficiency::from(alt_eff);
            } else if interp.0 != alt_eff {
                // provided both alt_eff != 1.0 and aux_supply_eff that is not equivalent;
                // emit warning that aux_supply_eff overrides alt_eff
                eprintln!(
                    "Warning: provided deprecated field `alt_eff` = `{}` is being overridden by `fc.aux_supply_eff`. The `alt_eff` field will be removed in a future release.",
                    alt_eff
                );
            } else {
                // alt_eff != 1.0 but happens to match fc.aux_supply_eff; still flag
                // the deprecated field since it has no effect but is present
                eprintln!(
                    "Warning: deprecated field `alt_eff` = `{}` is set but has no effect, since it matches `fc.aux_supply_eff`. This field will be removed in a future release; remove it from your vehicle file.",
                    alt_eff
                );
            }
        }
        self.fc
            .init()
            .map_err(|err| Error::InitError(format_dbg!(err)))?;
        self.fs
            .init()
            .map_err(|err| Error::InitError(format_dbg!(err)))?;
        self.transmission
            .init()
            .map_err(|err| Error::InitError(format_dbg!(err)))?;
        Ok(())
    }
}

impl HistoryMethods for ConventionalVehicle {
    fn save_interval(&self) -> anyhow::Result<Option<usize>> {
        bail!("`save_interval` is not implemented in ConventionalVehicle")
    }
    fn set_save_interval(&mut self, save_interval: Option<usize>) -> anyhow::Result<()> {
        // self.fs.set_save_interval(save_interval)?;
        self.fc.set_save_interval(save_interval)?;
        self.transmission.set_save_interval(save_interval)?;
        Ok(())
    }
    fn clear(&mut self) {
        self.fc.clear();
        self.transmission.clear();
    }
}

impl Powertrain for Box<ConventionalVehicle> {
    fn set_curr_pwr_prop_out_max(
        &mut self,
        _pwr_upstream: (si::Power, si::Power),
        pwr_aux: si::Power,
        dt: si::Time,
        veh_state: &VehicleState,
    ) -> anyhow::Result<()> {
        // TODO: account for transmission efficiency in here
        self.fc
            .set_curr_pwr_out_max(dt)
            .with_context(|| anyhow!(format_dbg!()))?;
        let aux_supply_eff = match &self.fc.aux_supply_eff {
            AuxSupplyEfficiency::Constant(interp) => interp.interpolate(&[]),
        }?;
        self.fc
            .set_curr_pwr_prop_max(pwr_aux / aux_supply_eff)
            .with_context(|| anyhow!(format_dbg!()))?;
        self.transmission
            .set_curr_pwr_prop_out_max(
                (
                    *self.fc.state.pwr_prop_max.get_fresh(|| format_dbg!())?,
                    si::Power::ZERO,
                ),
                f64::NAN * uc::W,
                dt,
                veh_state,
            )
            .with_context(|| format_dbg!())?;
        match &mut self.pt_cntrl {
            ConvPowertrainControls::Normal => (),
            ConvPowertrainControls::StartStop(ss) => {
                ss.handle_fc_on_causes(&self.fc, veh_state, dt)?;
            }
        }
        Ok(())
    }

    fn get_curr_pwr_prop_out_max(&self) -> anyhow::Result<(si::Power, si::Power)> {
        self.transmission
            .get_curr_pwr_prop_out_max()
            .with_context(|| format_dbg!())
    }

    fn solve(
        &mut self,
        pwr_out_req: si::Power,
        _enabled: bool,
        dt: si::Time,
    ) -> anyhow::Result<Option<si::Power>> {
        // NOTE: think about the possibility of engine braking, not urgent
        ensure!(pwr_out_req >= si::Power::ZERO, format_dbg!());
        ensure!(almost_le_uom(
            &pwr_out_req,
            self.transmission
                .state
                .pwr_out_fwd_max
                .get_fresh(|| format_dbg!())?,
            None
        ));
        ensure!(almost_le_uom(
            &pwr_out_req,
            self.transmission
                .state
                .pwr_out_fwd_max
                .get_fresh(|| format_dbg!())?,
            None
        ));
        let pwr_in_transmission = self
            .transmission
            .solve(pwr_out_req, true, dt)
            .with_context(|| format_dbg!())?
            .with_context(|| format!("{}\nExpected `Some`", format_dbg!()))?;
        match &mut self.pt_cntrl {
            ConvPowertrainControls::Normal => (),
            ConvPowertrainControls::StartStop(ss) => {
                ConvStartStopControl::handle_fc_on_causes_for_propulsion_request(
                    &mut ss.state.has_traction_power_request,
                    pwr_in_transmission,
                )?;
            }
        }
        let fc_on: bool = {
            let fc_on = self.pt_cntrl.fc_on()?;
            let fc_on_dfco = *self
                .dfco_cntrl
                .state
                .vehicle_dynamics_prevent_dfco
                .get_fresh(|| format_dbg!())?;
            let no_tractive_effort_requested = pwr_out_req <= 1e-6 * uc::KW;
            let fc_off = !fc_on || (!fc_on_dfco && no_tractive_effort_requested);
            !fc_off
        };
        if !fc_on {
            // NOTE: zero out aux loads if engine is off
            // NOTE: we could possibly use Vehicle.pwr_aux_base
            //       to tell if we have "regular" auxiliaries vs
            //       "special" auxiliaries for which the engine
            //       cannot be shut down.
            self.fc.state.pwr_aux.mark_stale();
            self.fc
                .state
                .pwr_aux
                .update(si::Power::ZERO, || format_dbg!())?;
        }
        self.fc
            .solve(pwr_in_transmission, fc_on, dt)
            .with_context(|| anyhow!(format_dbg!()))?;
        Ok(None)
    }

    fn pwr_regen(&self) -> anyhow::Result<si::Power> {
        Ok(si::Power::ZERO)
    }
}

impl ConventionalVehicle {
    pub fn solve_thermal(
        &mut self,
        te_amb: si::Temperature,
        pwr_thrml_fc_to_cab: Option<si::Power>,
        veh_state: &mut VehicleState,
        dt: si::Time,
    ) -> anyhow::Result<()> {
        self.fc
            .solve_thermal(te_amb, pwr_thrml_fc_to_cab, veh_state, dt)
    }
}

impl Mass for ConventionalVehicle {
    fn mass(&self) -> anyhow::Result<Option<si::Mass>> {
        let derived_mass = self
            .derived_mass()
            .with_context(|| anyhow!(format_dbg!()))?;
        match (derived_mass, self.mass) {
            (Some(derived_mass), Some(set_mass)) => {
                ensure!(
                    utils::almost_eq_uom(&set_mass, &derived_mass, None),
                    format!(
                        "{}",
                        format_dbg!(utils::almost_eq_uom(&set_mass, &derived_mass, None)),
                    )
                );
                Ok(Some(set_mass))
            }
            _ => Ok(self.mass.or(derived_mass)),
        }
    }

    fn set_mass(
        &mut self,
        new_mass: Option<si::Mass>,
        side_effect: MassSideEffect,
    ) -> anyhow::Result<()> {
        ensure!(
            side_effect == MassSideEffect::None,
            "At the powertrain level, only `MassSideEffect::None` is allowed"
        );
        let derived_mass = self
            .derived_mass()
            .with_context(|| anyhow!(format_dbg!()))?;
        self.mass = match (new_mass, derived_mass) {
            // Set using provided `new_mass`, setting constituent mass fields to `None` to match if inconsistent
            (Some(new_mass), Some(dm)) => {
                if dm != new_mass {
                    self.expunge_mass_fields();
                }
                Some(new_mass)
            }
            (Some(new_mass), None) => Some(new_mass),
            (None, Some(dm)) => Some(dm),
            (None, None) => bail!(
                "Not all mass fields in `{}` are set and no mass was provided.",
                stringify!(ConventionalVehicle)
            ),
        };
        ensure!(
            self.mass > Some(0.0 * uc::KG),
            "{} mass must be positive",
            stringify!(ConventionalVehicle)
        );
        Ok(())
    }

    fn derived_mass(&self) -> anyhow::Result<Option<si::Mass>> {
        let fc_mass = self.fc.mass().with_context(|| anyhow!(format_dbg!()))?;
        let fs_mass = self.fs.mass().with_context(|| anyhow!(format_dbg!()))?;
        let transmission_mass = self
            .transmission
            .mass()
            .with_context(|| anyhow!(format_dbg!()))?;
        match (fc_mass, fs_mass, transmission_mass) {
            (Some(fc_mass), Some(fs_mass), Some(transmission_mass)) => {
                Ok(Some(fc_mass + fs_mass + transmission_mass))
            }
            (None, None, None) => Ok(None),
            _ => bail!(
                "`{}` field masses are not consistently set to `Some` or `None`",
                stringify!(ConventionalVehicle)
            ),
        }
    }

    fn expunge_mass_fields(&mut self) {
        self.fc.expunge_mass_fields();
        self.fs.expunge_mass_fields();
        self.transmission.expunge_mass_fields();
        self.mass = None;
    }
}

#[derive(
    Clone, Debug, PartialEq, Deserialize, Serialize, IsVariant, derive_more::From, TryInto,
)]
pub enum ConvPowertrainControls {
    /// Normal controller that doesn't do anything special
    Normal,
    /// Start/Stop controller that allows the fuel converter to turn off at
    /// stop under certain conditions
    #[serde(alias = "StopStart")]
    StartStop(Box<ConvStartStopControl>),
}

impl Default for ConvPowertrainControls {
    fn default() -> Self {
        Self::Normal
    }
}

impl SetCumulative for ConvPowertrainControls {
    fn set_cumulative<F: Fn() -> String>(&mut self, dt: si::Time, loc: F) -> anyhow::Result<()> {
        match self {
            Self::Normal => Ok(()),
            Self::StartStop(cntrl) => {
                cntrl.set_cumulative(dt, || format!("{}\n{}", loc(), format_dbg!()))?;
                Ok(())
            }
        }
    }

    fn reset_cumulative<F: Fn() -> String>(&mut self, loc: F) -> anyhow::Result<()> {
        match self {
            Self::Normal => Ok(()),
            Self::StartStop(cntrl) => {
                cntrl.reset_cumulative(|| format!("{}\n{}", loc(), format_dbg!()))?;
                Ok(())
            }
        }
    }
}

impl Step for ConvPowertrainControls {
    fn step<F: Fn() -> String>(&mut self, loc: F) -> anyhow::Result<()> {
        match self {
            Self::Normal => Ok(()),
            Self::StartStop(cntrl) => cntrl.step(loc),
        }
    }

    fn reset_step<F: Fn() -> String>(&mut self, loc: F) -> anyhow::Result<()> {
        match self {
            Self::Normal => Ok(()),
            Self::StartStop(cntrls) => cntrls.reset_step(loc),
        }
    }
}

impl StateMethods for ConvPowertrainControls {}

impl SaveState for ConvPowertrainControls {
    fn save_state<F: Fn() -> String>(&mut self, loc: F) -> anyhow::Result<()> {
        match self {
            Self::Normal => Ok(()),
            Self::StartStop(cntrl) => cntrl.save_state(loc),
        }
    }
}

impl TrackedStateMethods for ConvPowertrainControls {
    fn check_and_reset<F: Fn() -> String>(&mut self, loc: F) -> anyhow::Result<()> {
        match self {
            Self::Normal => Ok(()),
            Self::StartStop(cntrl) => cntrl.check_and_reset(loc),
        }
    }

    fn mark_fresh<F: Fn() -> String>(&mut self, loc: F) -> anyhow::Result<()> {
        match self {
            Self::Normal => Ok(()),
            Self::StartStop(cntrl) => cntrl.mark_fresh(loc),
        }
    }
}

impl HistoryMethods for ConvPowertrainControls {
    fn set_save_interval(&mut self, save_interval: Option<usize>) -> anyhow::Result<()> {
        match self {
            Self::Normal => Ok(()),
            Self::StartStop(cntrl) => Ok(cntrl.set_save_interval(save_interval)?),
        }
    }

    fn save_interval(&self) -> anyhow::Result<Option<usize>> {
        match self {
            Self::Normal => Ok(Option::None),
            Self::StartStop(cntrl) => cntrl.save_interval(),
        }
    }

    fn clear(&mut self) {
        match self {
            Self::Normal => (),
            Self::StartStop(cntrl) => cntrl.clear(),
        }
    }
}

impl Init for ConvPowertrainControls {
    fn init(&mut self) -> Result<(), Error> {
        match self {
            Self::Normal => Ok(()),
            Self::StartStop(cntrl) => cntrl.init(),
        }
    }
}

impl ConvPowertrainControls {
    pub fn fc_on(&self) -> anyhow::Result<bool> {
        match self {
            Self::Normal => Ok(true),
            Self::StartStop(cntrl) => cntrl.state.fc_on(),
        }
    }

    pub fn handle_fc_on_causes_for_speed(&mut self, speed: si::Velocity) -> anyhow::Result<()> {
        match self {
            Self::Normal => Ok(()),
            Self::StartStop(cntrl) => ConvStartStopControl::handle_fc_on_causes_for_speed(
                &mut cntrl.state.vehicle_not_stopped,
                speed,
                cntrl.stopped_speed_threshold,
            ),
        }
    }
}

#[serde_api]
#[derive(Clone, Debug, PartialEq, Deserialize, Serialize, StateMethods, SetCumulative)]
#[cfg_attr(feature = "pyo3", pyclass(module = "fastsim", subclass, eq))]
#[non_exhaustive]
#[serde(deny_unknown_fields)]
pub struct ConvStartStopControl {
    /// Minimum time engine must remain on if it was on during the previous
    /// simulation time step.
    #[serde(default)]
    pub fc_min_time_on: Option<si::Time>,
    /// temperature at which engine is forced on to warm up
    #[serde(default)]
    pub temp_fc_forced_on: Option<si::Temperature>,
    /// temperature at which engine is allowed to turn off due to being sufficiently warm
    #[serde(default)]
    pub temp_fc_allowed_off: Option<si::Temperature>,
    /// Time delay after the vehicle reaches a stop before the engine is allowed
    /// to turn off. This is to try to prevent engine stopping when the vehicle
    /// stop is only momentary.
    #[serde(default)]
    pub time_delay_after_stop_until_fc_can_turn_off: Option<si::Time>,
    /// Speed threshold at or below which vehicle is considered stopped for start-stop logic.
    #[serde(default = "ConvStartStopControl::def_stopped_speed_threshold")]
    pub stopped_speed_threshold: si::Velocity,
    #[serde(default)]
    /// Time step interval between saves. 1 is a good option. If None, no saving occurs.
    pub save_interval: Option<usize>,
    /// current state of control variables
    #[serde(default)]
    pub state: ConvStartStopState,
    /// history of current state
    #[serde(
        default,
        skip_serializing_if = "ConvStartStopStateHistoryVec::is_empty"
    )]
    pub history: ConvStartStopStateHistoryVec,
}

#[pyo3_api]
impl ConvStartStopControl {}

impl StartStopControl for ConvStartStopControl {}

impl HistoryMethods for ConvStartStopControl {
    fn set_save_interval(&mut self, save_interval: Option<usize>) -> anyhow::Result<()> {
        self.save_interval = save_interval;
        Ok(())
    }

    fn save_interval(&self) -> anyhow::Result<Option<usize>> {
        Ok(self.save_interval)
    }

    fn clear(&mut self) {
        self.history.clear();
    }
}

impl Init for ConvStartStopControl {
    fn init(&mut self) -> Result<(), Error> {
        init_opt_default!(self, fc_min_time_on, uc::S * 5.0);
        init_opt_default!(
            self,
            time_delay_after_stop_until_fc_can_turn_off,
            0.0 * uc::S
        );
        Ok(())
    }
}

impl SerdeAPI for ConvStartStopControl {}

impl Default for ConvStartStopControl {
    fn default() -> Self {
        Self {
            fc_min_time_on: Option::default(),
            temp_fc_forced_on: Option::default(),
            temp_fc_allowed_off: Option::default(),
            time_delay_after_stop_until_fc_can_turn_off: Option::default(),
            stopped_speed_threshold: Self::def_stopped_speed_threshold(),
            save_interval: Option::default(),
            state: ConvStartStopState::default(),
            history: ConvStartStopStateHistoryVec::default(),
        }
    }
}

impl ConvStartStopControl {
    fn def_stopped_speed_threshold() -> si::Velocity {
        0.05 * uc::MPS
    }

    pub fn new(
        fc_min_time_on: Option<si::Time>,
        temp_fc_forced_on: Option<si::Temperature>,
        temp_fc_allowed_off: Option<si::Temperature>,
        time_delay_after_stop_until_fc_can_turn_off: Option<si::Time>,
        save_interval: Option<usize>,
    ) -> anyhow::Result<Self> {
        let mut result = Self {
            fc_min_time_on,
            temp_fc_forced_on,
            temp_fc_allowed_off,
            time_delay_after_stop_until_fc_can_turn_off,
            stopped_speed_threshold: Self::def_stopped_speed_threshold(),
            save_interval,
            state: ConvStartStopState::default(),
            history: ConvStartStopStateHistoryVec::default(),
        };
        result.init()?;
        Ok(result)
    }

    pub fn handle_fc_on_causes(
        &mut self,
        fc: &FuelConverter,
        veh_state: &VehicleState,
        dt: si::Time,
    ) -> anyhow::Result<()> {
        // NOTE: handle_fc_on_causes_for_propulsion_request called elsewhere
        Self::handle_fc_on_causes_for_stopped_time(
            &mut self.state.time_vehicle_stopped,
            &mut self.state.vehicle_not_stopped_long_enough,
            veh_state,
            dt,
            self.time_delay_after_stop_until_fc_can_turn_off,
            self.stopped_speed_threshold,
        )?;
        Self::handle_fc_on_causes_for_temp(
            fc,
            self.temp_fc_forced_on,
            self.temp_fc_allowed_off,
            &mut self.state.fc_temperature_too_low,
        )?;
        // NOTE: handle_fc_on_causes_for_speed(speed) called elsewhere
        Self::handle_fc_on_causes_for_on_time(
            fc,
            self.fc_min_time_on,
            &mut self.state.on_time_too_short,
        )?;
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
#[non_exhaustive]
#[serde(deny_unknown_fields)]
pub struct ConvStartStopState {
    /// time step index
    pub i: TrackedState<usize>,
    /// Engine must be on to self heat if thermal model is enabled
    pub fc_temperature_too_low: TrackedState<bool>,
    /// Engine start-stop can only happen while vehicle is stopped
    pub vehicle_not_stopped: TrackedState<bool>,
    /// Engine has not been on long enough (usually 30 s)
    pub on_time_too_short: TrackedState<bool>,
    /// The total time vehicle has been stopped
    pub time_vehicle_stopped: TrackedState<si::Time>,
    /// Vehicle stopped time
    pub vehicle_not_stopped_long_enough: TrackedState<bool>,
    /// Vehicle has a request for traction power for the current timestep
    pub has_traction_power_request: TrackedState<bool>,
}

impl ConvStartStopState {
    /// If any of the causes are true, engine must be on
    fn fc_on(&self) -> anyhow::Result<bool> {
        let c1 = *self.fc_temperature_too_low.get_fresh(|| format_dbg!())?;
        let c2 = *self.vehicle_not_stopped.get_fresh(|| format_dbg!())?;
        let c3 = *self.on_time_too_short.get_fresh(|| format_dbg!())?;
        let c4 = *self
            .vehicle_not_stopped_long_enough
            .get_fresh(|| format_dbg!())?;
        let c5 = *self
            .has_traction_power_request
            .get_fresh(|| format_dbg!())?;
        Ok(c1 || c2 || c3 || c4 || c5)
    }
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;

    fn make_favorable_dfco_conditions() -> (
        si::Velocity,     // prev_speed
        si::Velocity,     // speed
        si::Time,         // dt
        bool,             // dfco_allowed
        si::Velocity,     // minimum_dfco_speed
        si::Acceleration, // minimum_dfco_deceleration
    ) {
        (
            40.0 * uc::MPH,
            36.0 * uc::MPH,
            1.0 * uc::S,
            true,
            20.0 * uc::MPH,
            0.0 * uc::MPS2,
        )
    }

    #[test]
    fn dfco_activates_when_all_conditions_are_good() {
        let (prev_speed, speed, dt, dfco_allowed, minimum_dfco_speed, minimum_dfco_deceleration) =
            make_favorable_dfco_conditions();
        let result = DfcoControls::is_dfco_disabled_due_to_veh_dynamics(
            prev_speed,
            speed,
            dt,
            dfco_allowed,
            minimum_dfco_speed,
            minimum_dfco_deceleration,
            0.05 * uc::MPS,
        );
        assert_eq!(false, result);
    }

    #[test]
    fn dfco_cannot_be_active_if_speed_too_low() {
        let (_prev_speed, _speed, dt, dfco_allowed, minimum_dfco_speed, minimum_dfco_deceleration) =
            make_favorable_dfco_conditions();
        let prev_speed = 10.0 * uc::MPH;
        let speed = 8.0 * uc::MPH;
        let result = DfcoControls::is_dfco_disabled_due_to_veh_dynamics(
            prev_speed,
            speed,
            dt,
            dfco_allowed,
            minimum_dfco_speed,
            minimum_dfco_deceleration,
            0.05 * uc::MPS,
        );
        assert_eq!(result, true);
    }

    #[test]
    fn dfco_cannot_be_active_if_not_decelerating() {
        let (prev_speed, _speed, dt, dfco_allowed, minimum_dfco_speed, minimum_dfco_deceleration) =
            make_favorable_dfco_conditions();
        let speed = prev_speed + 2.0 * uc::MPH;
        let result = DfcoControls::is_dfco_disabled_due_to_veh_dynamics(
            prev_speed,
            speed,
            dt,
            dfco_allowed,
            minimum_dfco_speed,
            minimum_dfco_deceleration,
            0.05 * uc::MPS,
        );
        assert_eq!(true, result);
    }

    #[test]
    fn dfco_cannot_be_active_if_not_allowed() {
        let (prev_speed, speed, dt, _dfco_allowed, minimum_dfco_speed, minimum_dfco_deceleration) =
            make_favorable_dfco_conditions();
        let dfco_allowed = false;
        let result = DfcoControls::is_dfco_disabled_due_to_veh_dynamics(
            prev_speed,
            speed,
            dt,
            dfco_allowed,
            minimum_dfco_speed,
            minimum_dfco_deceleration,
            0.05 * uc::MPS,
        );
        assert_eq!(true, result);
    }
}
