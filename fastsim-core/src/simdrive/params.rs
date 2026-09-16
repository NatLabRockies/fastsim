use super::*;

#[serde_api]
#[derive(Clone, Debug, Deserialize, Serialize, PartialEq)]
#[non_exhaustive]
#[serde(deny_unknown_fields)]
#[cfg_attr(feature = "pyo3", pyclass(module = "fastsim", subclass, eq))]
/// Solver parameters
pub struct SimParams {
    #[serde(default = "SimParams::def_ach_speed_max_iter")]
    /// max number of iterations allowed in setting achieved speed when trace
    /// cannot be achieved
    pub ach_speed_max_iter: u32,
    #[serde(default = "SimParams::def_ach_speed_tol")]
    /// tolerance in change in speed guess in setting achieved speed when trace
    /// cannot be achieved
    pub ach_speed_tol: si::Ratio,
    #[serde(default = "SimParams::def_ach_speed_solver_gain")]
    /// Newton method gain for setting achieved speed
    pub ach_speed_solver_gain: f64,
    #[serde(default = "SimParams::def_ach_speed_max_depth")]
    /// Maximum number of nested solves for achieved speed within a single time
    /// step when the trace cannot be achieved.
    pub ach_speed_max_depth: u32,
    // TODO: plumb this up to actually do something
    /// When implemented, this will set the tolerance on how much trace miss
    /// is allowed
    #[serde(default = "SimParams::def_trace_miss_tol")]
    pub trace_miss_tol: TraceMissTolerance,
    #[serde(default = "SimParams::def_trace_miss_opts")]
    pub trace_miss_opts: TraceMissOptions,
    #[serde(default = "SimParams::def_trace_miss_correct_max_steps")]
    /// the maximum number of steps in which to re-rendezvous with reference
    /// trace after a trace miss. Note: this field only applies when
    /// trace_miss_opts is set to TraceMissOptions::Correct. Note: must
    /// be 2 or greater. Defaults to 6.
    pub trace_miss_correct_max_steps: u32,
    /// whether to use FASTSim-2 style air density
    #[serde(default = "SimParams::def_f2_const_air_density")]
    pub f2_const_air_density: bool,
    /// if true, vehicle is totally inactive except for thermal models
    pub ambient_thermal_soak: bool,
}

#[pyo3_api]
impl SimParams {
    #[staticmethod]
    #[pyo3(name = "default")]
    fn default_py() -> Self {
        Self::default()
    }
}

impl SimParams {
    fn def_ach_speed_max_iter() -> u32 {
        Self::default().ach_speed_max_iter
    }
    fn def_ach_speed_tol() -> si::Ratio {
        Self::default().ach_speed_tol
    }
    fn def_ach_speed_solver_gain() -> f64 {
        Self::default().ach_speed_solver_gain
    }
    fn def_ach_speed_max_depth() -> u32 {
        Self::default().ach_speed_max_depth
    }
    fn def_trace_miss_tol() -> TraceMissTolerance {
        Self::default().trace_miss_tol
    }
    fn def_trace_miss_opts() -> TraceMissOptions {
        Self::default().trace_miss_opts
    }
    fn def_trace_miss_correct_max_steps() -> u32 {
        Self::default().trace_miss_correct_max_steps
    }
    fn def_f2_const_air_density() -> bool {
        Self::default().f2_const_air_density
    }
}

impl SerdeAPI for SimParams {}
impl Init for SimParams {}

impl Default for SimParams {
    fn default() -> Self {
        Self {
            ach_speed_max_iter: 3,
            ach_speed_tol: 1.0e-3 * uc::R,
            ach_speed_solver_gain: 0.9,
            ach_speed_max_depth: 100,
            trace_miss_tol: Default::default(),
            trace_miss_opts: Default::default(),
            trace_miss_correct_max_steps: 6,
            f2_const_air_density: true,
            ambient_thermal_soak: false,
        }
    }
}

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq)]
#[serde(deny_unknown_fields)]
#[non_exhaustive]
pub struct TraceMissTolerance {
    /// if the vehicle falls this far behind trace in terms of absolute
    /// difference and [TraceMissOptions::is_allow_checked], fail
    pub tol_dist: si::Length,
    /// if the vehicle falls this far behind trace in terms of fractional
    /// difference and [TraceMissOptions::is_allow_checked], fail
    pub tol_dist_frac: si::Ratio,
    /// if the vehicle falls this far behind instantaneous speed and
    /// [TraceMissOptions::is_allow_checked], fail
    pub tol_speed: si::Velocity,
    /// if the vehicle falls this far behind instantaneous speed in terms of
    /// fractional difference and [TraceMissOptions::is_allow_checked], fail
    pub tol_speed_frac: si::Ratio,
}

