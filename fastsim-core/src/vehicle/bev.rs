use super::*;

#[serde_api]
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, StateMethods, SetCumulative)]
#[non_exhaustive]
#[serde(deny_unknown_fields)]
#[cfg_attr(feature = "pyo3", pyclass(module = "fastsim", subclass, eq))]
/// Battery electric vehicle
pub struct BatteryElectricVehicle {
    #[has_state]
    pub res: ReversibleEnergyStorage,
    #[has_state]
    pub em: ElectricMachine,
    #[has_state]
    pub transmission: Transmission,
    pub(crate) mass: Option<si::Mass>,
}

#[pyo3_api]
impl BatteryElectricVehicle {}

impl BatteryElectricVehicle {
    pub fn new(
        res: ReversibleEnergyStorage,
        em: ElectricMachine,
        transmission: Transmission,
        mass: Option<si::Mass>,
    ) -> anyhow::Result<Self> {
        let mut bev = Self {
            res,
            em,
            transmission,
            mass,
        };
        bev.init()?;
        Ok(bev)
    }
}

impl Init for BatteryElectricVehicle {
    fn init(&mut self) -> Result<(), Error> {
        self.res
            .init()
            .map_err(|err| Error::InitError(format_dbg!(err)))?;
        self.em
            .init()
            .map_err(|err| Error::InitError(format_dbg!(err)))?;
        self.transmission
            .init()
            .map_err(|err| Error::InitError(format_dbg!(err)))?;
        Ok(())
    }
}

impl SerdeAPI for BatteryElectricVehicle {}

impl Mass for BatteryElectricVehicle {
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
                stringify!(BatteryElectricVehicle)
            ),
        };
        ensure!(
            self.mass > Some(0.0 * uc::KG),
            "{} mass must be positive",
            stringify!(BatteryElectricVehicle)
        );
        Ok(())
    }

    fn derived_mass(&self) -> anyhow::Result<Option<si::Mass>> {
        let res_mass = self.res.mass().with_context(|| anyhow!(format_dbg!()))?;
        let em_mass = self.em.mass().with_context(|| anyhow!(format_dbg!()))?;
        let transmission_mass = self
            .transmission
            .mass()
            .with_context(|| anyhow!(format_dbg!()))?;
        match (res_mass, em_mass, transmission_mass) {
            (Some(res_mass), Some(em_mass), Some(transmission_mass)) => {
                Ok(Some(em_mass + res_mass + transmission_mass))
            }
            (None, None, None) => Ok(None),
            _ => bail!(
                "`{}` field masses are not consistently set to `Some` or `None`",
                stringify!(BatteryElectricVehicle)
            ),
        }
    }

    fn expunge_mass_fields(&mut self) {
        self.res.expunge_mass_fields();
        self.em.expunge_mass_fields();
        self.transmission.expunge_mass_fields();
        self.mass = None;
    }
}

impl HistoryMethods for BatteryElectricVehicle {
    fn save_interval(&self) -> anyhow::Result<Option<usize>> {
        bail!("`save_interval` is not implemented in BatteryElectricVehicle")
    }
    fn set_save_interval(&mut self, save_interval: Option<usize>) -> anyhow::Result<()> {
        self.res.set_save_interval(save_interval)?;
        self.em.set_save_interval(save_interval)?;
        self.transmission.set_save_interval(save_interval)?;
        Ok(())
    }
    fn clear(&mut self) {
        self.res.clear();
        self.em.clear();
        self.transmission.clear();
    }
}

