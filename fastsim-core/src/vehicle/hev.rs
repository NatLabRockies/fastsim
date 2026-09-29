use super::{vehicle_model::VehicleState, *};
use crate::{prelude::ElectricMachineState, vehicle::common::*};

#[serde_api]
#[derive(Clone, Debug, PartialEq, Deserialize, Serialize, StateMethods, SetCumulative)]
#[non_exhaustive]
#[serde(deny_unknown_fields)]
#[cfg_attr(feature = "pyo3", pyclass(module = "fastsim", subclass, eq))]
/// Hybrid vehicle with both engine and reversible energy storage (aka battery)
/// This type of vehicle is not likely to be widely prevalent due to modularity of consists.
pub struct HybridElectricVehicle {
    #[has_state]
    pub res: ReversibleEnergyStorage,
    pub fs: FuelStorage,
    #[has_state]
    pub fc: FuelConverter,
    #[has_state]
    pub em: ElectricMachine,
    #[has_state]
    pub transmission: Transmission,
    /// control strategy for distributing power demand between `fc` and `res`
    #[has_state]
    #[serde(default)]
    pub pt_cntrl: HEVPowertrainControls,
    /// control strategy for distributing aux power demand between `fc` and `res`
    #[serde(default)]
    pub aux_cntrl: HEVAuxControls,
    /// hybrid powertrain mass
    pub(crate) mass: Option<si::Mass>,
    #[serde(default)]
    pub sim_params: HEVSimulationParams,
    /// vector of SOC balance iterations
    #[serde(default)]
    pub soc_bal_iter_history: Vec<Self>,
    /// Number of `run` iterations required to achieve SOC balance (i.e. SOC
    /// ends at same starting value, ensuring no net [ReversibleEnergyStorage] usage)
    #[serde(default)]
    pub soc_bal_iters: TrackedState<u32>,
}

impl HybridElectricVehicle {
    /// This method should be called after initialization but prior to
    /// simulation start. It checks that the buffer parameters are reasonable as
    /// compared to RES capacity and the like. Note: currently, this routine
    /// doesn't panic -- only writes to stderr if it detects an issue.
    pub fn check_buffers(&self, veh_mass: si::Mass) -> anyhow::Result<()> {
        // CHECK BUFFER PARAMETERS ARE REALISTIC
        let (disch_buffer, chrg_buffer, fc_on_soc) = match &self.pt_cntrl {
            HEVPowertrainControls::RGWDB(rgwdb) => {
                let disch_buffer = (0.5
                    * veh_mass
                    * rgwdb
                        .speed_soc_disch_buffer
                        .with_context(|| format_dbg!())?
                        .powi(P2::new()))
                .max(si::Energy::ZERO)
                    * rgwdb
                        .speed_soc_disch_buffer_coeff
                        .with_context(|| format_dbg!())?;

                let chrg_buffer = (0.5
                    * veh_mass
                    * ((70.0 * uc::MPH).powi(P2::new())
                        - rgwdb
                            .speed_soc_regen_buffer
                            .with_context(|| format_dbg!())?
                            .powi(P2::new())))
                .max(si::Energy::ZERO)
                    * rgwdb
                        .speed_soc_regen_buffer_coeff
                        .with_context(|| format_dbg!())?;

                let fc_on_soc = {
                    let energy_delta_to_buffer_speed: si::Energy = 0.5
                        * veh_mass
                        * rgwdb
                            .speed_soc_fc_on_buffer
                            .with_context(|| format_dbg!())?
                            .powi(P2::new());
                    energy_delta_to_buffer_speed.max(si::Energy::ZERO)
                        * rgwdb
                            .speed_soc_fc_on_buffer_coeff
                            .with_context(|| format_dbg!())?
                } / self.res.energy_capacity_usable()
                    + self.res.min_soc;

                (disch_buffer, chrg_buffer, fc_on_soc)
            }
            HEVPowertrainControls::StartStop(_) => {
                let fc_on_soc = 0.10 * (self.res.max_soc - self.res.min_soc) + self.res.min_soc;
                let chrg_buffer = self.res.energy_capacity_usable();
                let disch_buffer = self.res.energy_capacity_usable();
                (disch_buffer, chrg_buffer, fc_on_soc)
            }
        };
        if fc_on_soc > self.res.max_soc {
            // eprintln!("fc_on_soc > self.res.max_soc");
            // eprintln!("fc_on_soc: {:?}", fc_on_soc);
        }
        if fc_on_soc < self.res.min_soc {
            // eprintln!("fc_on_soc < self.res.min_soc");
            // eprintln!("fc_on_soc: {:?}", fc_on_soc);
        }
        if disch_buffer > self.res.energy_capacity_usable() {
            // eprintln!("disch_buffer > self.res.energy_capacity_usable()");
            // eprintln!(
            //     "disch_buffer: {:?} kWh",
            //     disch_buffer.get::<si::kilowatt_hour>()
            // );
            // eprintln!(
            //     "RES usable energy capacity: {:?} kWh",
            //     self.res.energy_capacity_usable().get::<si::kilowatt_hour>()
            // );
        }
        if chrg_buffer > self.res.energy_capacity_usable() {
            // eprintln!("chrg_buffer > self.res.energy_capacity_usable()");
            // eprintln!(
            //     "chrg_buffer: {:?} kWh",
            //     chrg_buffer.get::<si::kilowatt_hour>()
            // );
            // eprintln!(
            //     "RES usable energy capacity: {:?} kWh",
            //     self.res.energy_capacity_usable().get::<si::kilowatt_hour>()
            // );
        }
        Ok(())
    }
}

#[pyo3_api]
impl HybridElectricVehicle {}

impl HybridElectricVehicle {
    pub fn new(
        res: ReversibleEnergyStorage,
        fs: FuelStorage,
        fc: FuelConverter,
        em: ElectricMachine,
        transmission: Transmission,
        pt_cntrl: HEVPowertrainControls,
        aux_cntrl: HEVAuxControls,
        mass: Option<si::Mass>,
        sim_params: HEVSimulationParams,
    ) -> anyhow::Result<Self> {
        let mut hev = Self {
            res,
            fs,
            fc,
            em,
            transmission,
            pt_cntrl,
            aux_cntrl,
            mass,
            sim_params,
            soc_bal_iter_history: Default::default(),
            soc_bal_iters: Default::default(),
        };
        hev.init()?;
        Ok(hev)
    }
}

impl HistoryMethods for HybridElectricVehicle {
    fn save_interval(&self) -> anyhow::Result<Option<usize>> {
        bail!("`save_interval` is not implemented in HybridElectricVehicle")
    }
    fn set_save_interval(&mut self, save_interval: Option<usize>) -> anyhow::Result<()> {
        self.res.set_save_interval(save_interval)?;
        // self.fs.set_save_interval(save_interval)?;
        self.fc.set_save_interval(save_interval)?;
        self.em.set_save_interval(save_interval)?;
        self.transmission.set_save_interval(save_interval)?;
        self.pt_cntrl.set_save_interval(save_interval)?;
        Ok(())
    }
    fn clear(&mut self) {
        self.res.clear();
        // self.fs.clear();
        self.fc.clear();
        self.em.clear();
        self.transmission.clear();
        self.pt_cntrl.clear();
    }
}