impl TraceMissTolerance {
    pub fn check_trace_miss(
        &self,
        cyc_speed: si::Velocity,
        ach_speed: si::Velocity,
        cyc_dist: si::Length,
        ach_dist: si::Length,
    ) -> anyhow::Result<()> {
        ensure!(
            (cyc_speed - ach_speed).abs() < self.tol_speed,
            concat!(
                "trace miss: achieved speed misses prescribed speed\n",
                "    achieved speed: {:?}\n",
                "    prescribed speed: {:?}\n",
                "    exceeds allowed tolerance: {:?}",
            ),
            ach_speed,
            cyc_speed,
            self.tol_speed,
        );
        // if condition to prevent divide-by-zero errors
        if cyc_speed > self.tol_speed {
            ensure!(
                (cyc_speed - ach_speed).abs() / cyc_speed < self.tol_speed_frac,
                concat!(
                    "trace miss: achieved speed misses prescribed speed (fractional)\n",
                    "    achieved speed: {:?}\n",
                    "    prescribed speed: {:?}\n",
                    "    exceeds allowed fractional tolerance: {:?}",
                ),
                ach_speed,
                cyc_speed,
                self.tol_speed_frac
            )
        }
        ensure!(
            (cyc_dist - ach_dist).abs() < self.tol_dist,
            concat!(
                "trace miss: achieved distance misses prescribed distance\n",
                "    achieved distance: {:?}\n",
                "    prescribed distance: {:?}\n",
                "    exceeds allowed tolerance: {:?}",
            ),
            ach_dist,
            cyc_dist,
            self.tol_dist
        );
        // if condition to prevent checking early in cycle
        if cyc_dist > self.tol_dist * 5.0 {
            ensure!(
                (cyc_dist - ach_dist).abs() / cyc_dist < self.tol_dist_frac,
                concat!(
                    "trace miss: achieved distance misses prescribed distance (fractional)\n",
                    "    achieved distance: {:?}\n",
                    "    prescribed distance: {:?}\n",
                    "    exceeds allowed fractional tolerance: {:?}",
                ),
                ach_dist,
                cyc_dist,
                self.tol_dist_frac
            )
        }

        Ok(())
    }
}
impl SerdeAPI for TraceMissTolerance {}
impl Init for TraceMissTolerance {}
impl Default for TraceMissTolerance {
    fn default() -> Self {
        Self {
            tol_dist: 100. * uc::M,
            tol_dist_frac: 0.05 * uc::R,
            tol_speed: 10. * uc::MPS,
            tol_speed_frac: 0.5 * uc::R,
        }
    }
}

#[derive(
    Clone, Default, Debug, Deserialize, Serialize, PartialEq, IsVariant, derive_more::From, TryInto,
)]
pub enum TraceMissOptions {
    /// Allow trace miss without any fanfare
    Allow,
    /// Allow trace miss within error tolerance
    AllowChecked,
    // /// Show warning when any trace miss happens
    // #[default]
    // Warn,
    // /// Show warning when trace miss outside tolerance happens
    #[default]
    // WarnChecked,
    /// Throw error when trace miss happens
    Error,
    /// Correct trace miss with driver model that catches up
    Correct,
}

