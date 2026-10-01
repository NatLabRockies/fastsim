use super::*;

#[serde_api]
#[derive(Deserialize, Serialize, Debug, Clone, PartialEq, StateMethods, SetCumulative)]
#[non_exhaustive]
#[serde(deny_unknown_fields)]
pub struct Transmission {
    /// Transmission mass
    #[serde(default)]
    pub(crate) mass: Option<si::Mass>,

    /// interpolator for calculating [Self] efficiency as a function of the following variants:  
    /// - 0d -- constant
    #[serde(serialize_with = "serialize_nested")]
    pub eff_interp: InterpolatorEnum<f64>,
    /// struct for tracking current state
    #[serde(default)]
    pub state: TransmissionState,
    /// Custom vector of [Self::state]
    #[serde(default, skip_serializing_if = "TransmissionStateHistoryVec::is_empty")]
    pub history: TransmissionStateHistoryVec,
    /// time step interval between saves. 1 is a good option. If None, no saving occurs.
    pub save_interval: Option<usize>,
}

impl Transmission {
    /// Constructor for Transmission
    pub fn new(
        mass: Option<si::Mass>,
        eff_interp: InterpolatorEnum<f64>,
        save_interval: Option<usize>,
    ) -> anyhow::Result<Self> {
        let mut transmission = Self {
            mass,
            eff_interp,
            state: Default::default(),
            history: Default::default(),
            save_interval,
        };
        transmission.init()?;
        Ok(transmission)
    }
}

impl Powertrain for Transmission {
    fn set_curr_pwr_prop_out_max(
        &mut self,
        pwr_upstream: (si::Power, si::Power),
        _pwr_aux: si::Power,
        _dt: si::Time,
        _veh_state: &VehicleState,
    ) -> anyhow::Result<()> {
        self.state.pwr_out_fwd_max.update(
            pwr_upstream.0
                * (self
                    .eff_interp
                    .interpolate(&[])
                    .with_context(|| format_dbg!())?
                    * uc::R),
            || format_dbg!(),
        )?;
        self.state.pwr_out_regen_max.update(
            pwr_upstream.1
                * (self
                    .eff_interp
                    .interpolate(&[])
                    .with_context(|| format_dbg!())?
                    * uc::R),
            || format_dbg!(),
        )?;
        Ok(())
    }

    fn get_curr_pwr_prop_out_max(&self) -> anyhow::Result<(si::Power, si::Power)> {
        Ok((
            *self.state.pwr_out_fwd_max.get_fresh(|| format_dbg!())?,
            *self.state.pwr_out_regen_max.get_fresh(|| format_dbg!())?,
        ))
    }

    fn solve(
        &mut self,
        pwr_out_req: si::Power,
        _enabled: bool,
        _dt: si::Time,
    ) -> anyhow::Result<Option<si::Power>> {
        let state = &mut self.state;
        // positive traction
        ensure!(
            pwr_out_req <= *state.pwr_out_fwd_max.get_fresh(|| format_dbg!())?,
            "{}\n`pwr_out_req` ({} kW) exceeds `state.pwr_out_fwd_max` ({})",
            format_dbg!(),
            pwr_out_req.get::<si::kilowatt>().format_eng(None),
            state
                .pwr_out_fwd_max
                .get_fresh(|| format_dbg!())?
                .get::<si::kilowatt>()
                .format_eng(None)
        );
        // no need for negative traction because that still includes component from friction brakes

        let eff_pt: &[f64] = match self.eff_interp {
            InterpolatorEnum::Interp0D(_) => &[],
            _ => unimplemented!("Only Interp0D is currently implemented"),
        };
        state.eff.update(
            self.eff_interp.interpolate(eff_pt)? * uc::R,
            || format_dbg!(),
        )?;
        ensure!(
            *state.eff.get_fresh(|| format_dbg!())? >= 0.0 * uc::R
                && *state.eff.get_fresh(|| format_dbg!())? <= 1.0 * uc::R,
            format!(
                "{}\nTransmission efficiency ({}) must be between 0 and 1",
                format_dbg!(
                    *state.eff.get_fresh(|| format_dbg!())? >= 0.0 * uc::R
                        && *state.eff.get_fresh(|| format_dbg!())? <= 1.0 * uc::R
                ),
                state.eff.get_fresh(|| format_dbg!())?.get::<si::ratio>()
            )
        );

        state.pwr_out.update(pwr_out_req, || format_dbg!())?;
        state.pwr_in.update(
            if *state.pwr_out.get_fresh(|| format_dbg!())? > si::Power::ZERO {
                *state.pwr_out.get_fresh(|| format_dbg!())?
                    / *state.eff.get_fresh(|| format_dbg!())?
            } else {
                *state.pwr_out.get_fresh(|| format_dbg!())?
                    * *state.eff.get_fresh(|| format_dbg!())?
            },
            || format_dbg!(),
        )?;
        state.pwr_loss.update(
            (*state.pwr_in.get_fresh(|| format_dbg!())?
                - *state.pwr_out.get_fresh(|| format_dbg!())?)
            .abs(),
            || format_dbg!(),
        )?;

        Ok(Some(*state.pwr_in.get_fresh(|| format_dbg!())?))
    }

    fn pwr_regen(&self) -> anyhow::Result<si::Power> {
        Ok(-self
            .state
            .pwr_out
            .get_fresh(|| format_dbg!())?
            .max(si::Power::ZERO))
    }
}

impl HistoryMethods for Transmission {
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
impl SerdeAPI for Transmission {}
impl Init for Transmission {}

impl Mass for Transmission {
    fn mass(&self) -> anyhow::Result<Option<si::Mass>> {
        Ok(self.mass)
    }

    fn set_mass(
        &mut self,
        new_mass: Option<si::Mass>,
        _side_effect: MassSideEffect,
    ) -> anyhow::Result<()> {
        match new_mass {
            Some(_) => {
                ensure!(
                    new_mass > Some(0.0 * uc::KG),
                    "{} mass must be positive",
                    stringify!(Transmission)
                );
                self.mass = new_mass;
            }
            None => {
                self.mass = None;
            }
        }
        Ok(())
    }

    fn derived_mass(&self) -> anyhow::Result<Option<si::Mass>> {
        Ok(self.mass)
    }

    fn expunge_mass_fields(&mut self) {
        self.mass = None;
    }
}

#[serde_api]
#[derive(
    Clone,
    Default,
    Debug,
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
pub struct TransmissionState {
    /// time step index
    pub i: TrackedState<usize>,

    /// max power output in the forward direction
    pub pwr_out_fwd_max: TrackedState<si::Power>,

    /// max power output in the backward/regen direction
    pub pwr_out_regen_max: TrackedState<si::Power>,

    /// efficiency at current time step
    #[si_unit(percent)]
    pub eff: TrackedState<si::Ratio>,

    /// Power at output side of transmission.  Positive indicates forward power
    /// (e.g. acceleration, ascent, working against dissipative forces)
    pub pwr_out: TrackedState<si::Power>,
    pub energy_out: TrackedState<si::Energy>,
    /// Power at input side of transmission.  Positive indicates forward power
    /// (e.g. acceleration, ascent, working against dissipative forces)
    pub pwr_in: TrackedState<si::Power>,
    pub energy_in: TrackedState<si::Energy>,

    /// Power loss: [Self::pwr_in] - [Self::pwr_out]
    pub pwr_loss: TrackedState<si::Power>,
    pub energy_loss: TrackedState<si::Energy>,
}

impl Init for TransmissionState {}
impl SerdeAPI for TransmissionState {}
