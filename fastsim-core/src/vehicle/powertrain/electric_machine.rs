//! Module for electric machine (i.e. bidirectional electromechanical device), generator, or motor

use super::*;

#[allow(unused_imports)]
#[cfg(feature = "pyo3")]
use crate::pyo3::*;

#[derive(Clone, Debug, Serialize, PartialEq, IsVariant, TryInto)]
/// Determines what [ElectricMachine] state variables to use in calculating efficiency
pub enum EMEfficiency {
    /// Efficiency is constant
    Constant(#[serde(serialize_with = "serialize_nested")] Interp0D<f64>),
    /// Efficiency = f(output power / max output power)
    PwrOutFrac(#[serde(serialize_with = "serialize_nested")] Interp1D<f64, strategy::Linear>),
}

impl<'de> Deserialize<'de> for EMEfficiency {
    /// Accepts the current, externally-tagged shape (`{Constant: ...}` /
    /// `{PwrOutFrac: {...}}`) as well as the shape used by the old, now-removed
    /// `eff_interp_achieved` field: a bare, untagged [`InterpolatorEnum`], with the
    /// variant inferred from its own shape (see [`ElectricMachine::eff_interp`]).
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        #[derive(Deserialize)]
        enum Tagged {
            Constant(Interp0D<f64>),
            PwrOutFrac(Interp1D<f64, strategy::Linear>),
        }

        #[derive(Deserialize)]
        #[serde(untagged)]
        enum Repr {
            Tagged(Tagged),
            Legacy(InterpolatorEnum<f64>),
        }

        Ok(match Repr::deserialize(deserializer)? {
            Repr::Tagged(Tagged::Constant(interp)) => Self::Constant(interp),
            Repr::Tagged(Tagged::PwrOutFrac(interp)) => Self::PwrOutFrac(interp),
            Repr::Legacy(InterpolatorEnumBase::Interp0D(interp)) => Self::Constant(interp),
            Repr::Legacy(InterpolatorEnumBase::Interp1D(interp)) => {
                let strategy::enums::Strategy1DEnum::Linear(strategy) = interp.strategy else {
                    return Err(serde::de::Error::custom(
                        "legacy `EMEfficiency` data only supports the `Linear` strategy",
                    ));
                };
                Self::PwrOutFrac(
                    Interp1D::new(
                        interp.data.grid[0].clone(),
                        interp.data.values.clone(),
                        strategy,
                        interp.extrapolate,
                    )
                    .map_err(serde::de::Error::custom)?,
                )
            }
            Repr::Legacy(_) => {
                return Err(serde::de::Error::custom(
                    "`EMEfficiency` only supports 0-D (`Constant`) or 1-D (`PwrOutFrac`) interpolator data",
                ))
            }
        })
    }
}

impl_efficiency_enum!(EMEfficiency {
    Constant,
    PwrOutFrac,
});

#[serde_api]
#[derive(Deserialize, Serialize, Debug, Clone, PartialEq, StateMethods, SetCumulative)]
#[non_exhaustive]
#[serde(deny_unknown_fields)]
#[cfg_attr(feature = "pyo3", pyclass(module = "fastsim", subclass, eq))]
/// Struct for modeling electric machines.  This lumps performance and efficiency of motor and power
/// electronics.
pub struct ElectricMachine {
    /// Efficiency map
    #[serde(alias = "eff_interp_achieved")]
    pub eff_interp: EMEfficiency,
    /// Legacy field from before `eff_interp_achieved`/`eff_interp_at_max_input` were
    /// merged into `eff_interp`. `eff_interp_at_max_input` was always fully derived
    /// from `eff_interp_achieved` and is now recomputed on the fly in
    /// [`Self::set_curr_pwr_prop_out_max`], so it is safe to discard; this field
    /// exists only so old files containing it still deserialize under
    /// `deny_unknown_fields`.
    #[serde(rename = "eff_interp_at_max_input", default, skip_serializing)]
    #[allow(dead_code)]
    pub(crate) _eff_interp_at_max_input_legacy: Option<serde::de::IgnoredAny>,
    /// Electrical input power fraction array at which efficiencies are evaluated.
    /// Calculated during runtime if not provided.
    // /// this will disappear and instead be in eff_interp_bwd
    // pub pwr_in_frac_interp: Vec<f64>,
    /// ElectricMachine maximum output power \[W\]
    pub pwr_out_max: si::Power,
    /// ElectricMachine specific power
    pub specific_pwr: Option<si::SpecificPower>,
    /// ElectricMachine mass
    pub(crate) mass: Option<si::Mass>,
    /// Time step interval between saves. 1 is a good option. If None, no saving occurs.
    pub save_interval: Option<usize>,
    /// struct for tracking current state
    #[serde(default)]
    pub state: ElectricMachineState,
    /// Custom vector of [Self::state]
    #[serde(
        default,
        skip_serializing_if = "ElectricMachineStateHistoryVec::is_empty"
    )]
    pub history: ElectricMachineStateHistoryVec,
}