impl Init for HybridElectricVehicle {
    fn init(&mut self) -> Result<(), Error> {
        self.fc
            .init()
            .map_err(|err| Error::InitError(format_dbg!(err)))?;
        self.res
            .init()
            .map_err(|err| Error::InitError(format_dbg!(err)))?;
        self.em
            .init()
            .map_err(|err| Error::InitError(format_dbg!(err)))?;
        self.transmission
            .init()
            .map_err(|err| Error::InitError(format_dbg!(err)))?;
        self.pt_cntrl
            .init()
            .map_err(|err| Error::InitError(format_dbg!(err)))?;
        Ok(())
    }
}

impl SerdeAPI for HybridElectricVehicle {}

impl Powertrain for Box<HybridElectricVehicle> {
    fn set_curr_pwr_prop_out_max(
        &mut self,
        _pwr_upstream: (si::Power, si::Power),
        pwr_aux: si::Power,
        dt: si::Time,
        veh_state: &VehicleState,
    ) -> anyhow::Result<()> {
        // TODO: account for transmission efficiency in here
        let (disch_buffer, chrg_buffer) = match &mut self.pt_cntrl {
            HEVPowertrainControls::RGWDB(rgwdb) => {
                rgwdb.handle_fc_on_causes(&self.fc, veh_state, &self.res, &self.em.state)?;

                let disch_buffer = (0.5
                    * *veh_state.mass.get_fresh(|| format_dbg!())?
                    * (rgwdb
                        .speed_soc_disch_buffer
                        .with_context(|| format_dbg!())?
                        .powi(P2::new())
                        - veh_state
                            .speed_ach
                            .get_stale(|| format_dbg!())?
                            .powi(P2::new())))
                .max(si::Energy::ZERO)
                    * rgwdb
                        .speed_soc_disch_buffer_coeff
                        .with_context(|| format_dbg!())?;

                let chrg_buffer = (0.5
                    * *veh_state.mass.get_fresh(|| format_dbg!())?
                    * (veh_state
                        .speed_ach
                        .get_stale(|| format_dbg!())?
                        .powi(P2::new())
                        - rgwdb
                            .speed_soc_regen_buffer
                            .with_context(|| format_dbg!())?
                            .powi(P2::new())))
                .max(si::Energy::ZERO)
                    * rgwdb
                        .speed_soc_regen_buffer_coeff
                        .with_context(|| format_dbg!())?;

                (disch_buffer, chrg_buffer)
            }
            HEVPowertrainControls::StartStop(cntrl) => {
                cntrl.handle_fc_on_causes(&self.fc, veh_state, &self.res, dt)?;

                let disch_buffer = 0.0 * uc::J;
                let chrg_buffer = self.res.energy_capacity_usable();
                (disch_buffer, chrg_buffer)
            }
        };
        // set total max powers, including aux power
        self.fc
            .set_curr_pwr_out_max(dt)
            .with_context(|| anyhow!(format_dbg!()))?;
        self.res
            .set_curr_pwr_out_max(dt, disch_buffer, chrg_buffer)
            .with_context(|| anyhow!(format_dbg!()))?;

        // determine distribution of aux power between engine and battery
        let (pwr_aux_res, pwr_aux_fc) = {
            match self.aux_cntrl {
                HEVAuxControls::AuxOnResPriority => {
                    if pwr_aux <= *self.res.state.pwr_disch_max.get_fresh(|| format_dbg!())? {
                        (pwr_aux, si::Power::ZERO)
                    } else {
                        (si::Power::ZERO, pwr_aux)
                    }
                }
                HEVAuxControls::AuxOnFcPriority => (si::Power::ZERO, pwr_aux),
            }
        };

        match &mut self.pt_cntrl {
            HEVPowertrainControls::RGWDB(rgwdb) => {
                rgwdb
                    .state
                    .aux_power_demand
                    .update(pwr_aux_fc > si::Power::ZERO, || format_dbg!())?;
            }
            HEVPowertrainControls::StartStop(cntrl) => {
                cntrl
                    .state
                    .aux_power_demand
                    .update(pwr_aux_fc > si::Power::ZERO, || format_dbg!())?;
            }
        }

        // set max propulsion powers
        self.fc
            .set_curr_pwr_prop_max(pwr_aux_fc)
            .with_context(|| anyhow!(format_dbg!()))?;
        self.res
            .set_curr_pwr_prop_max(pwr_aux_res)
            .with_context(|| anyhow!(format_dbg!()))?;
        self.em
            .set_curr_pwr_prop_out_max(
                // TODO: add means of controlling whether fc can provide power to em and also how much
                // Try out a 'power out type' enum field on the fuel converter with variants for mechanical and electrical
                self.res
                    .get_curr_pwr_prop_out_max()
                    .with_context(|| format_dbg!())?,
                pwr_aux,
                dt,
                veh_state,
            )
            .with_context(|| anyhow!(format_dbg!()))?;
        let em_pwr_prop_out_maxes = self
            .em
            .get_curr_pwr_prop_out_max()
            .with_context(|| format_dbg!())?;
        let fc_max = self.fc.state.pwr_prop_max.get_fresh(|| format_dbg!())?;
        self.transmission
            .set_curr_pwr_prop_out_max(
                (em_pwr_prop_out_maxes.0 + *fc_max, em_pwr_prop_out_maxes.1),
                f64::NAN * uc::W,
                dt,
                veh_state,
            )
            .with_context(|| format_dbg!())?;
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
        // TODO: address these concerns
        // - what happens when the fc is on and producing more power than the
        //   transmission requires? It seems like the excess goes straight to the battery,
        //   but it should probably go through the em somehow.
        let pwr_in_transmission = self
            .transmission
            .solve(pwr_out_req, true, dt)
            .with_context(|| format_dbg!())?
            .with_context(|| format!("{}\nExpected `Some`", format_dbg!()))?;

        // TODO: use an enum with a match here to determine whether power is shared by
        // - fc and em (e.g. for ICE HEV)
        //   or
        // - fc and res (e.g. for H2FC HEV)

        let (fc_pwr_out_req, em_pwr_out_req) = self
            .pt_cntrl
            .get_pwr_fc_and_em(pwr_in_transmission, &self.fc, &self.em.state, &self.res)
            .with_context(|| format_dbg!())?;
        let fc_on: bool = self.pt_cntrl.fc_on().map_err(|err| {
            anyhow::anyhow!(
                "self.pt_cntrl.fc_on() failed at line {} with \
                originating error [{}]",
                format_dbg!(),
                err
            )
        })?;

        self.fc.solve(fc_pwr_out_req, fc_on, dt).map_err(|err| {
            anyhow::anyhow!(
                "self.fc.solve(fc_pwr_out_req, fc_on, dt) with values: \
                    fc_pwr_out_req={:?}, fc_on={}, dt={:?} \
                    failed at line {} \
                    with originating error [{}]",
                fc_pwr_out_req,
                fc_on,
                dt,
                format_dbg!(),
                err
            )
        })?;

        let res_pwr_out_req = self
            .em
            .solve(em_pwr_out_req, true, dt)
            .map_err(|err| {
                anyhow!(format!(
                    "em.solve failed at line {} with originating error [{}]",
                    format_dbg!(),
                    err
                ))
            })?
            .with_context(|| format!("{}\nExpected `Some`", format_dbg!()))?;
        // TODO: `res_pwr_out_req` probably does not include charging from the engine
        self.res.solve(res_pwr_out_req, dt).map_err(|err| {
            anyhow!(format!(
                "res.solve failed at line {} with originating error [{}]",
                format_dbg!(),
                err
            ))
        })?;
        Ok(None)
    }