impl Powertrain for BatteryElectricVehicle {
    fn solve(
        &mut self,
        pwr_out_req: si::Power,
        _enabled: bool,
        dt: si::Time,
    ) -> anyhow::Result<Option<si::Power>> {
        let pwr_in_transmission = self
            .transmission
            .solve(pwr_out_req, true, dt)
            .with_context(|| format_dbg!())?
            .with_context(|| format!("{}\nExpected `Some`", format_dbg!()))?;
        let pwr_in_em = self
            .em
            .solve(pwr_in_transmission, true, dt)
            .map_err(|err| anyhow::anyhow!(
                format!(
                    "error at line {}: \ntransmission `pwr_out_req`: {} kW\n`self.transmission.state.pwr_out_fwd_max`: {} kW \n with originating error [{}]", 
                    format_dbg!(),
                    pwr_out_req.get::<si::kilowatt>().format_eng(None),
                    self.transmission
                        .state
                        .pwr_out_fwd_max
                        .get_fresh(|| format_dbg!())
                        .unwrap()
                        .get::<si::kilowatt>()
                        .format_eng(None), err)))?
            .with_context(|| format!("{}\nExpected `Some`", format_dbg!()))?;
        self.res
            .solve(pwr_in_em, dt)
            .with_context(|| format_dbg!())?;
        Ok(None)
    }

    fn get_curr_pwr_prop_out_max(&self) -> anyhow::Result<(si::Power, si::Power)> {
        self.transmission
            .get_curr_pwr_prop_out_max()
            .with_context(|| format_dbg!())
    }

    fn set_curr_pwr_prop_out_max(
        &mut self,
        _pwr_upstream: (si::Power, si::Power),
        pwr_aux: si::Power,
        dt: si::Time,
        _veh_state: &VehicleState,
    ) -> anyhow::Result<()> {
        // TODO: account for transmission efficiency in here
        let disch_buffer = si::Energy::ZERO;
        let chrg_buffer = si::Energy::ZERO;
        self.res
            .set_curr_pwr_out_max(dt, disch_buffer, chrg_buffer)
            .with_context(|| anyhow!(format_dbg!()))?;
        let aux_supply_eff = match &self.res.aux_supply_eff {
            AuxSupplyEfficiency::Constant(interp) => interp.interpolate(&[]),
        }?;
        self.res
            .set_curr_pwr_prop_max(pwr_aux / aux_supply_eff)
            .with_context(|| anyhow!(format_dbg!()))?;
        self.em
            .set_curr_pwr_prop_out_max(
                self.res
                    .get_curr_pwr_prop_out_max()
                    .with_context(|| format_dbg!())?,
                f64::NAN * uc::W,
                dt,
                _veh_state,
            )
            .with_context(|| anyhow!(format_dbg!()))?;
        self.transmission
            .set_curr_pwr_prop_out_max(
                self.em
                    .get_curr_pwr_prop_out_max()
                    .with_context(|| format_dbg!())?,
                f64::NAN * uc::W,
                dt,
                _veh_state,
            )
            .with_context(|| anyhow!(format_dbg!()))?;

        Ok(())
    }

    /// Regen braking power, positive means braking is happening
    fn pwr_regen(&self) -> anyhow::Result<si::Power> {
        // When `pwr_mech_prop_out` is negative, regen is happening.  First, clip it at 0, and then negate it.
        // see https://play.rust-lang.org/?version=stable&mode=debug&edition=2021&gist=e8f7af5a6e436dd1163fa3c70931d18d
        // for example
        self.transmission.pwr_regen().with_context(|| format_dbg!())
    }
}

impl BatteryElectricVehicle {
    /// Solve change in temperature and other thermal effects
    /// # Arguments
    /// - `te_amb`: ambient temperature
    /// - `pwr_thrml_hvac_to_res`: thermal power flowing from [Vehicle::hvac] system to [ReversibleEnergyStorage::thrml]
    /// - `te_cab`: cabin temperature for heat transfer interaction with [ReversibleEnergyStorage]
    /// - `dt`: simulation time step size
    pub fn solve_thermal(
        &mut self,
        te_amb: si::Temperature,
        pwr_thrml_hvac_to_res: si::Power,
        te_cab: Option<si::Temperature>,
        dt: si::Time,
    ) -> anyhow::Result<()> {
        self.res
            .solve_thermal(te_amb, pwr_thrml_hvac_to_res, te_cab, dt)
            .with_context(|| format_dbg!())?;
        Ok(())
    }
}