#[pyo3_api]
impl ElectricMachine {
    // #[new]
    // fn __new__(
    //     pwr_out_frac_interp: Vec<f64>,
    //     eff_interp: Vec<f64>,
    //     pwr_out_max_watts: f64,
    //     save_interval: Option<usize>,
    // ) -> anyhow::Result<Self> {
    //     Self::new(
    //         pwr_out_frac_interp,
    //         eff_interp,
    //         pwr_out_max_watts,
    //         save_interval,
    //     )
    // }

    // #[setter]
    // pub fn set_eff_interp(&mut self, new_value: Vec<f64>) -> anyhow::Result<()> {
    //     self.eff_interp = new_value;
    //     self.set_pwr_in_frac_interp()
    // }

    #[getter("eff_fwd_max")]
    fn get_eff_max_fwd_py(&self) -> PyResult<f64> {
        Ok(*self.get_eff_fwd_max()?)
    }

    #[setter("__eff_fwd_max")]
    fn set_eff_fwd_max_py(&mut self, eff_max: f64) -> PyResult<()> {
        self.set_eff_fwd_max(eff_max)?;
        Ok(())
    }

    #[getter("eff_min_fwd")]
    fn get_eff_min_fwd_py(&self) -> PyResult<f64> {
        Ok(*self.get_eff_min_fwd()?)
    }

    #[getter("eff_fwd_range")]
    fn get_eff_fwd_range_py(&self) -> PyResult<f64> {
        Ok(self.get_eff_fwd_range()?)
    }

    #[setter("__eff_fwd_range")]
    fn set_eff_fwd_range_py(&mut self, eff_range: f64) -> PyResult<()> {
        self.set_eff_fwd_range(eff_range)?;
        Ok(())
    }
}

impl ElectricMachine {
    pub fn new(
        eff_interp: EMEfficiency,
        pwr_out_max: si::Power,
        specific_pwr: Option<si::SpecificPower>,
        mass: Option<si::Mass>,
        save_interval: Option<usize>,
    ) -> anyhow::Result<Self> {
        let mut em = ElectricMachine {
            eff_interp,
            _eff_interp_at_max_input_legacy: None,
            pwr_out_max,
            specific_pwr,
            mass,
            save_interval,
            state: ElectricMachineState::default(),
            history: ElectricMachineStateHistoryVec::default(),
        };
        em.init()?;

        Ok(em)
    }
}