    /// Regen braking power, positive means braking is happening
    fn pwr_regen(&self) -> anyhow::Result<si::Power> {
        // When `pwr_mech_prop_out` is negative, regen is happening.  First, clip it at 0, and then negate it.
        // see https://play.rust-lang.org/?version=stable&mode=debug&edition=2021&gist=e8f7af5a6e436dd1163fa3c70931d18d
        // for example
        self.transmission.pwr_regen().with_context(|| format_dbg!())
    }
}

impl HybridElectricVehicle {
    /// # Arguments
    /// - `te_amb`: ambient temperature
    /// - `pwr_thrml_fc_to_cab`: thermal power flow from [FuelConverter::thrml]
    ///   to [Vehicle::cabin], if cabin is equipped
    /// - `veh_state`: current [VehicleState]
    /// - `pwr_thrml_hvac_to_res`: thermal power flow from [Vehicle::hvac] --
    ///   zero if `None` is passed
    /// - `te_cab`: cabin temperature, required if [ReversibleEnergyStorage::thrml] is `Some`
    /// - `dt`: simulation time step size
    pub fn solve_thermal(
        &mut self,
        te_amb: si::Temperature,
        pwr_thrml_fc_to_cab: Option<si::Power>,
        veh_state: &mut VehicleState,
        pwr_thrml_hvac_to_res: Option<si::Power>,
        te_cab: Option<si::Temperature>,
        dt: si::Time,
    ) -> anyhow::Result<()> {
        self.fc
            .solve_thermal(te_amb, pwr_thrml_fc_to_cab, veh_state, dt)
            .with_context(|| format_dbg!())?;
        self.res
            .solve_thermal(
                te_amb,
                pwr_thrml_hvac_to_res.unwrap_or_default(),
                te_cab,
                dt,
            )
            .with_context(|| format_dbg!())?;
        Ok(())
    }
}

impl Mass for HybridElectricVehicle {
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
                stringify!(HybridElectricVehicle)
            ),
        };
        ensure!(
            self.mass > Some(0.0 * uc::KG),
            "{} mass must be positive",
            stringify!(HybridElectricVehicle)
        );
        Ok(())
    }

    fn derived_mass(&self) -> anyhow::Result<Option<si::Mass>> {
        let fc_mass = self.fc.mass().with_context(|| anyhow!(format_dbg!()))?;
        let fs_mass = self.fs.mass().with_context(|| anyhow!(format_dbg!()))?;
        let res_mass = self.res.mass().with_context(|| anyhow!(format_dbg!()))?;
        let em_mass = self.em.mass().with_context(|| anyhow!(format_dbg!()))?;
        let transmission_mass = self
            .transmission
            .mass()
            .with_context(|| anyhow!(format_dbg!()))?;
        match (fc_mass, fs_mass, res_mass, em_mass, transmission_mass) {
            (
                Some(fc_mass),
                Some(fs_mass),
                Some(res_mass),
                Some(em_mass),
                Some(transmission_mass),
            ) => Ok(Some(
                fc_mass + fs_mass + res_mass + em_mass + transmission_mass,
            )),
            (None, None, None, None, None) => Ok(None),
            _ => bail!(
                "`{}` field masses are not consistently set to `Some` or `None`",
                stringify!(HybridElectricVehicle)
            ),
        }
    }

    fn expunge_mass_fields(&mut self) {
        self.fc.expunge_mass_fields();
        self.fs.expunge_mass_fields();
        self.res.expunge_mass_fields();
        self.em.expunge_mass_fields();
        self.transmission.expunge_mass_fields();
        self.mass = None;
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
#[is_state]
pub struct RGWDBState {
    /// time step index
    pub i: TrackedState<usize>,
    /// Engine must be on to self heat if thermal model is enabled
    pub fc_temperature_too_low: TrackedState<bool>,
    /// Engine must be on for high vehicle speed to ensure powertrain can meet
    /// any spikes in power demand
    pub vehicle_speed_too_high: TrackedState<bool>,
    /// Engine has not been on long enough (usually 30 s)
    pub on_time_too_short: TrackedState<bool>,
    /// Powertrain power demand exceeds motor and/or battery capabilities
    pub propulsion_power_demand: TrackedState<bool>,
    /// Powertrain power demand exceeds optimal motor and/or battery output
    pub propulsion_power_demand_soft: TrackedState<bool>,
    /// Aux power demand exceeds battery capability
    pub aux_power_demand: TrackedState<bool>,
    /// SOC is below min buffer so FC is charging RES
    pub charging_for_low_soc: TrackedState<bool>,
    /// buffer at which FC is forced on
    pub soc_fc_on_buffer: TrackedState<si::Ratio>,
}
impl SerdeAPI for RGWDBState {}
impl Init for RGWDBState {}

impl RGWDBState {
    /// If any of the causes are true, engine must be on
    fn fc_on(&self) -> anyhow::Result<bool> {
        Ok(*self.fc_temperature_too_low.get_fresh(|| format_dbg!())?
            || *self.vehicle_speed_too_high.get_fresh(|| format_dbg!())?
            || *self.on_time_too_short.get_fresh(|| format_dbg!())?
            || *self.propulsion_power_demand.get_fresh(|| format_dbg!())?
            || *self
                .propulsion_power_demand_soft
                .get_fresh(|| format_dbg!())?
            || *self.aux_power_demand.get_fresh(|| format_dbg!())?
            || *self.charging_for_low_soc.get_fresh(|| format_dbg!())?)
    }
}

/// Options for controlling simulation behavior
#[derive(Clone, Debug, PartialEq, Deserialize, Serialize)]
#[non_exhaustive]
#[serde(deny_unknown_fields)]
pub struct HEVSimulationParams {
    /// [ReversibleEnergyStorage] per [FuelConverter]
    pub res_per_fuel_lim: si::Ratio,
    /// Threshold of SOC balancing iteration for triggering error
    pub soc_balance_iter_err: u32,
    /// Whether to allow iteration to achieve SOC balance
    pub balance_soc: bool,
    /// Whether to save each SOC balance iteration    
    pub save_soc_bal_iters: bool,
}

impl HEVSimulationParams {
    pub fn new(
        res_per_fuel_lim: si::Ratio,
        soc_balance_iter_err: u32,
        balance_soc: bool,
        save_soc_bal_iters: bool,
    ) -> anyhow::Result<Self> {
        Ok(Self {
            res_per_fuel_lim,
            soc_balance_iter_err,
            balance_soc,
            save_soc_bal_iters,
        })
    }
}

impl Default for HEVSimulationParams {
    fn default() -> Self {
        Self {
            res_per_fuel_lim: uc::R * 0.005,
            soc_balance_iter_err: 5,
            balance_soc: true,
            save_soc_bal_iters: false,
        }
    }
}

#[derive(
    Clone, Debug, PartialEq, Deserialize, Serialize, Default, IsVariant, derive_more::From, TryInto,
)]
pub enum HEVAuxControls {
    /// If feasible, use [ReversibleEnergyStorage] to handle aux power demand
    #[default]
    AuxOnResPriority,
    /// If feasible, use [FuelConverter] to handle aux power demand
    AuxOnFcPriority,
}

