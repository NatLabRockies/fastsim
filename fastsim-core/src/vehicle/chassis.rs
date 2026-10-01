pub use super::*;

/// Possible drive wheel configurations for traction limit calculations
#[derive(
    Clone, Debug, Serialize, Deserialize, PartialEq, IsVariant, derive_more::From, TryInto,
)]
pub enum DriveTypes {
    /// Rear-wheel drive
    RWD,
    /// Front-wheel drive
    FWD,
    /// All-wheel drive
    AWD,
    /// 4-wheel drive
    #[serde(alias = "FourWD")]
    #[serde(rename = "4WD")]
    FourWD,
}

impl SerdeAPI for DriveTypes {}
impl Init for DriveTypes {}

#[serde_api]
#[non_exhaustive]
#[derive(PartialEq, Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
/// Struct for simulating vehicle
pub struct Chassis {
    /// Aerodynamic drag coefficient
    #[si_unit(unitless)]
    pub drag_coef: si::Ratio,
    /// Projected frontal area for drag calculations
    pub frontal_area: si::Area,
    /// Wheel rolling resistance coefficient for the vehicle (i.e. all wheels included)
    #[si_unit(unitless)]
    pub wheel_rr_coef: si::Ratio,
    /// Wheel inertia per wheel
    pub wheel_inertia: si::MomentOfInertia,
    /// Number of wheels
    pub num_wheels: u8,
    /// Wheel radius
    #[serde(default)]
    pub wheel_radius: Option<si::Length>,
    /// Tire code (optional method of calculating wheel radius)
    #[serde(default)]
    pub tire_code: Option<String>,
    /// Vehicle center of mass height
    pub cg_height: si::Length,
    /// Wheel coefficient of friction
    #[si_unit(unitless)]
    pub wheel_fric_coef: si::Ratio,

    /// Drive wheel configuration
    pub drive_type: DriveTypes,
    /// Fraction of vehicle weight on drive action when stationary
    pub drive_axle_weight_frac: si::Ratio,
    /// Wheel base length
    pub wheel_base: si::Length,

    pub(crate) mass: Option<si::Mass>,
    /// Vehicle mass excluding cargo, passengers, and powertrain components
    pub(crate) glider_mass: Option<si::Mass>,
    /// Cargo mass including passengers
    #[serde(default)]
    pub cargo_mass: Option<si::Mass>,
}

impl SerdeAPI for Chassis {}
impl Init for Chassis {}

impl Mass for Chassis {
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
        _side_effect: MassSideEffect,
    ) -> anyhow::Result<()> {
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
                stringify!(Chassis)
            ),
        };
        ensure!(
            self.mass > Some(0.0 * uc::KG),
            "{} mass must be positive",
            stringify!(Chassis)
        );
        Ok(())
    }

    fn derived_mass(&self) -> anyhow::Result<Option<si::Mass>> {
        let mass =
            if let (Some(glider_mass), Some(cargo_mass)) = (self.glider_mass, self.cargo_mass) {
                Some(glider_mass + cargo_mass)
            } else if let (None, None) = (self.glider_mass, self.cargo_mass) {
                None
            } else {
                bail!(
                    "`{}` field masses are not consistently set to `Some` or `None`",
                    stringify!(Chassis)
                )
            };
        Ok(mass)
    }

    fn expunge_mass_fields(&mut self) {
        self.mass = None;
        self.glider_mass = None;
        self.cargo_mass = None;
    }
}
