use super::*;

#[derive(Deserialize, Serialize, Debug, Clone, PartialEq)]
pub enum FuelType {
    Gasoline,
    Diesel,
    Hydrogen,
}

#[serde_api]
#[derive(Deserialize, Serialize, Debug, Clone, PartialEq, SetCumulative)]
#[cfg_attr(feature = "pyo3", pyclass(module = "fastsim", subclass, eq))]
#[non_exhaustive]
#[serde(deny_unknown_fields)]
pub struct FuelStorage {
    /// max power output
    pub pwr_out_max: si::Power,
    /// time to peak power
    pub pwr_ramp_lag: si::Time,
    /// fuel type
    #[serde(default)]
    pub fuel_type: Option<FuelType>,
    /// energy capacity
    #[si_unit(kilowatt_hours)]
    pub energy_capacity: si::Energy,
    /// Fuel and tank specific energy
    pub(crate) specific_energy: Option<si::SpecificEnergy>,
    /// Mass of fuel storage
    #[serde(default)]
    pub(crate) mass: Option<si::Mass>,
    // TODO: add state to track fuel level and make sure mass changes propagate up to vehicle level,
    // which should then include vehicle mass in state
}

#[pyo3_api]
impl FuelStorage {
    // TODO: decide on way to deal with `side_effect` coming after optional arg and uncomment
    // #[setter("__mass_kg")]
    // fn set_mass_py(&mut self, mass_kg: Option<f64>) -> anyhow::Result<()> {
    //     self.set_mass(mass_kg.map(|m| m * uc::KG))?;
    //     Ok(())
    // }

    // #[getter("mass_kg")]
    // fn get_mass_py(&self) -> PyResult<Option<f64>> {
    //     Ok(self.mass()?.map(|m| m.get::<si::kilogram>()))
    // }
}

impl FuelStorage {
    pub fn new(
        pwr_out_max: si::Power,
        pwr_ramp_lag: si::Time,
        fuel_type: Option<FuelType>,
        energy_capacity: si::Energy,
        specific_energy: Option<si::SpecificEnergy>,
        mass: Option<si::Mass>,
    ) -> anyhow::Result<Self> {
        let mut fs = Self {
            pwr_out_max,
            pwr_ramp_lag,
            fuel_type,
            energy_capacity,
            specific_energy,
            mass,
        };
        fs.init()?;
        Ok(fs)
    }
}

impl SerdeAPI for FuelStorage {}
impl Init for FuelStorage {}

impl Mass for FuelStorage {
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
                            self.energy_capacity = self.specific_energy.with_context(|| {
                                format!(
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
                stringify!(FuelStorage)
            ),
        };
        ensure!(
            self.mass > Some(0.0 * uc::KG),
            "{} mass must be positive",
            stringify!(FuelStorage)
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