#[derive(
    Clone, Debug, PartialEq, Deserialize, Serialize, IsVariant, derive_more::From, TryInto,
)]
pub enum HEVPowertrainControls {
    /// Greedily uses [ReversibleEnergyStorage] with buffers that derate charge
    /// and discharge power inside of static min and max SOC range.  Also, includes
    /// buffer for forcing [FuelConverter] to be active/on.
    RGWDB(Box<RESGreedyWithDynamicBuffers>),
    /// Uses the [ReversibleEnergyStorage] only for supplying auxiliary power.
    /// Also, includes logic for when the [FuelConverter] must be on.
    #[serde(alias = "StopStart")]
    StartStop(Box<HEVStartStopControl>),
}

impl Default for HEVPowertrainControls {
    fn default() -> Self {
        Self::RGWDB(Default::default())
    }
}

impl SetCumulative for HEVPowertrainControls {
    fn set_cumulative<F: Fn() -> String>(&mut self, dt: si::Time, loc: F) -> anyhow::Result<()> {
        match self {
            Self::RGWDB(rgwdb) => {
                rgwdb.set_cumulative(dt, || format!("{}\n{}", loc(), format_dbg!()))?
            }
            Self::StartStop(cntrl) => {
                cntrl.set_cumulative(dt, || format!("{}\n{}", loc(), format_dbg!()))?
            }
        }
        Ok(())
    }

    fn reset_cumulative<F: Fn() -> String>(&mut self, loc: F) -> anyhow::Result<()> {
        match self {
            Self::RGWDB(rgwdb) => {
                rgwdb.reset_cumulative(|| format!("{}\n{}", loc(), format_dbg!()))?
            }
            Self::StartStop(cntrl) => {
                cntrl.reset_cumulative(|| format!("{}\n{}", loc(), format_dbg!()))?
            }
        }
        Ok(())
    }
}
impl Step for HEVPowertrainControls {
    fn step<F: Fn() -> String>(&mut self, loc: F) -> anyhow::Result<()> {
        match self {
            HEVPowertrainControls::RGWDB(rgwdb) => rgwdb.step(loc)?,
            HEVPowertrainControls::StartStop(cntrls) => cntrls.step(loc)?,
        }
        Ok(())
    }

    fn reset_step<F: Fn() -> String>(&mut self, loc: F) -> anyhow::Result<()> {
        match self {
            HEVPowertrainControls::RGWDB(rgwdb) => rgwdb.reset_step(loc)?,
            HEVPowertrainControls::StartStop(cntrls) => cntrls.reset_step(loc)?,
        }
        Ok(())
    }
}

impl StateMethods for HEVPowertrainControls {}

impl SaveState for HEVPowertrainControls {
    fn save_state<F: Fn() -> String>(&mut self, loc: F) -> anyhow::Result<()> {
        match self {
            HEVPowertrainControls::RGWDB(rgwdb) => rgwdb.save_state(loc)?,
            HEVPowertrainControls::StartStop(cntrl) => cntrl.save_state(loc)?,
        }
        Ok(())
    }
}
impl TrackedStateMethods for HEVPowertrainControls {
    fn check_and_reset<F: Fn() -> String>(&mut self, loc: F) -> anyhow::Result<()> {
        match self {
            HEVPowertrainControls::RGWDB(rgwdb) => rgwdb.check_and_reset(loc)?,
            HEVPowertrainControls::StartStop(cntrl) => cntrl.check_and_reset(loc)?,
        }
        Ok(())
    }

    fn mark_fresh<F: Fn() -> String>(&mut self, loc: F) -> anyhow::Result<()> {
        match self {
            HEVPowertrainControls::RGWDB(rgwdb) => rgwdb.mark_fresh(loc)?,
            HEVPowertrainControls::StartStop(cntrl) => cntrl.mark_fresh(loc)?,
        }
        Ok(())
    }
}
impl HistoryMethods for HEVPowertrainControls {
    fn set_save_interval(&mut self, save_interval: Option<usize>) -> anyhow::Result<()> {
        match self {
            HEVPowertrainControls::RGWDB(rgwdb) => Ok(rgwdb.set_save_interval(save_interval)?),
            HEVPowertrainControls::StartStop(cntrl) => Ok(cntrl.set_save_interval(save_interval)?),
        }
    }

    fn save_interval(&self) -> anyhow::Result<Option<usize>> {
        match self {
            HEVPowertrainControls::RGWDB(rgwdb) => rgwdb.save_interval(),
            HEVPowertrainControls::StartStop(cntrl) => cntrl.save_interval(),
        }
    }
    fn clear(&mut self) {
        match self {
            HEVPowertrainControls::RGWDB(rgwdb) => rgwdb.clear(),
            HEVPowertrainControls::StartStop(cntrl) => cntrl.clear(),
        }
    }
}

impl Init for HEVPowertrainControls {
    fn init(&mut self) -> Result<(), Error> {
        match self {
            Self::RGWDB(rgwb) => rgwb.init()?,
            Self::StartStop(cntrl) => cntrl.init()?,
        }
        Ok(())
    }
}