impl Powertrain for ElectricMachine {
    /// Returns maximum possible positive and negative propulsion-related powers
    /// this component/system can produce, accounting for any aux-related power
    /// required.
    /// # Arguments
    /// - `pwr_in_fwd_lim`: positive-propulsion-related power available to this
    ///   component. Positive values indicate that the upstream component can supply
    ///   positive tractive power.
    /// - `pwr_in_bwd_lim`: negative-propulsion-related power available to this
    ///   component. Zero means no power can be sent to upstream compnents and positive
    ///   values indicate upstream components can absorb energy.
    /// - `pwr_aux`: aux-related power required from this component
    /// - `dt`: simulation time step size
    fn set_curr_pwr_prop_out_max(
        &mut self,
        pwr_upstream: (si::Power, si::Power),
        _pwr_aux: si::Power,
        _dt: si::Time,
        _veh_state: &VehicleState,
    ) -> anyhow::Result<()> {
        let pwr_in_fwd_lim = &pwr_upstream.0;
        let pwr_in_bwd_lim = &pwr_upstream.1;
        ensure!(
            pwr_in_fwd_lim >= &si::Power::ZERO,
            "`{}` ({} W) must be greater than or equal to zero for `{}`",
            stringify!(pwr_in_fwd_lim),
            pwr_in_fwd_lim.get::<si::watt>().format_eng(None),
            stringify!(ElectricMachine::get_curr_pwr_prop_out_max)
        );
        ensure!(
            pwr_in_bwd_lim >= &si::Power::ZERO,
            "`{}` ({} W) must be greater than or equal to zero for `{}`",
            stringify!(pwr_in_bwd_lim),
            pwr_in_bwd_lim.get::<si::watt>().format_eng(None),
            stringify!(ElectricMachine::get_curr_pwr_prop_out_max)
        );

        let raw_tractive_lookup_ratio = (*pwr_in_fwd_lim / self.pwr_out_max).get::<si::ratio>();
        let raw_regen_lookup_ratio = (*pwr_in_bwd_lim / self.pwr_out_max).get::<si::ratio>();

        self.state.eff_fwd_at_max_input.update(
            uc::R
                * match &self.eff_interp {
                    EMEfficiency::Constant(interp) => interp.interpolate(&[])?,
                    EMEfficiency::PwrOutFrac(interp) => {
                        let x_in = abs_checked_x_val(
                            raw_tractive_lookup_ratio,
                            interp.data.grid[0]
                                .as_slice()
                                .ok_or_else(|| anyhow!(format_dbg!()))?,
                        )?;
                        interp
                            .ratio_inverse(Extrapolate::Clamp)
                            .interpolate(&[x_in])?
                    }
                },
            || format_dbg!(),
        )?;
        self.state.eff_at_max_regen.update(
            uc::R
                * match &self.eff_interp {
                    EMEfficiency::Constant(interp) => interp.interpolate(&[])?,
                    EMEfficiency::PwrOutFrac(interp) => {
                        let x_in = abs_checked_x_val(
                            raw_regen_lookup_ratio,
                            interp.data.grid[0]
                                .as_slice()
                                .ok_or_else(|| anyhow!(format_dbg!()))?,
                        )?;
                        interp
                            .ratio_inverse(Extrapolate::Clamp)
                            .interpolate(&[x_in])?
                    }
                },
            || format_dbg!(),
        )?;

        // maximum power in forward direction is minimum of component `pwr_out_max` parameter or time-varying max
        // power based on what the ReversibleEnergyStorage can provide
        self.state.pwr_mech_fwd_out_max.update(
            self.pwr_out_max.min(
                *pwr_in_fwd_lim
                    * *self
                        .state
                        .eff_fwd_at_max_input
                        .get_fresh(|| format_dbg!())?,
            ),
            || format_dbg!(),
        )?;
        // maximum power in backward direction is minimum of component `pwr_out_max` parameter or time-varying max
        // power in bacward direction (i.e. regen) based on what the ReversibleEnergyStorage can provide
        self.state.pwr_mech_regen_max.update(
            self.pwr_out_max
                .min(*pwr_in_bwd_lim / *self.state.eff_at_max_regen.get_fresh(|| format_dbg!())?),
            || format_dbg!(),
        )?;
        Ok(())
    }

    fn get_curr_pwr_prop_out_max(&self) -> anyhow::Result<(si::Power, si::Power)> {
        Ok((
            *self
                .state
                .pwr_mech_fwd_out_max
                .get_fresh(|| format_dbg!())?,
            *self.state.pwr_mech_regen_max.get_fresh(|| format_dbg!())?,
        ))
    }