impl SerdeAPI for TraceMissOptions {}
impl Init for TraceMissOptions {}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    #[cfg(feature = "resources")]
    #[cfg(feature = "yaml")]
    fn test_trace_miss_allow() {
        let mut veh =
            crate::vehicle::Vehicle::from_resource("2012_Ford_Fusion.yaml", false).unwrap();
        veh.mass = Some(10000.0 * uc::KG);
        let params = SimParams {
            trace_miss_opts: TraceMissOptions::Allow,
            ..Default::default()
        };
        let cyc = crate::drive_cycle::CYC_ACCEL.clone();
        let mut sim = SimDrive::new(veh, cyc, Some(params));
        assert!(sim.run().is_ok());
    }

    #[test]
    #[cfg(feature = "resources")]
    #[cfg(feature = "yaml")]
    fn test_trace_miss_allowchecked() {
        let mut veh =
            crate::vehicle::Vehicle::from_resource("2012_Ford_Fusion.yaml", false).unwrap();
        veh.mass = Some(10000.0 * uc::KG);
        let cyc = crate::drive_cycle::CYC_ACCEL.clone();
        // misses default tolerances
        let params = SimParams {
            trace_miss_opts: TraceMissOptions::AllowChecked,
            ..Default::default()
        };
        let mut sim = SimDrive::new(veh.clone(), cyc.clone(), Some(params));
        assert!(sim.run().is_err());
        // meets modified tolerances
        let params = SimParams {
            trace_miss_opts: TraceMissOptions::AllowChecked,
            trace_miss_tol: TraceMissTolerance {
                tol_dist: 1e6 * uc::M,
                tol_dist_frac: 10.0 * uc::R,
                tol_speed: *cyc.speed.max().unwrap(),
                tol_speed_frac: 1.0 * uc::R,
            },
            ..Default::default()
        };
        let mut sim = SimDrive::new(veh.clone(), cyc.clone(), Some(params));
        sim.run().unwrap();
        // misses mixed tolerances
        let params = SimParams {
            trace_miss_opts: TraceMissOptions::AllowChecked,
            trace_miss_tol: TraceMissTolerance {
                tol_dist_frac: 10.0 * uc::R,
                tol_speed: *cyc.speed.max().unwrap(),
                tol_speed_frac: 1.0 * uc::R,
                ..Default::default()
            },
            ..Default::default()
        };
        let mut sim = SimDrive::new(veh.clone(), cyc.clone(), Some(params));
        assert!(sim.run().is_err());
        // misses mixed tolerances
        let params = SimParams {
            trace_miss_opts: TraceMissOptions::AllowChecked,
            trace_miss_tol: TraceMissTolerance {
                tol_dist: 100. * uc::M,
                tol_dist_frac: 0.05 * uc::R,
                tol_speed_frac: 0.5 * uc::R,
                ..Default::default()
            },
            ..Default::default()
        };
        let mut sim = SimDrive::new(veh.clone(), cyc.clone(), Some(params));
        assert!(sim.run().is_err());
        // misses mixed tolerances
        let params = SimParams {
            trace_miss_opts: TraceMissOptions::AllowChecked,
            trace_miss_tol: TraceMissTolerance {
                tol_dist: 100. * uc::M,
                tol_dist_frac: 0.05 * uc::R,
                tol_speed: 10. * uc::MPS,
                ..Default::default()
            },
            ..Default::default()
        };
        let mut sim = SimDrive::new(veh.clone(), cyc.clone(), Some(params));
        assert!(sim.run().is_err());
    }

    // TODO: implement when TraceMissOptions::Warn is implemented
    // #[test]
    // #[cfg(feature = "yaml")]
    // fn test_trace_miss_warn() {
    //     let mut veh =
    //         crate::vehicle::Vehicle::from_resource("2012_Ford_Fusion.yaml", false).unwrap();
    //     veh.mass = Some(10000.0 * uc::KG);
    //     let params = SimParams {
    //         trace_miss_opts: TraceMissOptions::Warn,
    //         ..Default::default()
    //     };
    //     let cyc = crate::drive_cycle::CYC_ACCEL.clone();
    //     let mut sim = SimDrive::new(veh, cyc, Some(params));
    //     todo!();
    // }

    #[test]
    #[cfg(feature = "resources")]
    #[cfg(feature = "yaml")]
    fn test_trace_miss_error() {
        let mut veh =
            crate::vehicle::Vehicle::from_resource("2012_Ford_Fusion.yaml", false).unwrap();
        veh.mass = Some(10000.0 * uc::KG);
        let params = SimParams {
            trace_miss_opts: TraceMissOptions::Error,
            ..Default::default()
        };
        let cyc = crate::drive_cycle::CYC_ACCEL.clone();
        let mut sim = SimDrive::new(veh, cyc, Some(params));
        assert!(sim.run().is_err());
    }

    // TODO: why does sim.cyc.speed have spikes? sim.veh.history.speed_ach seems reasonable
    #[test]
    #[cfg(feature = "resources")]
    #[cfg(feature = "yaml")]
    fn test_trace_miss_correct() {
        let mut veh =
            crate::vehicle::Vehicle::from_resource("2012_Ford_Fusion.yaml", false).unwrap();
        veh.mass = Some(10000.0 * uc::KG);
        let params = SimParams {
            trace_miss_opts: TraceMissOptions::Correct,
            ..Default::default()
        };
        let cyc = crate::drive_cycle::CYC_ACCEL.clone();
        let mut sim = SimDrive::new(veh, cyc, Some(params));
        sim.run().unwrap();
    }
}