impl HEVPowertrainControls {
    /// Determines power split between engine and electric machine
    ///
    /// # Arguments
    /// - `pwr_prop_req`: tractive power required
    /// - `veh_state`: vehicle state
    /// - `hev_state`: HEV powertrain state
    /// - `fc`: fuel converter
    /// - `em_state`: electric machine state
    /// - `res`: reversible energy storage (e.g. high voltage battery)
    fn get_pwr_fc_and_em(
        &mut self,
        pwr_prop_req: si::Power,
        fc: &FuelConverter,
        em_state: &ElectricMachineState,
        res: &ReversibleEnergyStorage,
    ) -> anyhow::Result<(si::Power, si::Power)> {
        let fc_state = &fc.state;
        ensure!(
            // `almost` is in case of negligible numerical precision discrepancies
            almost_le_uom(
                &pwr_prop_req,
                &(*em_state.pwr_mech_fwd_out_max.get_fresh(|| format_dbg!())?
                    + *fc_state.pwr_prop_max.get_fresh(|| format_dbg!())?),
                None
            ),
            "{}
`pwr_out_req`: {} kW
`em_state.pwr_mech_fwd_out_max`: {} kW
`fc_state.pwr_prop_max`: {} kW
`res.state.soc`: {}",
            format_dbg!(),
            pwr_prop_req.get::<si::kilowatt>(),
            em_state
                .pwr_mech_fwd_out_max
                .get_fresh(|| format_dbg!())?
                .get::<si::kilowatt>(),
            fc_state
                .pwr_prop_max
                .get_fresh(|| format_dbg!())?
                .get::<si::kilowatt>(),
            res.state
                .soc
                .get_fresh(|| format_dbg!())?
                .get::<si::ratio>()
        );

        // # Brain dump for thermal stuff
        // TODO: engine on/off w.r.t. thermal stuff should not come into play
        // if there is no component (e.g. cabin) demanding heat from the engine.  My 2019
        // Hyundai Ioniq will turn the engine off if there is no heat demand regardless of
        // the coolant temperature
        // TODO: make sure idle fuel gets converted to heat correctly

        match self {
            Self::RGWDB(rgwdb) => rgwdb.get_pwr_fc_and_em(fc, pwr_prop_req, em_state),
            Self::StartStop(cntrl) => cntrl.get_pwr_fc_and_em(fc, pwr_prop_req, em_state),
        }
    }

    pub fn fc_on(&self) -> anyhow::Result<bool> {
        match self {
            Self::RGWDB(rgwdb) => rgwdb.state.fc_on(),
            Self::StartStop(cntrl) => cntrl.state.fc_on(),
        }
    }

    pub fn handle_fc_on_causes_for_speed(&mut self, speed: si::Velocity) -> anyhow::Result<()> {
        match self {
            Self::StartStop(cntrl) => HEVStartStopControl::handle_fc_on_causes_for_speed(
                &mut cntrl.state.vehicle_not_stopped,
                speed,
                cntrl.stopped_speed_threshold,
            )?,
            _ => (),
        }
        Ok(())
    }
}

/// Greedily uses [ReversibleEnergyStorage] with buffers that derate charge
/// and discharge power inside of static min and max SOC range.  Also, includes
/// buffer for forcing [FuelConverter] to be active/on. See [Self::init] for
/// default values.
#[serde_api]
#[derive(Clone, Debug, PartialEq, Deserialize, Serialize, Default, StateMethods, SetCumulative)]
#[cfg_attr(feature = "pyo3", pyclass(module = "fastsim", subclass, eq))]
#[non_exhaustive]
#[serde(deny_unknown_fields)]
pub struct RESGreedyWithDynamicBuffers {
    /// RES energy delta from minimum SOC corresponding to kinetic energy of
    /// vehicle at this speed that triggers ramp down in RES discharge.
    pub speed_soc_disch_buffer: Option<si::Velocity>,
    /// Coefficient for modifying amount of accel buffer
    pub speed_soc_disch_buffer_coeff: Option<si::Ratio>,
    /// RES energy delta from minimum SOC corresponding to kinetic energy of
    /// vehicle at this speed that triggers FC to be forced on.
    pub speed_soc_fc_on_buffer: Option<si::Velocity>,
    /// Coefficient for modifying amount of [Self::speed_soc_fc_on_buffer]
    pub speed_soc_fc_on_buffer_coeff: Option<si::Ratio>,
    /// RES energy delta from maximum SOC corresponding to kinetic energy of
    /// vehicle at current speed minus kinetic energy of vehicle at this speed
    /// triggers ramp down in RES discharge
    pub speed_soc_regen_buffer: Option<si::Velocity>,
    /// Coefficient for modifying amount of regen buffer
    pub speed_soc_regen_buffer_coeff: Option<si::Ratio>,
    /// Minimum time engine must remain on if it was on during the previous
    /// simulation time step.
    pub fc_min_time_on: Option<si::Time>,
    /// Speed at which [FuelConverter] is forced on.
    pub speed_fc_forced_on: Option<si::Velocity>,
    /// Fraction of total aux and powertrain rated power at which
    /// [FuelConverter] is forced on.
    pub frac_pwr_demand_fc_forced_on: Option<si::Ratio>,
    /// Force engine, if on, to run at this fraction of power at which peak
    /// efficiency occurs or the required power, whichever is greater. If SOC is
    /// below min buffer or engine is otherwise forced on and battery has room
    /// to receive charge, engine will run at this level and charge.
    pub frac_of_most_eff_pwr_to_run_fc: Option<si::Ratio>,
    /// Fraction of available charging capacity to use toward running the engine
    /// efficiently.
    /// Time step interval between saves. 1 is a good option. If None, no saving occurs.
    pub save_interval: Option<usize>,
    /// temperature at which engine is forced on to warm up
    #[serde(default)]
    pub temp_fc_forced_on: Option<si::Temperature>,
    /// temperature at which engine is allowed to turn off due to being sufficiently warm
    #[serde(default)]
    pub temp_fc_allowed_off: Option<si::Temperature>,
    /// current state of control variables
    #[serde(default)]
    #[is_state]
    pub state: RGWDBState,
    /// history of current state
    #[serde(default, skip_serializing_if = "RGWDBStateHistoryVec::is_empty")]
    pub history: RGWDBStateHistoryVec,
}

#[pyo3_api]
impl RESGreedyWithDynamicBuffers {}

impl RESGreedyWithDynamicBuffers {
    pub fn new(
        speed_soc_disch_buffer: Option<si::Velocity>,
        speed_soc_disch_buffer_coeff: Option<si::Ratio>,
        speed_soc_fc_on_buffer: Option<si::Velocity>,
        speed_soc_fc_on_buffer_coeff: Option<si::Ratio>,
        speed_soc_regen_buffer: Option<si::Velocity>,
        speed_soc_regen_buffer_coeff: Option<si::Ratio>,
        fc_min_time_on: Option<si::Time>,
        speed_fc_forced_on: Option<si::Velocity>,
        frac_pwr_demand_fc_forced_on: Option<si::Ratio>,
        frac_of_most_eff_pwr_to_run_fc: Option<si::Ratio>,
        temp_fc_forced_on: Option<si::Temperature>,
        temp_fc_allowed_off: Option<si::Temperature>,
        save_interval: Option<usize>,
    ) -> anyhow::Result<Self> {
        let mut res_greedy_w_dynamic_buffers = Self {
            speed_soc_disch_buffer,
            speed_soc_disch_buffer_coeff,
            speed_soc_fc_on_buffer,
            speed_soc_fc_on_buffer_coeff,
            speed_soc_regen_buffer,
            speed_soc_regen_buffer_coeff,
            fc_min_time_on,
            speed_fc_forced_on,
            frac_pwr_demand_fc_forced_on,
            frac_of_most_eff_pwr_to_run_fc,
            temp_fc_forced_on,
            temp_fc_allowed_off,
            state: RGWDBState::default(),
            history: RGWDBStateHistoryVec::default(),
            save_interval,
        };
        res_greedy_w_dynamic_buffers.init()?;
        Ok(res_greedy_w_dynamic_buffers)
    }
}