    /// Solves for this powertrain system/component efficiency and sets/returns power input required.
    /// # Arguments
    /// - `pwr_out_req`: propulsion-related power output required
    /// - `dt`: simulation time step size
    fn solve(
        &mut self,
        pwr_out_req: si::Power,
        _enabled: bool,
        _dt: si::Time,
    ) -> anyhow::Result<Option<si::Power>> {
        if pwr_out_req > si::Power::ZERO {
            ensure!(
                almost_le_uom(&pwr_out_req, &self.pwr_out_max, None),
                format!(
                    "{}\nedrv required power ({} kW) exceeds static max power ({} kW)",
                    format_dbg!(),
                    pwr_out_req.get::<si::kilowatt>().format_eng(Some(9)),
                    self.pwr_out_max.get::<si::kilowatt>().format_eng(Some(9))
                ),
            );
        }
        // not needed during negative traction because friction braking is still included
        ensure!(
            almost_le_uom(&pwr_out_req , self.state.pwr_mech_fwd_out_max.get_fresh(|| format_dbg!())?, None),
            format!(
                "{}\nedrv required propulsion power ({} kW) exceeds current max propulsion power ({} kW) by {} kW",
                format_dbg!(pwr_out_req <= *self.state.pwr_mech_fwd_out_max.get_fresh(|| format_dbg!())?),
                pwr_out_req.get::<si::kilowatt>().format_eng(Some(6)),
                self.state
                    .pwr_mech_fwd_out_max
                    .get_fresh(|| format_dbg!())?
                    .get::<si::kilowatt>()
                    .format_eng(Some(6)),
                    (pwr_out_req - *self.state.pwr_mech_fwd_out_max.get_fresh(|| format_dbg!())?).get::<si::kilowatt>().format_eng(Some(6))
            ),
        );
        if pwr_out_req < si::Power::ZERO {
            ensure!(
                almost_le_uom(
                    &pwr_out_req.abs(),
                    self.state.pwr_mech_regen_max.get_fresh(|| format_dbg!())?,
                    None
                ),
                format!(
                    "{}\nedrv charge power ({:.6} kW) exceeds current max charge power ({:.6} kW)",
                    format_dbg!(),
                    -pwr_out_req.get::<si::kilowatt>(),
                    self.state
                        .pwr_mech_regen_max
                        .get_fresh(|| format_dbg!())?
                        .get::<si::kilowatt>()
                ),
            );
        }

        // if pwr_out_req is almost less than or equal to pwr_out_max, but technically ever so slightly bigger
        // set to pwr_out_max to avoid extrapolation errors
        if (pwr_out_req > self.pwr_out_max) && almost_le_uom(&pwr_out_req, &self.pwr_out_max, None)
        {
            self.state
                .pwr_out_req
                .update(self.pwr_out_max, || format_dbg!())?;
        } else {
            self.state
                .pwr_out_req
                .update(pwr_out_req, || format_dbg!())?;
        }

        // updated pwr_out_req since it may have been changed slightly above
        let pwr_out_req = *self.state.pwr_out_req.get_fresh(|| format_dbg!())?;

        // `pwr_mech_prop_out` is `pwr_out_req` unless `pwr_out_req` is more negative than `pwr_mech_regen_max`,
        // in which case, excess is handled by `pwr_mech_dyn_brake`
        self.state.pwr_mech_prop_out.update(
            pwr_out_req.max(-*self.state.pwr_mech_regen_max.get_fresh(|| format_dbg!())?),
            || format_dbg!(),
        )?;

        let is_max_output = pwr_out_req
            == *self
                .state
                .pwr_mech_fwd_out_max
                .get_fresh(|| format_dbg!())?;

        // ensuring eff_interp has Extrapolate set to Error before calculating self.state.eff
        self.eff_interp.set_extrapolate(Extrapolate::Error)?;

        let raw_lookup_pwr_ratio = (pwr_out_req / self.pwr_out_max).get::<si::ratio>();
        let calculated_eff = uc::R
            * match &self.eff_interp {
                EMEfficiency::Constant(interp) => interp.interpolate(&[])?,
                EMEfficiency::PwrOutFrac(interp) => {
                    // not needed during negative traction because friction braking is
                    // still included, so clamp to 0 rather than querying at negative x
                    let x_out = if interp.data.grid[0]
                        .first()
                        .with_context(|| anyhow!(format_dbg!()))?
                        >= &0.
                    {
                        raw_lookup_pwr_ratio.max(0.)
                    } else {
                        raw_lookup_pwr_ratio
                    };
                    interp.interpolate(&[x_out]).map_err(|e| {
                        anyhow!(
                            "failed to calculate efficiency at line {} with originating error [{}]",
                            format_dbg!(),
                            e
                        )
                    })?
                }
            };
        let eff_value = if is_max_output {
            if pwr_out_req >= si::Power::ZERO {
                *self
                    .state
                    .eff_fwd_at_max_input
                    .get_fresh(|| format_dbg!())?
            } else {
                *self.state.eff_at_max_regen.get_fresh(|| format_dbg!())?
            }
        } else {
            calculated_eff
        };
        ensure!(eff_value >= si::Ratio::ZERO && eff_value <= 1.0 * uc::R);
        self.state.eff.update(eff_value, || format_dbg!())?;

        self.state.pwr_mech_dyn_brake.update(
            -(pwr_out_req - *self.state.pwr_mech_prop_out.get_fresh(|| format_dbg!())?),
            || format_dbg!(),
        )?;
        ensure!(
            *self.state.pwr_mech_dyn_brake.get_fresh(|| format_dbg!())? >= si::Power::ZERO,
            "Mech Dynamic Brake Power cannot be below 0.0"
        );

        // if pwr_out_req is negative, need to multiply by eff
        self.state.pwr_elec_prop_in.update(
            if pwr_out_req > si::Power::ZERO {
                *self.state.pwr_mech_prop_out.get_fresh(|| format_dbg!())?
                    / *self.state.eff.get_fresh(|| format_dbg!())?
            } else {
                *self.state.pwr_mech_prop_out.get_fresh(|| format_dbg!())?
                    * *self.state.eff.get_fresh(|| format_dbg!())?
            },
            || format_dbg!(),
        )?;

        self.state.pwr_elec_dyn_brake.update(
            *self.state.pwr_mech_dyn_brake.get_fresh(|| format_dbg!())?
                * *self.state.eff.get_fresh(|| format_dbg!())?,
            || format_dbg!(),
        )?;

        // loss does not account for dynamic braking
        self.state.pwr_loss.update(
            (*self.state.pwr_mech_prop_out.get_fresh(|| format_dbg!())?
                - *self.state.pwr_elec_prop_in.get_fresh(|| format_dbg!())?)
            .abs(),
            || format_dbg!(),
        )?;

        Ok(Some(
            *self.state.pwr_elec_prop_in.get_fresh(|| format_dbg!())?,
        ))
    }