impl HistoryMethods for RESGreedyWithDynamicBuffers {
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

impl Init for RESGreedyWithDynamicBuffers {
    fn init(&mut self) -> Result<(), Error> {
        // TODO: make sure these values propagate to the documented defaults above
        init_opt_default!(self, speed_soc_disch_buffer, 50.0 * uc::MPH);
        init_opt_default!(self, speed_soc_disch_buffer_coeff, 1.0 * uc::R);
        init_opt_default!(
            self,
            speed_soc_fc_on_buffer,
            self.speed_soc_disch_buffer.unwrap() * 1.2
        );
        init_opt_default!(self, speed_soc_fc_on_buffer_coeff, 1.0 * uc::R);
        init_opt_default!(self, speed_soc_regen_buffer, 30. * uc::MPH);
        init_opt_default!(self, speed_soc_regen_buffer_coeff, 1.0 * uc::R);
        init_opt_default!(self, fc_min_time_on, uc::S * 5.0);
        init_opt_default!(self, speed_fc_forced_on, uc::MPH * 75.);
        init_opt_default!(self, frac_pwr_demand_fc_forced_on, uc::R * 0.75);
        init_opt_default!(self, frac_of_most_eff_pwr_to_run_fc, 1.0 * uc::R);
        Ok(())
    }
}
impl SerdeAPI for RESGreedyWithDynamicBuffers {}

impl RESGreedyWithDynamicBuffers {
    fn get_pwr_fc_and_em(
        &mut self,
        fc: &FuelConverter,
        pwr_prop_req: si::Power,
        em_state: &ElectricMachineState,
    ) -> anyhow::Result<(si::Power, si::Power)> {
        // Tractive power `em` must provide before deciding power
        // split, cannot exceed ElectricMachine max output power.
        // Excess demand will be handled by `fc`.  Favors drawing power from
        // `em` before engine
        let em_pwr = pwr_prop_req
            .min(*em_state.pwr_mech_fwd_out_max.get_fresh(|| format_dbg!())?)
            .max(-*em_state.pwr_mech_regen_max.get_fresh(|| format_dbg!())?);
        // tractive power handled by fc
        let (fc_pwr, em_pwr) = if !self.state.fc_on()? {
            // engine is off, and `em_pwr` has already been limited within bounds
            (si::Power::ZERO, em_pwr)
        } else {
            // engine has been forced on
            let frac_of_pwr_for_peak_eff: si::Ratio = self
                .frac_of_most_eff_pwr_to_run_fc
                .with_context(|| format_dbg!())?;
            let fc_pwr = if pwr_prop_req < si::Power::ZERO {
                // negative tractive power
                // max power system can receive from engine during negative traction
                (*em_state.pwr_mech_regen_max.get_fresh(|| format_dbg!())? + pwr_prop_req)
                    // or peak efficiency power if it's lower than above
                    .min(fc.pwr_for_peak_eff * frac_of_pwr_for_peak_eff)
                    // but not negative
                    .max(si::Power::ZERO)
            } else {
                // positive tractive power
                if pwr_prop_req - em_pwr > fc.pwr_for_peak_eff * frac_of_pwr_for_peak_eff {
                    // engine needs to run higher than peak efficiency point
                    pwr_prop_req - em_pwr
                } else {
                    // engine does not need to run higher than peak
                    // efficiency point to make tractive demand

                    // fc handles all power not covered by em
                    (pwr_prop_req - em_pwr)
                        // and if that's less than the
                        // efficiency-focused value, then operate at
                        // that value
                        .max(fc.pwr_for_peak_eff * frac_of_pwr_for_peak_eff)
                        // but don't exceed what what the battery can
                        // absorb + tractive demand
                        .min(
                            pwr_prop_req
                                + *em_state.pwr_mech_regen_max.get_fresh(|| format_dbg!())?,
                        )
                }
            }
            // and don't exceed what the fc can do
            .min(*fc.state.pwr_prop_max.get_fresh(|| format_dbg!())?);

            // recalculate `em_pwr` based on `fc_pwr`
            let em_pwr_corrected = (pwr_prop_req - fc_pwr)
                .max(-*em_state.pwr_mech_regen_max.get_fresh(|| format_dbg!())?);
            (fc_pwr, em_pwr_corrected)
        };
        Ok((fc_pwr, em_pwr))
    }

    fn handle_fc_on_causes(
        &mut self,
        fc: &FuelConverter,
        veh_state: &VehicleState,
        res: &ReversibleEnergyStorage,
        em_state: &ElectricMachineState,
    ) -> Result<(), anyhow::Error> {
        self.handle_fc_on_causes_for_temp(fc)?;
        self.handle_fc_on_causes_for_speed(veh_state)?;
        self.handle_fc_on_causes_for_low_soc(res, veh_state)?;
        self.handle_fc_on_causes_for_pwr_demand(
            *veh_state
                .pwr_tractive
                .get_stale(|| format_dbg!(veh_state.pwr_tractive))?,
            em_state,
            &fc.state,
        )
        .with_context(|| format_dbg!())?;
        self.handle_fc_on_causes_for_on_time(fc)?;
        Ok(())
    }

    fn handle_fc_on_causes_for_on_time(&mut self, fc: &FuelConverter) -> Result<(), anyhow::Error> {
        self.state.on_time_too_short.update(*fc.state.fc_on.get_stale(|| format_dbg!())? && *fc.state.time_on.get_stale(|| format_dbg!())?
                    < self.fc_min_time_on.with_context(|| {
                    anyhow!(
                        "{}\n Expected `ResGreedyWithBuffers::init` to have been called beforehand.",
                        format_dbg!()
                    )
                })?, || format_dbg!())?;
        Ok(())
    }

    /// Determines whether power demand requires engine to be on.  Not needed during
    /// negative traction.
    fn handle_fc_on_causes_for_pwr_demand(
        &mut self,
        pwr_out_req_for_cyc: si::Power,
        em_state: &ElectricMachineState,
        fc_state: &FuelConverterState,
    ) -> Result<(), anyhow::Error> {
        let frac_pwr_demand_fc_forced_on: si::Ratio = self
            .frac_pwr_demand_fc_forced_on
            .with_context(|| format_dbg!())?;
        self.state.propulsion_power_demand_soft.update(
            pwr_out_req_for_cyc
                > frac_pwr_demand_fc_forced_on
                    * (*em_state.pwr_mech_fwd_out_max.get_stale(|| format_dbg!())?
                        + *fc_state.pwr_out_max.get_stale(|| format_dbg!())?),
            || format_dbg!(),
        )?;
        self.state.propulsion_power_demand.update(
            pwr_out_req_for_cyc - *em_state.pwr_mech_fwd_out_max.get_stale(|| format_dbg!())?
                >= si::Power::ZERO,
            || format_dbg!(),
        )?;
        Ok(())
    }