    fn pwr_regen(&self) -> anyhow::Result<si::Power> {
        Ok(-self
            .state
            .pwr_mech_dyn_brake
            .get_fresh(|| format_dbg!())?
            .max(si::Power::ZERO))
    }
}

impl SerdeAPI for ElectricMachine {}
impl Init for ElectricMachine {
    fn init(&mut self) -> Result<(), Error> {
        let _ = self
            .mass()
            .map_err(|err| Error::InitError(format_dbg!(err)))?;
        if let EMEfficiency::PwrOutFrac(interp) = &self.eff_interp {
            let _ = check_interp_frac_data(
                interp
                    .data
                    .grid[0]
                    .as_slice()
                    .ok_or(Error::Other("Cannot convert to slice".to_string()))?,
                InterpRange::Either,
            )
            .map_err(|err| {
                Error::InitError(format!(
                    "{}\nInvalid values for `ElectricMachine::pwr_out_frac_interp`; must range from [-1..1] or [0..1].",
                    format_dbg!(err)
                ))
            })?;
        }
        self.state
            .init()
            .map_err(|err| Error::InitError(format_dbg!(err)))?;
        Ok(())
    }
}
impl HistoryMethods for ElectricMachine {
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

impl Mass for ElectricMachine {
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
            (None, None) => {
                bail!(
                    "Not all mass fields in `{}` are set and no mass was provided.",
                    stringify!(ElectricMachine)
                )
            }
        };
        ensure!(
            self.mass > Some(0.0 * uc::KG),
            "{} mass must be positive",
            stringify!(ElectricMachine)
        );
        Ok(())
    }

    fn derived_mass(&self) -> anyhow::Result<Option<si::Mass>> {
        Ok(self
            .specific_pwr
            .map(|specific_pwr| self.pwr_out_max / specific_pwr))
    }

    fn expunge_mass_fields(&mut self) {
        self.specific_pwr = None;
        self.mass = None;
    }
}