    /// Detemrines whether engine must be on to charge battery
    fn handle_fc_on_causes_for_low_soc(
        &mut self,
        res: &ReversibleEnergyStorage,
        veh_state: &VehicleState,
    ) -> anyhow::Result<()> {
        self.state.soc_fc_on_buffer.update(
            {
                let energy_delta_to_buffer_speed: si::Energy = 0.5
                    * *veh_state.mass.get_fresh(|| format_dbg!())?
                    * (self
                        .speed_soc_fc_on_buffer
                        .with_context(|| format_dbg!())?
                        .powi(P2::new())
                        - veh_state
                            .speed_ach
                            .get_stale(|| format_dbg!())?
                            .powi(P2::new()));
                energy_delta_to_buffer_speed.max(si::Energy::ZERO)
                    * self
                        .speed_soc_fc_on_buffer_coeff
                        .with_context(|| format_dbg!())?
            } / res.energy_capacity_usable()
                + res.min_soc,
            || format_dbg!(),
        )?;
        self.state.charging_for_low_soc.update(
            *res.state.soc.get_stale(|| format_dbg!())?
                < *self.state.soc_fc_on_buffer.get_fresh(|| format_dbg!())?,
            || format_dbg!(),
        )?;
        Ok(())
    }

    /// Determines whether enigne must be on for high speed
    fn handle_fc_on_causes_for_speed(&mut self, veh_state: &VehicleState) -> anyhow::Result<()> {
        self.state.vehicle_speed_too_high.update(
            *veh_state.speed_ach.get_stale(|| format_dbg!())?
                > self.speed_fc_forced_on.with_context(|| format_dbg!())?,
            || format_dbg!(),
        )?;
        Ok(())
    }

    /// Determines whether engine needs to be on due to low temperature and pushes
    /// appropriate variant to `fc_on_causes`
    fn handle_fc_on_causes_for_temp(&mut self, fc: &FuelConverter) -> anyhow::Result<()> {
        match (
            match fc.temperature() {
                Some(fct) => Some(*fct.get_fresh(|| format_dbg!())?),
                None => None,
            },
            match fc.temperature() {
                Some(fct) => Some(*fct.get_fresh(|| format_dbg!())?),
                None => None,
            },
            self.temp_fc_forced_on,
            self.temp_fc_allowed_off,
        ) {
            (None, None, None, None) => {
                self.state
                    .fc_temperature_too_low
                    .update(false, || format_dbg!())?;
            }
            (
                Some(temperature),
                Some(temp_prev),
                Some(temp_fc_forced_on),
                Some(temp_fc_allowed_off),
            ) => {
                self.state.fc_temperature_too_low.update(
                    // temperature is currently below forced on threshold
                    temperature < temp_fc_forced_on ||
            // temperature was below forced on threshold and still has not exceeded allowed off threshold
            (temp_prev < temp_fc_forced_on && temperature < temp_fc_allowed_off),
                    || format_dbg!(),
                )?;
            }
            _ => {
                bail!(
                    "{}\n`fc.temperature()`, `fc.temp_prev()`, `self.temp_fc_forced_on`, and 
`self.temp_fc_allowed_off` must all be `None` or `Some` because these controls are necessary
for an HEV equipped with thermal models or superfluous otherwise",
                    format_dbg!((
                        fc.temperature(),
                        self.temp_fc_forced_on,
                        self.temp_fc_allowed_off
                    ))
                );
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
#[non_exhaustive]
#[serde(deny_unknown_fields)]
#[is_state]
pub struct StartStopState {
    /// time step index
    pub i: TrackedState<usize>,
    /// Engine must be on to self heat if thermal model is enabled
    pub fc_temperature_too_low: TrackedState<bool>,
    /// Engine start-stop can only happen while vehicle is stopped
    pub vehicle_not_stopped: TrackedState<bool>,
    /// Engine has not been on long enough (usually 30 s)
    pub on_time_too_short: TrackedState<bool>,
    /// Aux power demand exceeds battery capability
    pub aux_power_demand: TrackedState<bool>,
    /// SOC is below min buffer so FC is charging RES
    pub charging_for_low_soc: TrackedState<bool>,
    /// The total time vehicle has been stopped
    pub time_vehicle_stopped: TrackedState<si::Time>,
    /// Vehicle stopped time
    pub vehicle_not_stopped_long_enough: TrackedState<bool>,
    /// Vehicle has a request for traction power for the current timestep
    pub has_traction_power_request: TrackedState<bool>,
}

impl StartStopState {
    /// If any of the causes are true, engine must be on
    fn fc_on(&self) -> anyhow::Result<bool> {
        Ok(*self.fc_temperature_too_low.get_fresh(|| format_dbg!())?
            || *self.vehicle_not_stopped.get_fresh(|| format_dbg!())?
            || *self.on_time_too_short.get_fresh(|| format_dbg!())?
            || *self.aux_power_demand.get_fresh(|| format_dbg!())?
            || *self.charging_for_low_soc.get_fresh(|| format_dbg!())?
            || *self
                .vehicle_not_stopped_long_enough
                .get_fresh(|| format_dbg!())?
            || *self
                .has_traction_power_request
                .get_fresh(|| format_dbg!())?)
    }
}

#[serde_api]
#[derive(Clone, Debug, PartialEq, Deserialize, Serialize, StateMethods, SetCumulative)]
#[cfg_attr(feature = "pyo3", pyclass(module = "fastsim", subclass, eq))]
#[non_exhaustive]
#[serde(deny_unknown_fields)]
pub struct HEVStartStopControl {
    /// Minimum time engine must remain on if it was on during the previous
    /// simulation time step.
    pub fc_min_time_on: Option<si::Time>,
    /// The range of usable SOC of the storage system below which the
    /// [FuelConverter] is forced on.
    pub soc_fc_forced_on: Option<si::Ratio>,
    /// Force engine, if on, to run at this fraction of power at which peak
    /// efficiency occurs or the required power, whichever is greater. If SOC is
    /// below min buffer or engine is otherwise forced on and battery has room
    /// to receive charge, engine will run at this level and charge.
    pub frac_of_most_eff_pwr_to_run_fc: Option<si::Ratio>,
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
    #[serde(default = "HEVStartStopControl::def_stopped_speed_threshold")]
    pub stopped_speed_threshold: si::Velocity,
    /// If true, the electric machine can recharge from regenerative braking
    pub em_can_regen: Option<bool>,
    #[serde(default)]
    /// Time step interval between saves. 1 is a good option. If None, no saving occurs.
    pub save_interval: Option<usize>,
    /// current state of control variables
    #[serde(default)]
    #[is_state]
    pub state: StartStopState,
    /// history of current state
    #[serde(default, skip_serializing_if = "StartStopStateHistoryVec::is_empty")]
    pub history: StartStopStateHistoryVec,
}

#[pyo3_api]
impl HEVStartStopControl {}

impl HistoryMethods for HEVStartStopControl {
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

impl Init for HEVStartStopControl {
    fn init(&mut self) -> Result<(), Error> {
        init_opt_default!(self, fc_min_time_on, 5.0 * uc::S);
        init_opt_default!(self, soc_fc_forced_on, 0.1 * uc::R);
        init_opt_default!(self, frac_of_most_eff_pwr_to_run_fc, 1.0 * uc::R);
        init_opt_default!(
            self,
            time_delay_after_stop_until_fc_can_turn_off,
            0.0 * uc::S
        );
        Ok(())
    }
}

impl SerdeAPI for HEVStartStopControl {}

impl Default for HEVStartStopControl {
    fn default() -> Self {
        Self {
            fc_min_time_on: Option::default(),
            soc_fc_forced_on: Option::default(),
            frac_of_most_eff_pwr_to_run_fc: Option::default(),
            temp_fc_forced_on: Option::default(),
            temp_fc_allowed_off: Option::default(),
            time_delay_after_stop_until_fc_can_turn_off: Option::default(),
            stopped_speed_threshold: Self::def_stopped_speed_threshold(),
            em_can_regen: Option::default(),
            save_interval: Option::default(),
            state: StartStopState::default(),
            history: StartStopStateHistoryVec::default(),
        }
    }
}

impl StartStopControl for HEVStartStopControl {}

impl HEVStartStopControl {
    fn def_stopped_speed_threshold() -> si::Velocity {
        0.05 * uc::MPS
    }

    pub fn new(
        fc_min_time_on: Option<si::Time>,
        soc_fc_forced_on: Option<si::Ratio>,
        frac_of_most_eff_pwr_to_run_fc: Option<si::Ratio>,
        temp_fc_forced_on: Option<si::Temperature>,
        temp_fc_allowed_off: Option<si::Temperature>,
        time_delay_after_stop_until_fc_can_turn_off: Option<si::Time>,
        em_can_regen: Option<bool>,
        save_interval: Option<usize>,
    ) -> anyhow::Result<Self> {
        let mut result = Self {
            fc_min_time_on,
            soc_fc_forced_on,
            frac_of_most_eff_pwr_to_run_fc,
            temp_fc_forced_on,
            temp_fc_allowed_off,
            time_delay_after_stop_until_fc_can_turn_off,
            stopped_speed_threshold: Self::def_stopped_speed_threshold(),
            em_can_regen,
            save_interval,
            state: StartStopState::default(),
            history: StartStopStateHistoryVec::default(),
        };
        result.init()?;
        Ok(result)
    }

    fn get_pwr_fc_and_em(
        &mut self,
        fc: &FuelConverter,
        pwr_prop_req: si::Power,
        em_state: &ElectricMachineState,
    ) -> anyhow::Result<(si::Power, si::Power)> {
        let no_prop_pwr_demand = pwr_prop_req == si::Power::ZERO;
        let em_can_regen = self.em_can_regen.unwrap_or(true);
        let em_pwr = pwr_prop_req.min(si::Power::ZERO).max(if em_can_regen {
            -*em_state.pwr_mech_regen_max.get_fresh(|| format_dbg!())?
        } else {
            si::Power::ZERO
        });
        let (fc_pwr, em_pwr) = {
            // engine is on or forced on if tractive effort is required
            let frac_of_pwr_for_peak_eff: si::Ratio = self
                .frac_of_most_eff_pwr_to_run_fc
                .with_context(|| format_dbg!())?;
            let fc_pwr = if pwr_prop_req < si::Power::ZERO {
                // negative tractive power
                // max power system can receive from engine during negative traction
                (*em_state.pwr_mech_regen_max.get_fresh(|| format_dbg!())? + pwr_prop_req)
                    // or peak efficiency power if it's lower than above
                    .min(fc.pwr_for_peak_eff * frac_of_pwr_for_peak_eff)
                    // but not negative
                    .max(si::Power::ZERO)
            } else if no_prop_pwr_demand {
                // no propulsion power needed. Allow for engine-off
                // as much as possible.
                // TODO: take into consideration RESS SOC and aux loads?
                0.0 * uc::W
            } else {
                // positive tractive power
                if pwr_prop_req - em_pwr > fc.pwr_for_peak_eff * frac_of_pwr_for_peak_eff {
                    // engine needs to run higher than peak efficiency point
                    pwr_prop_req - em_pwr
                } else {
                    // engine does not need to run higher than peak
                    // efficiency point to make tractive demand

                    // fc handles all power not covered by em
                    (pwr_prop_req - em_pwr)
                        // and if that's less than the
                        // efficiency-focused value, then operate at
                        // that value
                        .max(fc.pwr_for_peak_eff * frac_of_pwr_for_peak_eff)
                        // but don't exceed what the battery can
                        // absorb + tractive demand
                        .min(
                            pwr_prop_req
                                + *em_state.pwr_mech_regen_max.get_fresh(|| format_dbg!())?,
                        )
                }
            }
            // and don't exceed what the fc can do
            .min(*fc.state.pwr_prop_max.get_fresh(|| format_dbg!())?);

            // recalculate `em_pwr` based on `fc_pwr`
            let em_pwr_corrected = (pwr_prop_req - fc_pwr).max(if em_can_regen {
                -*em_state.pwr_mech_regen_max.get_fresh(|| format_dbg!())?
            } else {
                si::Power::ZERO
            });
            (fc_pwr, em_pwr_corrected)
        };
        Self::handle_fc_on_causes_for_propulsion_request(
            &mut self.state.has_traction_power_request,
            fc_pwr,
        )?;
        Ok((fc_pwr, em_pwr))
    }

    pub fn handle_fc_on_causes(
        &mut self,
        fc: &FuelConverter,
        veh_state: &VehicleState,
        res: &ReversibleEnergyStorage,
        dt: si::Time,
    ) -> Result<(), anyhow::Error> {
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
        self.handle_fc_on_causes_for_low_soc(res)?;
        Self::handle_fc_on_causes_for_on_time(
            fc,
            self.fc_min_time_on,
            &mut self.state.on_time_too_short,
        )?;
        Ok(())
    }

    fn handle_fc_on_causes_for_low_soc(
        &mut self,
        res: &ReversibleEnergyStorage,
    ) -> anyhow::Result<()> {
        let soc_fc_forced_on = if let Some(soc_frac) = self.soc_fc_forced_on {
            soc_frac * (res.max_soc - res.min_soc) + res.min_soc
        } else {
            0.1 * (res.max_soc - res.min_soc) + res.min_soc
        };
        self.state.charging_for_low_soc.update(
            *res.state.soc.get_stale(|| format_dbg!())? < soc_fc_forced_on,
            || format_dbg!(),
        )?;
        Ok(())
    }
}