impl TryFrom<EMBuilder> for ElectricMachine {
    type Error = anyhow::Error;
    fn try_from(em_builder: EMBuilder) -> anyhow::Result<ElectricMachine> {
        let mut em = ElectricMachine {
            eff_interp: em_builder.eff_interp.clone(),
            _eff_interp_at_max_input_legacy: None,
            pwr_out_max: em_builder.pwr_out_max,
            specific_pwr: None,
            mass: None,
            save_interval: Some(1),
            state: Default::default(),
            history: Default::default(),
        };
        em.init()?;

        Ok(em)
    }
}

impl ElectricMachine {
    /// Returns max value of `eff_interp`
    pub fn get_eff_fwd_max(&self) -> anyhow::Result<&f64> {
        // since efficiency is all f64 between 0 and 1, NEG_INFINITY is safe
        self.eff_interp.max()
    }

    /// Scales eff_interp by ratio of new `eff_max` per current calculated max
    pub fn set_eff_fwd_max(&mut self, eff_max: f64) -> anyhow::Result<()> {
        if (0.0..=1.0).contains(&eff_max) {
            let old_max = *self.get_eff_fwd_max()?;
            match &mut self.eff_interp {
                EMEfficiency::Constant(interp) => interp.0 = eff_max,
                EMEfficiency::PwrOutFrac(interp) => {
                    interp.data.values = interp
                        .data
                        .values
                        .iter()
                        .map(|x| x * eff_max / old_max)
                        .collect::<Array1<_>>();
                }
            }
            Ok(())
        } else {
            Err(anyhow!(
                "`eff_max` ({:.3}) must be between 0.0 and 1.0",
                eff_max,
            ))
        }
    }

    /// Returns min value of `eff_interp`
    pub fn get_eff_min_fwd(&self) -> anyhow::Result<&f64> {
        self.eff_interp.min()
    }

    /// Max value of `eff_interp` minus min value of `eff_interp`.
    pub fn get_eff_fwd_range(&self) -> anyhow::Result<f64> {
        Ok(self.get_eff_fwd_max()? - self.get_eff_min_fwd()?)
    }

    /// Scales values of `eff_interp` without changing max such that max - min
    /// is equal to new range.  Will change max if needed to ensure no values are
    /// less than zero.
    pub fn set_eff_fwd_range(&mut self, eff_range: f64) -> anyhow::Result<()> {
        let eff_max = self.get_eff_fwd_max()?.to_owned();
        if eff_range == 0.0 {
            if let EMEfficiency::PwrOutFrac(interp) = &mut self.eff_interp {
                let f_x = vec![eff_max; interp.data.values.len()];
                interp.data.values = Array::from_vec(f_x);
            }
            return Ok(());
        }
        if !(0.0..=1.0).contains(&eff_range) {
            return Err(anyhow!(
                "`eff_range` ({:.3}) must be between 0.0 and 1.0",
                eff_range,
            ));
        }
        let old_min = *self.get_eff_min_fwd()?;
        let old_range = eff_max - old_min;
        if old_range == 0.0 {
            return Err(anyhow!(
                "`eff_range` is already zero so it cannot be modified."
            ));
        }
        match &mut self.eff_interp {
            EMEfficiency::Constant(_) => bail!(
                "`eff_range` cannot be set to a nonzero value for a `Constant` efficiency map."
            ),
            EMEfficiency::PwrOutFrac(interp) => {
                interp.data.values = interp
                    .data
                    .values
                    .iter()
                    .map(|x| eff_max + (x - eff_max) * eff_range / old_range)
                    .collect();
                interp.validate()?;
            }
        }
        if *self.get_eff_min_fwd()? < 0. {
            let x_neg = *self.get_eff_min_fwd()?;
            if let EMEfficiency::PwrOutFrac(interp) = &mut self.eff_interp {
                interp.data.values.map_inplace(|x| *x -= x_neg);
                interp.validate()?;
            }
        }
        if *self.get_eff_fwd_max()? > 1.0 {
            return Err(anyhow!(format!(
                "`eff_max` ({:.3}) must be no greater than 1.0",
                self.get_eff_fwd_max()?
            )));
        }
        Ok(())
    }
}

#[serde_api]
#[derive(Deserialize, Serialize, Debug, Clone, PartialEq)]
#[serde(deny_unknown_fields)]
#[cfg_attr(feature = "pyo3", pyclass(module = "fastsim", subclass, eq))]
/// Builder for [ElectricMachine].  Use this to instantiate EM with minimal parameterization
pub struct EMBuilder {
    /// Efficiency map
    pub eff_interp: EMEfficiency,
    /// Electrical input power fraction array at which efficiencies are evaluated.
    /// Calculated during runtime if not provided.
    // /// this will disappear and instead be in eff_interp_bwd
    // pub pwr_in_frac_interp: Vec<f64>,
    /// ElectricMachine maximum output power \[W\]
    pub pwr_out_max: si::Power,
}

#[allow(dead_code)]
impl EMBuilder {
    fn with_save_interval(&self, save_interval: Option<usize>) -> anyhow::Result<ElectricMachine> {
        let mut em: ElectricMachine = self.clone().try_into()?;
        em.save_interval = save_interval;
        Ok(em)
    }

    fn with_state(&self, state: ElectricMachineState) -> anyhow::Result<ElectricMachine> {
        let mut em: ElectricMachine = self.clone().try_into()?;
        em.state = state;
        Ok(em)
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
#[serde(default)]
#[serde(deny_unknown_fields)]
#[cfg_attr(feature = "pyo3", pyclass(module = "fastsim", subclass, eq))]

pub struct ElectricMachineState {
    /// time step index
    pub i: TrackedState<usize>,
    /// Component efficiency based on current power demand.
    pub eff: TrackedState<si::Ratio>,
    // Component limits
    /// Maximum possible positive traction power.
    pub pwr_mech_fwd_out_max: TrackedState<si::Power>,
    /// efficiency in forward direction at max possible input power from `FuelConverter` and `ReversibleEnergyStorage`
    pub eff_fwd_at_max_input: TrackedState<si::Ratio>,
    /// Maximum possible regeneration power going to ReversibleEnergyStorage.
    pub pwr_mech_regen_max: TrackedState<si::Power>,
    /// efficiency in backward direction at max possible input power from `FuelConverter` and `ReversibleEnergyStorage`
    pub eff_at_max_regen: TrackedState<si::Ratio>,

    // Current values
    /// Raw power requirement from boundary conditions
    pub pwr_out_req: TrackedState<si::Power>,
    /// Integral of [Self::pwr_out_req]
    pub energy_out_req: TrackedState<si::Energy>,
    /// Electrical power to propulsion from ReversibleEnergyStorage and Generator.
    /// negative value indicates regenerative braking
    pub pwr_elec_prop_in: TrackedState<si::Power>,
    /// Integral of [Self::pwr_elec_prop_in]
    pub energy_elec_prop_in: TrackedState<si::Energy>,
    /// Mechanical power to propulsion, corrected by efficiency, from ReversibleEnergyStorage and Generator.
    /// Negative value indicates regenerative braking.
    pub pwr_mech_prop_out: TrackedState<si::Power>,
    /// Integral of [Self::pwr_mech_prop_out]
    pub energy_mech_prop_out: TrackedState<si::Energy>,
    /// Mechanical power from dynamic braking.  Positive value indicates braking; this should be zero otherwise.
    pub pwr_mech_dyn_brake: TrackedState<si::Power>,
    /// Integral of [Self::pwr_mech_dyn_brake]
    pub energy_mech_dyn_brake: TrackedState<si::Energy>,
    /// Electrical power from dynamic braking, dissipated as heat.
    pub pwr_elec_dyn_brake: TrackedState<si::Power>,
    /// Integral of [Self::pwr_elec_dyn_brake]
    pub energy_elec_dyn_brake: TrackedState<si::Energy>,
    /// Power lost in regeneratively converting mechanical power to power that can be absorbed by the battery.
    pub pwr_loss: TrackedState<si::Power>,
    /// Integral of [Self::pwr_loss]
    pub energy_loss: TrackedState<si::Energy>,
}

#[pyo3_api]
impl ElectricMachineState {}

impl Init for ElectricMachineState {}
impl SerdeAPI for ElectricMachineState {}
