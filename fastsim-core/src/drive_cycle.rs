pub mod maneuvers;
pub mod manipulation_utils;

use crate::drive_cycle::manipulation_utils::{
    speed_for_constant_jerk, ConstantJerkTrajectory, CycleCache,
};
use crate::imports::*;
use crate::prelude::*;
use std::cmp;

#[serde_api]
#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Default)]
#[non_exhaustive]
#[serde(deny_unknown_fields)]
#[cfg_attr(feature = "pyo3", pyclass(module = "fastsim", subclass, eq))]
/// Container
pub struct Cycle {
    /// Name of cycle (can be left empty)
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub name: String,
    /// inital elevation
    pub init_elev: Option<si::Length>,
    /// simulation time
    pub time: Vec<si::Time>,
    /// prescribed speed
    #[serde(alias = "speed_mps")]
    pub speed: Vec<si::Velocity>,
    // TODO: consider trapezoidal integration scheme
    /// calculated prescribed distance based on RHS integral of time and speed
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub dist: Vec<si::Length>,
    /// road grade (expressed as a decimal, not percent)
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    #[si_unit(percent)]
    pub grade: Vec<si::Ratio>,
    // TODO: consider trapezoidal integration scheme
    // TODO: @mokeefe, please check out how elevation is handled
    /// calculated prescribed elevation based on RHS integral distance and grade
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub elev: Vec<si::Length>,
    /// road charging/discharing capacity
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub pwr_max_chrg: Vec<si::Power>,
    /// ambient air temperature w.r.t. to time (rather than spatial position)
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub temp_amb_air: Vec<si::Temperature>,
    /// solar heat load w.r.t. to time (rather than spatial position)
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub pwr_solar_load: Vec<si::Power>,
    // TODO: add provision for optional time-varying aux load
    /// grade interpolator
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        serialize_with = "serialize_nested"
    )]
    pub grade_interp: Option<InterpolatorEnum<f64>>,
    /// elevation interpolator
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        serialize_with = "serialize_nested"
    )]
    pub elev_interp: Option<InterpolatorEnum<f64>>,
}

#[pyo3_api]
impl Cycle {
    #[pyo3(name = "len")]
    /// return the length of the cycle
    fn len_py(&self) -> PyResult<usize> {
        Ok(self.len_checked()?)
    }

    #[pyo3(name = "to_microtrips", signature=(stop_speed_m_per_s=None))]
    /// convert cycle to a list of microtrips.
    /// If stop speed is specified, it signifies the speed at or below which
    /// a vehicle should be considered as stopped. This can be useful when
    /// processing real-world data.
    fn to_microtrips_py(&self, stop_speed_m_per_s: Option<f64>) -> PyResult<Vec<Cycle>> {
        let stop_speed = stop_speed_m_per_s.map(|v| v * uc::MPS);
        Ok(self.to_microtrips(stop_speed))
    }

    #[pyo3(name = "extend_time", signature=(absolute_time_s=None, time_fraction=None))]
    /// extend cycle with idle time.
    /// This is useful when a cycle's duration needs to be extended.
    /// - absolute_time_s: optional time to extend the cycle
    /// - time_fraction: optional fraction of cycle's duration to add to cycle.
    ///
    /// NOTE: if both absolute time and time fraction are specified, they
    /// both add to extend the cycle. For example, if we have a 100 s cycle
    /// and specify an absolute_time_s of 10 and time_fraction of 0.5, the
    /// resulting cycle will have a duration of 160 s = 100.0 + (10 + 100.0 * 0.5)
    fn extend_time_py(
        &mut self,
        absolute_time_s: Option<f64>,
        time_fraction: Option<f64>,
    ) -> PyResult<Cycle> {
        let absolute_time = absolute_time_s.map(|t| t * uc::S);
        let time_fraction = time_fraction.map(|f| f * uc::R);
        Ok(self.extend_time(absolute_time, time_fraction))
    }

    #[pyo3(name = "dt_at_i")]
    /// time step duration at step i.
    pub fn dt_at_i_py(&self, i: usize) -> PyResult<f64> {
        let i = std::cmp::max(1, i);
        let dt = if i < self.time.len() {
            self.time[i].get::<si::second>() - self.time[i - 1].get::<si::second>()
        } else {
            0.0
        };
        Ok(dt)
    }

    #[pyo3(name = "ending_idle_time_s")]
    /// calculate and return the ending "idle" time of a cycle.
    /// "Idle" time is defined as the amount of contiguous time
    /// at the end of a cycle where the vehicle is not moving.
    pub fn ending_idle_time_py(&self) -> PyResult<f64> {
        let dt_end_idle = self.ending_idle_time();
        Ok(dt_end_idle.get::<si::second>())
    }

    #[pyo3(name = "trim_ending_idle", signature=(idle_to_keep_s=None))]
    /// trim ending "idle" time from a cycle.
    /// The "idle" time is the time the vehicle is not moving.
    /// - idle_to_keep_s: the amount of time to keep
    ///
    /// NOTE: if idle_to_keep_s is specified, the ending idle duration
    /// will be UP TO this idle_to_keep_s amount but could be less if
    /// there is insufficient idle time.
    pub fn trim_ending_idle_py(&self, idle_to_keep_s: Option<f64>) -> PyResult<Cycle> {
        let idle_to_keep = idle_to_keep_s.map(|idle| idle * uc::S);
        Ok(self.trim_ending_idle(idle_to_keep))
    }

    #[pyo3(name = "average_speed_m_per_s", signature=(while_moving=None))]
    /// calculate and return the average speed of the cycle in (m/s).
    /// - while_moving: if specified and true, calculate the speed only
    ///   while the vehicle is moving. Otherwise, calculate the average speed
    ///   including stopped time.
    pub fn average_speed_py(&self, while_moving: Option<bool>) -> PyResult<f64> {
        let while_moving = while_moving.unwrap_or(false);
        let vavg = self.average_speed(while_moving);
        Ok(vavg.get::<si::meter_per_second>())
    }

    #[pyo3(name = "average_step_speeds_m_per_s")]
    /// calculate and return the average speeds per time-step in (m/s).
    pub fn average_step_speeds_py(&self) -> PyResult<Vec<f64>> {
        Ok(self
            .average_step_speeds()
            .iter()
            .map(|v| v.get::<si::meter_per_second>())
            .collect())
    }

    #[pyo3(name = "average_step_speed_in_m_per_s_at")]
    /// calculate the average step speed at the given step in (m/s).
    pub fn average_step_speed_at_py(&self, i: usize) -> PyResult<f64> {
        Ok(self.average_step_speed_at(i).get::<si::meter_per_second>())
    }

    #[pyo3(name = "resample")]
    /// create a new cycle with the values resampled to the given time-step
    /// duration.
    pub fn resample_py(&self, time_step_s: f64) -> PyResult<Cycle> {
        let time_step = time_step_s.max(0.01) * uc::S;
        Ok(self.resample(time_step))
    }
}

lazy_static! {
    pub static ref ELEV_DEFAULT: si::Length = 400. * uc::FT;
}

impl Init for Cycle {
    /// Sets `self.dist` and `self.elev`
    /// # Assumptions
    /// - if `init_elev.is_none()`, then defaults to [static@ELEV_DEFAULT]
    fn init(&mut self) -> Result<(), Error> {
        let _ = self
            .len_checked()
            .map_err(|err| Error::InitError(format_dbg!(err)))?;

        if !self.temp_amb_air.is_empty() {
            if self.temp_amb_air.len() != self.time.len() {
                return Err(Error::InitError(format_dbg!()));
            }
        } else {
            self.temp_amb_air = vec![*TE_STD_AIR; self.time.len()];
        }

        // calculate distance from RHS integral of speed and time
        self.dist = {
            self.time
                .diff()
                .iter()
                .zip(&self.speed)
                .scan(0. * uc::M, |dist, (dt, speed)| {
                    *dist += *dt * *speed;
                    Some(*dist)
                })
                .collect()
        };

        // populate grade if not provided
        if self.grade.is_empty() {
            self.grade = vec![
                si::Ratio::ZERO;
                self.len_checked()
                    .map_err(|err| Error::InitError(format_dbg!(err)))?
            ]
        };
        // calculate elevation from RHS integral of grade and distance
        self.init_elev = self.init_elev.or_else(|| Some(*ELEV_DEFAULT));
        self.elev = self
            .grade
            .iter()
            .zip(&self.dist.diff())
            .scan(
                // already guaranteed to be `Some`
                self.init_elev.unwrap(),
                |elev, (grade, dist)| {
                    *elev += *dist * grade.atan().sin();
                    Some(*elev)
                },
            )
            .collect();
        let g0 = if !self.grade.is_empty() {
            self.grade[0]
        } else {
            0.0 * uc::R
        };
        if self.grade.iter().all(|&g| g != g0) {
            self.grade_interp = Some(
                InterpolatorEnum::new_1d(
                    self.dist.iter().map(|x| x.get::<si::meter>()).collect(),
                    self.grade.iter().map(|y| y.get::<si::ratio>()).collect(),
                    strategy::Linear,
                    Extrapolate::Error,
                )
                .map_err(|e| Error::NinterpError(e.to_string()))?,
            );

            self.elev_interp = Some(
                InterpolatorEnum::new_1d(
                    self.dist.iter().map(|x| x.get::<si::meter>()).collect(),
                    self.elev.iter().map(|y| y.get::<si::meter>()).collect(),
                    strategy::Linear,
                    Extrapolate::Error,
                )
                .map_err(|e| Error::NinterpError(e.to_string()))?,
            );
        } else {
            self.grade_interp = Some(InterpolatorEnum::new_0d(g0.get::<si::ratio>()));
            self.elev_interp = Some(InterpolatorEnum::new_0d(
                self.init_elev.unwrap().get::<si::meter>(),
            ));
        }

        Ok(())
    }
}

impl SerdeAPI for Cycle {
    const ACCEPTED_BYTE_FORMATS: &'static [&'static str] = &[
        #[cfg(feature = "csv")]
        "csv",
        #[cfg(feature = "json")]
        "json",
        #[cfg(feature = "msgpack")]
        "msgpack",
        #[cfg(feature = "toml")]
        "toml",
        #[cfg(feature = "yaml")]
        "yaml",
    ];
    const ACCEPTED_STR_FORMATS: &'static [&'static str] = &[
        #[cfg(feature = "csv")]
        "csv",
        #[cfg(feature = "json")]
        "json",
        #[cfg(feature = "toml")]
        "toml",
        #[cfg(feature = "yaml")]
        "yaml",
    ];
    #[cfg(feature = "resources")]
    const RESOURCES_SUBDIR: &'static str = "cycles";

    /// Write (serialize) an object into anything that implements [`std::io::Write`]
    ///
    /// # Arguments:
    ///
    /// * `wtr` - The writer into which to write object data
    /// * `format` - The target format, any of those listed in [`ACCEPTED_BYTE_FORMATS`](`SerdeAPI::ACCEPTED_BYTE_FORMATS`)
    ///
    fn to_writer<W: std::io::Write>(&self, mut wtr: W, format: &str) -> Result<(), Error> {
        match format.trim_start_matches('.').to_lowercase().as_str() {
            #[cfg(feature = "csv")]
            "csv" => {
                let mut wtr = csv::Writer::from_writer(wtr);
                for i in 0..self
                    .len_checked()
                    .map_err(|err| Error::SerdeError(format_dbg!(err)))?
                {
                    wtr.serialize(CycleElement {
                        // unchecked indexing should be ok because of `self.len()`
                        time: self.time[i],
                        speed: self.speed[i],
                        grade: if !self.grade.is_empty() {
                            Some(self.grade[i])
                        } else {
                            None
                        },
                        pwr_max_charge: if !self.pwr_max_chrg.is_empty() {
                            Some(self.pwr_max_chrg[i])
                        } else {
                            None
                        },
                        temp_amb_air: if !self.temp_amb_air.is_empty() {
                            Some(self.temp_amb_air[i])
                        } else {
                            None
                        },
                        pwr_solar_load: if !self.pwr_solar_load.is_empty() {
                            Some(self.pwr_solar_load[i])
                        } else {
                            None
                        },
                    })
                    .map_err(|err| Error::SerdeError(format_dbg!(err)))?;
                }
                wtr.flush()
                    .map_err(|err| Error::SerdeError(format_dbg!(err)))?
            }
            #[cfg(feature = "json")]
            "json" => serde_json::to_writer(wtr, self)
                .map_err(|err| Error::SerdeError(format_dbg!(err)))?,
            #[cfg(feature = "toml")]
            "toml" => {
                let toml_string = self
                    .to_toml()
                    .map_err(|err| Error::SerdeError(format_dbg!(err)))?;
                wtr.write_all(toml_string.as_bytes())
                    .map_err(|err| Error::SerdeError(format_dbg!(err)))?;
            }
            #[cfg(feature = "yaml")]
            "yaml" | "yml" => serde_yaml::to_writer(wtr, self)
                .map_err(|err| Error::SerdeError(format_dbg!(err)))?,
            _ => Err(Error::SerdeError(format!(
                "Unsupported format {format:?}, must be one of {:?}",
                Self::ACCEPTED_BYTE_FORMATS,
            )))?,
        }
        Ok(())
    }

    /// Deserialize an object from anything that implements [`std::io::Read`]
    ///
    /// # Arguments:
    ///
    /// * `rdr` - The reader from which to read object data
    /// * `format` - The source format, any of those listed in [`ACCEPTED_BYTE_FORMATS`](`SerdeAPI::ACCEPTED_BYTE_FORMATS`)
    ///
    fn from_reader<R: std::io::Read>(
        rdr: &mut R,
        format: &str,
        skip_init: bool,
    ) -> Result<Self, Error> {
        let mut deserialized: Self =
            match format.trim_start_matches('.').to_lowercase().as_str() {
                #[cfg(feature = "csv")]
                "csv" => {
                    // Create empty cycle to be populated
                    let mut cyc = Self::default();
                    let mut rdr = csv::Reader::from_reader(rdr);
                    for result in rdr.deserialize() {
                        cyc.push(result.map_err(|err| Error::SerdeError(format_dbg!(err)))?)
                            .map_err(|err| Error::SerdeError(format!("{err}")))?;
                    }
                    cyc
                }
                #[cfg(feature = "json")]
                "json" => serde_json::from_reader(rdr)
                    .map_err(|err| Error::SerdeError(format!("{err}")))?,
                #[cfg(feature = "toml")]
                "toml" => {
                    let mut buf = String::new();
                    rdr.read_to_string(&mut buf)
                        .map_err(|err| Error::SerdeError(format_dbg!(err)))?;
                    Self::from_toml(buf, skip_init)
                        .map_err(|err| Error::SerdeError(format_dbg!(err)))?
                }
                #[cfg(feature = "yaml")]
                "yaml" | "yml" => serde_yaml::from_reader(rdr)
                    .map_err(|err| Error::SerdeError(format_dbg!(err)))?,
                _ => {
                    return Err(Error::SerdeError(format!(
                        "Unsupported format {format:?}, must be one of {:?}",
                        Self::ACCEPTED_BYTE_FORMATS
                    )))
                }
            };
        if !skip_init {
            deserialized.init()?;
        }
        Ok(deserialized)
    }

    /// Write (serialize) an object into a string
    ///
    /// # Arguments:
    ///
    /// * `format` - The target format, any of those listed in [`ACCEPTED_STR_FORMATS`](`SerdeAPI::ACCEPTED_STR_FORMATS`)
    ///
    fn to_str(&self, format: &str) -> anyhow::Result<String> {
        match format.trim_start_matches('.').to_lowercase().as_str() {
            #[cfg(feature = "csv")]
            "csv" => self.to_csv(),
            #[cfg(feature = "json")]
            "json" => self.to_json(),
            #[cfg(feature = "toml")]
            "toml" => self.to_toml(),
            #[cfg(feature = "yaml")]
            "yaml" | "yml" => self.to_yaml(),
            _ => bail!(
                "Unsupported format {format:?}, must be one of {:?}",
                Self::ACCEPTED_STR_FORMATS
            ),
        }
    }

    /// Read (deserialize) an object from a string
    ///
    /// # Arguments:
    ///
    /// * `contents` - The string containing the object data
    /// * `format` - The source format, any of those listed in [`ACCEPTED_STR_FORMATS`](`SerdeAPI::ACCEPTED_STR_FORMATS`)
    ///
    fn from_str<S: AsRef<str>>(contents: S, format: &str, skip_init: bool) -> anyhow::Result<Self> {
        Ok(
            match format.trim_start_matches('.').to_lowercase().as_str() {
                #[cfg(feature = "csv")]
                "csv" => Self::from_csv(contents, skip_init)?,
                #[cfg(feature = "json")]
                "json" => Self::from_json(contents, skip_init)?,
                #[cfg(feature = "toml")]
                "toml" => Self::from_toml(contents, skip_init)?,
                #[cfg(feature = "yaml")]
                "yaml" | "yml" => Self::from_yaml(contents, skip_init)?,
                _ => bail!(
                    "Unsupported format {format:?}, must be one of {:?}",
                    Self::ACCEPTED_STR_FORMATS
                ),
            },
        )
    }
}

impl Cycle {
    /// rust-internal time steps at i
    pub fn dt_at_i(&self, i: usize) -> anyhow::Result<si::Time> {
        Ok(*self.time.get(i).with_context(|| format_dbg!())?
            - *self.time.get(i - 1).with_context(|| format_dbg!())?)
    }

    /// return the length of the cycle
    pub fn len_checked(&self) -> anyhow::Result<usize> {
        ensure!(
            self.time.len() == self.speed.len(),
            format!(
                "{}\n`time` and `speed` fields do not have same `len()`",
                format_dbg!()
            )
        );
        ensure!(
            self.dist.is_empty() || self.time.len() == self.dist.len(),
            format!(
                "{}\n`time` and `dist` fields do not have same `len()`",
                format_dbg!()
            )
        );
        ensure!(
            self.grade.is_empty() || self.time.len() == self.grade.len(),
            format!(
                "{}\n`time` and `grade` fields do not have same `len()`",
                format_dbg!()
            )
        );
        ensure!(
            self.elev.is_empty() || self.grade.len() == self.elev.len(),
            format!(
                "{}\n`grade` and `elev` fields do not have same `len()`",
                format_dbg!()
            )
        );
        ensure!(
            self.pwr_max_chrg.is_empty() || self.time.len() == self.pwr_max_chrg.len(),
            format!(
                "{}\n`time` and `pwr_max_chrg` fields do not have same `len()`",
                format_dbg!()
            )
        );
        ensure!(
            self.temp_amb_air.is_empty() || self.time.len() == self.temp_amb_air.len(),
            format!(
                "{}\n`time` and `temp_amb_air` fields do not have same `len()`",
                format_dbg!()
            )
        );
        Ok(self.time.len())
    }

    /// return true if the cycle is empty, else false
    pub fn is_empty(&self) -> anyhow::Result<bool> {
        Ok(self.len_checked().with_context(|| format_dbg!())? == 0)
    }

    /// append the given cycle element
    pub fn push(&mut self, element: CycleElement) -> anyhow::Result<()> {
        // TODO: maybe automate generation of this function as derive macro
        // TODO: maybe automate `ensure!` that all vec fields are same length before returning result
        // TODO: make sure all fields are being updated as appropriate
        self.time.push(element.time);
        self.speed.push(element.speed);
        match element.grade {
            Some(grade) => self.grade.push(grade),
            None => self.grade.push(si::Ratio::ZERO),
        }
        match element.pwr_max_charge {
            Some(pwr_max_chrg) => self.pwr_max_chrg.push(pwr_max_chrg),
            None => self.pwr_max_chrg.push(si::Power::ZERO),
        }
        match element.temp_amb_air {
            Some(temp_amb_air) => self.temp_amb_air.push(temp_amb_air),
            None => self.temp_amb_air.push(*TE_STD_AIR),
        }
        match element.pwr_solar_load {
            Some(pwr_solar_load) => self.pwr_solar_load.push(pwr_solar_load),
            None => self.pwr_solar_load.push(si::Power::ZERO),
        }
        Ok(())
    }

    /// extend the cycle by a vector of elements
    pub fn extend(&mut self, vec: Vec<CycleElement>) -> anyhow::Result<()> {
        self.time.extend(vec.iter().map(|x| x.time).clone());
        todo!();
        // self.time.extend(vec.iter().map(|x| x.time).clone());
        // match (&mut self.grade, vec.grade) {
        //     (Some(grade_mut), Some(grade)) => grade_mut.push(grade),
        //     (None, Some(_)) => {
        //         bail!("Element and Cycle `grade` fields must both be `Some` or `None`")
        //     }
        //     (Some(_), None) => {
        //         bail!("Element and Cycle `grade` fields must both be `Some` or `None`")
        //     }
        //     _ => {}
        // }
        // match (&mut self.pwr_max_chrg, vec.pwr_max_charge) {
        //     (Some(pwr_max_chrg_mut), Some(pwr_max_chrg)) => pwr_max_chrg_mut.push(pwr_max_chrg),
        //     (None, Some(_)) => {
        //         bail!("Element and Cycle `pwr_max_chrg` fields must both be `Some` or `None`")
        //     }
        //     (Some(_), None) => {
        //         bail!("Element and Cycle `pwr_max_chrg` fields must both be `Some` or `None`")
        //     }
        //     _ => {}
        // }
        // self.speed.push(vec.speed);
        // Ok(())
    }

    /// trim the cycle to the given start_idx and end_idx.
    ///
    /// NOTE: ending cycle will include start_idx but NOT end_idx
    pub fn trim(&mut self, start_idx: Option<usize>, end_idx: Option<usize>) -> anyhow::Result<()> {
        let start_idx = start_idx.unwrap_or_default();
        let len = self.len_checked().with_context(|| format_dbg!())?;
        let end_idx = end_idx.unwrap_or(len);
        ensure!(end_idx <= len, format_dbg!(end_idx <= len));

        self.time = self.time[start_idx..end_idx].to_vec();
        self.speed = self.speed[start_idx..end_idx].to_vec();
        Ok(())
    }

    /// Write (serialize) cycle to a CSV string
    #[cfg(feature = "csv")]
    pub fn to_csv(&self) -> anyhow::Result<String> {
        let mut buf = Vec::with_capacity(self.len_checked().with_context(|| format_dbg!())?);
        self.to_writer(&mut buf, "csv")?;
        Ok(String::from_utf8(buf)?)
    }

    /// Read (deserialize) an object from a CSV string
    ///
    /// # Arguments
    ///
    /// * `json_str` - JSON-formatted string to deserialize from
    ///
    #[cfg(feature = "csv")]
    fn from_csv<S: AsRef<str>>(csv_str: S, skip_init: bool) -> anyhow::Result<Self> {
        let mut csv_de = Self::from_reader(&mut csv_str.as_ref().as_bytes(), "csv", skip_init)?;
        if !skip_init {
            csv_de.init()?;
        }
        Ok(csv_de)
    }

    /// convert cycle to a vector of CycleElement
    pub fn to_elements(&self) -> Vec<CycleElement> {
        let mut result = Vec::with_capacity(self.time.len());
        for idx in 0..self.time.len() {
            let element = CycleElement {
                time: self.time[idx],
                speed: self.speed[idx],
                grade: if self.grade.is_empty() {
                    None
                } else {
                    Some(self.grade[idx])
                },
                pwr_max_charge: if self.pwr_max_chrg.is_empty() {
                    None
                } else {
                    Some(self.pwr_max_chrg[idx])
                },
                temp_amb_air: if self.temp_amb_air.is_empty() {
                    None
                } else {
                    Some(self.temp_amb_air[idx])
                },
                pwr_solar_load: if self.pwr_solar_load.is_empty() {
                    None
                } else {
                    Some(self.pwr_solar_load[idx])
                },
            };
            result.push(element);
        }
        result
    }

    /// Convert cycle into a vector of "microtrips".
    /// A microtrip is a start to a subsequent stop plus any idle time.
    /// - stop_speed: the speed at or below which vehicle is considered "stopped"
    ///
    /// RETURN: vector of cycles with each cycle being a "microtrip".
    pub fn to_microtrips(&self, stop_speed: Option<si::Velocity>) -> Vec<Cycle> {
        let stop_speed = stop_speed.unwrap_or(1e-6 * uc::MPS);
        let mut microtrips = Vec::new();
        let mut current = Cycle {
            name: self.name.clone(),
            init_elev: self.init_elev,
            time: vec![],
            speed: vec![],
            dist: vec![],
            grade: vec![],
            elev: vec![],
            pwr_max_chrg: vec![],
            temp_amb_air: vec![],
            pwr_solar_load: vec![],
            grade_interp: self.grade_interp.clone(),
            elev_interp: self.elev_interp.clone(),
        };
        let elements = self.to_elements();
        let mut moving: bool = false;
        for element in &elements {
            if element.speed > stop_speed && !moving && current.time.len() > 1 {
                current.init().unwrap();
                let last_idx = current.time.len() - 1;
                let last_time = current.time[last_idx];
                let last_speed = current.speed[last_idx];
                let last_grade = if last_idx >= current.grade.len() {
                    None
                } else {
                    Some(current.grade[last_idx])
                };
                let last_elevation = if last_idx >= current.elev.len() {
                    None
                } else {
                    Some(current.elev[last_idx])
                };
                let last_temperature = if last_idx >= current.temp_amb_air.len() {
                    None
                } else {
                    Some(current.temp_amb_air[last_idx])
                };
                let last_solar_load = if last_idx >= current.pwr_solar_load.len() {
                    None
                } else {
                    Some(current.pwr_solar_load[last_idx])
                };
                let last_charge_power = if last_idx >= current.pwr_max_chrg.len() {
                    None
                } else {
                    Some(current.pwr_max_chrg[last_idx])
                };
                current.time = current.time.iter().map(|t| *t - current.time[0]).collect();
                microtrips.push(current.clone());
                current = Cycle {
                    name: self.name.clone(),
                    init_elev: last_elevation,
                    time: vec![last_time],
                    speed: vec![last_speed],
                    dist: vec![],
                    grade: if let Some(g) = last_grade {
                        vec![g]
                    } else {
                        vec![]
                    },
                    elev: vec![],
                    pwr_max_chrg: if let Some(p) = last_charge_power {
                        vec![p]
                    } else {
                        vec![]
                    },
                    temp_amb_air: if let Some(temp) = last_temperature {
                        vec![temp]
                    } else {
                        vec![]
                    },
                    pwr_solar_load: if let Some(p) = last_solar_load {
                        vec![p]
                    } else {
                        vec![]
                    },
                    grade_interp: self.grade_interp.clone(),
                    elev_interp: self.elev_interp.clone(),
                };
            }
            current
                .push(element.clone())
                .expect("Push shouldn't have an error path");
            moving = element.speed > stop_speed;
        }
        if current.time.len() > 1 {
            current.time = current.time.iter().map(|t| *t - current.time[0]).collect();
            current.init().unwrap();
            microtrips.push(current.clone());
        }
        microtrips
    }

    /// Determine average speed of cycle.
    /// -- while_moving: if true, only takes average while moving
    ///
    /// RETURN: average speed
    pub fn average_speed(&self, while_moving: bool) -> si::Velocity {
        let mut d = si::Length::ZERO;
        let mut t = si::Time::ZERO;
        for idx in 1..self.speed.len() {
            let dt = self.time[idx] - self.time[idx - 1];
            let vavg = 0.5 * (self.speed[idx] + self.speed[idx - 1]);
            let dd = vavg * dt;
            let no_move = (dd.get::<si::meter>().ceil() as i32) == 0;
            d += dd;
            t += if while_moving && no_move {
                si::Time::ZERO
            } else {
                dt
            };
        }
        if t > si::Time::ZERO {
            d / t
        } else {
            si::Velocity::ZERO
        }
    }

    /// Return the average step speeds of the cycle as vector of velicities.
    /// NOTE: the average speed from sample i-1 to i will appear as entry i.
    /// RETURN: vector of velocities representing average step speeds.
    pub fn average_step_speeds(&self) -> Vec<si::Velocity> {
        let mut result = Vec::with_capacity(self.time.len());
        result.push(0.0 * uc::MPS);
        for i in 1..self.time.len() {
            result.push(0.5 * (self.speed[i] + self.speed[i - 1]));
        }
        result
    }

    /// Calculate the average step speed at step i
    /// (i.e., from sample point i-1 to i)
    pub fn average_step_speed_at(&self, i: usize) -> si::Velocity {
        if i >= self.speed.len() {
            return 0.0 * uc::MPS;
        }
        0.5 * (self.speed[i] + self.speed[i - 1])
    }

    /// The distances traveled over each step using trapezoidal
    /// integration.
    pub fn trapz_step_distances(&self) -> Vec<si::Length> {
        let mut result = Vec::with_capacity(self.time.len());
        result.push(0.0 * uc::M);
        for i in 1..self.time.len() {
            let step_time = self.time[i] - self.time[i - 1];
            let average_speed = 0.5 * (self.speed[i] + self.speed[i - 1]);
            result.push(step_time * average_speed);
        }
        result
    }

    /// The elevation climb each step using trapezoidal integration.
    // TODO: verify the height calculation is correct, see cycle init changes
    pub fn trapz_step_elevations(&self) -> Vec<si::Length> {
        let mut result = Vec::with_capacity(self.time.len());
        result.push(0.0 * uc::M);
        for i in 1..self.time.len() {
            let step_time = self.time[i].get::<si::second>() - self.time[i - 1].get::<si::second>();
            let average_speed = 0.5
                * (self.speed[i].get::<si::meter_per_second>()
                    + self.speed[i - 1].get::<si::meter_per_second>());
            let step_dist = step_time * average_speed;
            let gr = self.grade[i].get::<si::ratio>();
            let dh = gr.atan().cos() * step_dist * gr;
            result.push(dh * uc::M);
        }
        result
    }

    /// The distance traveled from start to the beginning of step i
    /// (i.e., distance traveled up to sample point i-1)
    pub fn trapz_step_start_distance(&self, step: usize) -> si::Length {
        let mut distance = 0.0 * uc::M;
        let step_max = cmp::min(step, self.time.len());
        for i in 1..step_max {
            let step_time = self.time[i] - self.time[i - 1];
            let average_speed = 0.5 * (self.speed[i] + self.speed[i - 1]);
            distance += step_time * average_speed;
        }
        distance
    }

    /// The distance traveled during the given step
    /// (i.e., distance from sample point i-1 to i for step i)
    pub fn trapz_distance_for_step(&self, step: usize) -> si::Length {
        let average_speed = self.average_step_speed_at(step);
        let elapsed_time = self.time[step] - self.time[step - 1];
        average_speed * elapsed_time
    }

    /// Calculate the distance from step i_start to the start of step i_end
    /// (i.e., distance from sample point i_start - 1 to i_end - 1)
    pub fn trapz_distance_over_range(&self, step0: usize, step1: usize) -> si::Length {
        let distances = self.trapz_step_distances();
        let last_i = cmp::max(distances.len() - 1, 0);
        let i_start = cmp::min(step0, last_i);
        let i_end = cmp::min(step1, last_i);
        let mut distance = 0.0 * uc::M;
        for d in &distances[cmp::min(i_start, i_end)..cmp::max(i_start, i_end)] {
            distance += *d;
        }
        distance
    }

    /// Calculate the time in a cycle spent moving
    /// - stopped_speed_m_per_s: the speed above which we are considered to be moving
    ///
    /// RETURN: the time spent moving in seconds
    pub fn time_spent_moving(&self, stopped_speed: Option<si::Velocity>) -> si::Time {
        let stop_speed = stopped_speed.unwrap_or(0.0 * uc::MPS);
        let mut result = 0.0 * uc::S;
        for i in 1..self.time.len() {
            let step_time = self.time[i] - self.time[i - 1];
            if self.speed[i] > stop_speed || self.speed[i - 1] > stop_speed {
                result += step_time;
            }
        }
        result
    }

    /// Create distance and target speeds by microtrip.
    /// Splits cycle into microtrips and returns a list of
    /// 2-tuples of:
    /// (distance from start in meters, target speed in m/s)
    /// The distance is measured to the start of the microtrip.
    ///
    /// # Parameters
    ///
    /// * `blend_factor`: from 0.0 to 1.0
    ///    - if 0.0, use the average speed of the microtrip
    ///    - if 1.0, use the average speed while moving (i.e., no stopped time)
    ///    - otherwise, something in between
    /// * `min_target_speed`: the minimum target speed allowed
    ///
    /// # Result
    ///
    /// List of 2-tuple of (distance from start, target speed).
    /// A tuple represents the distance from start of the start
    /// of the given microtrip and its target speed.
    ///
    /// # Notes
    ///
    /// * target speed per microtrip is not allowed to be
    ///   below the `min_target_speed`
    pub fn distance_and_target_speeds_by_microtrip(
        &self,
        stop_speed: Option<si::Velocity>,
        blend_factor: f64,
        min_target_speed: si::Velocity,
    ) -> Vec<(si::Length, si::Velocity)> {
        let blend_factor = blend_factor.clamp(0.0, 1.0);
        let mut result = Vec::new();
        let microtrips = self.to_microtrips(stop_speed);
        let mut distance_at_start = 0.0 * uc::M;
        let t0 = 0.0 * uc::S;
        let v0 = 0.0 * uc::MPS;
        let d0 = 0.0 * uc::M;
        for mt in microtrips {
            let distance = mt
                .trapz_step_distances()
                .iter()
                .fold(0.0 * uc::M, |total, dist| total + *dist);
            let last_index = cmp::max(mt.time.len() - 1, 0);
            let end_time = mt.time[last_index];
            let start_time = mt.time[0];
            let total_time = end_time - start_time;
            let moving_time = mt.time_spent_moving(stop_speed);
            let average_speed = if total_time > t0 {
                distance / total_time
            } else {
                v0
            };
            let moving_average_speed = if moving_time > t0 {
                distance / moving_time
            } else {
                v0
            };
            let target_speed =
                blend_factor * (moving_average_speed - average_speed) + average_speed;
            let target_speed = if target_speed > min_target_speed {
                target_speed
            } else {
                min_target_speed
            };
            if distance > d0 {
                result.push((distance_at_start, target_speed));
                distance_at_start += distance;
            }
        }
        result
    }

    /// Add idle time to Cycle.
    /// By "idle" time, we mean "stopped" time (i.e., vehicle not moving).
    pub fn extend_time(
        &self,
        absolute_time: Option<si::Time>,
        time_fraction: Option<si::Ratio>,
    ) -> Cycle {
        let absolute_time = absolute_time.unwrap_or(0.0 * uc::S);
        let time_fraction = time_fraction.unwrap_or(0.0 * uc::R);
        let mut ts = self.time.clone();
        let mut vs = self.speed.clone();
        let mut gs = self.grade.clone();
        let mut ps = self.pwr_max_chrg.clone();
        let mut temps = self.temp_amb_air.clone();
        let mut ss = self.pwr_solar_load.clone();
        let t_end = *ts.last().unwrap();
        let extra_time_s = (absolute_time.get::<si::second>()
            + time_fraction.get::<si::ratio>() * t_end.get::<si::second>())
        .round() as i32;
        if extra_time_s == 0 {
            return self.clone();
        }
        let dt = 1.0 * uc::S;
        let dt_s = dt.get::<si::second>();
        let mut idx = 1;
        loop {
            let dt_extra_s = dt_s * idx as f64;
            if dt_extra_s > extra_time_s as f64 {
                break;
            }
            ts.push(t_end + dt_extra_s * uc::S);
            vs.push(0.0 * uc::MPS);
            if !gs.is_empty() {
                gs.push(0.0 * uc::R);
            }
            if !ps.is_empty() {
                ps.push(*ps.last().unwrap());
            }
            if !temps.is_empty() {
                temps.push(*temps.last().unwrap());
            }
            if !ss.is_empty() {
                ss.push(*ss.last().unwrap());
            }
            idx += 1;
        }
        let mut cyc = Cycle {
            name: self.name.clone(),
            init_elev: self.init_elev,
            time: ts,
            speed: vs,
            dist: vec![],
            grade: gs,
            elev: vec![],
            pwr_max_chrg: vec![],
            grade_interp: self.grade_interp.clone(),
            elev_interp: self.elev_interp.clone(),
            temp_amb_air: temps,
            pwr_solar_load: ss,
        };
        cyc.init().unwrap();
        cyc
    }

    /// Create a cache object for faster computations on Cycle.
    pub fn build_cache(&self) -> CycleCache {
        CycleCache::new(self)
    }

    /// Returns the average grade over the given range of distances.
    /// - distance_start: the distance at start of evaluation area
    /// - delta_distance: distance traveled from distance_start
    /// - cache: optional CycleCache which can save computation time
    ///
    /// RETURN: average grade (rise over run) for the given range.
    ///
    /// NOTE: grade is assumed to be constant from just after the
    /// previous sample point until the current sample point (inclusive).
    /// That is, grade[i] applies from distance, d, of (d[i - 1], d[i]]
    pub fn average_grade_over_range(
        &self,
        distance_start: si::Length,
        delta_distance: si::Length,
        cache: Option<&CycleCache>,
    ) -> si::Ratio {
        let tol = 1e-6;
        match &cache {
            Some(cc) => {
                let dd_m = delta_distance.get::<si::meter>();
                if cc.grade_all_zero {
                    0.0 * uc::R
                } else if dd_m <= tol {
                    let dist_m = distance_start.get::<si::meter>();
                    cc.interp_grade(dist_m) * uc::R
                } else {
                    let dist0_m = distance_start.get::<si::meter>();
                    let dist1_m = dist0_m + dd_m;
                    let e0 = cc.interp_elevation(dist0_m);
                    let e1 = cc.interp_elevation(dist1_m);
                    ((e1 - e0) / dd_m).asin().tan() * uc::R
                }
            }
            None => {
                let zero_grade = 0.0 * uc::R;
                let grade_all_zero = {
                    let mut all0 = true;
                    for idx in 0..self.grade.len() {
                        if self.grade[idx] != zero_grade {
                            all0 = false;
                            break;
                        }
                    }
                    all0
                };
                if grade_all_zero {
                    0.0 * uc::R
                } else {
                    let delta_dists_m: Vec<f64> = self
                        .trapz_step_distances()
                        .iter()
                        .map(|dd| dd.get::<si::meter>())
                        .collect();
                    let trapz_distances_m = {
                        let mut d = 0.0;
                        let mut result = Vec::with_capacity(delta_dists_m.len());
                        for dd in &delta_dists_m {
                            d += *dd;
                            result.push(d);
                        }
                        result
                    };
                    let dist0_m = distance_start.get::<si::meter>();
                    let dd_m = delta_distance.get::<si::meter>();
                    let dist1_m = dist0_m + dd_m;
                    if dd_m < tol {
                        if dist0_m < trapz_distances_m[0] {
                            return self.grade[0];
                        }
                        let max_idx = self.grade.len() - 1;
                        if dist0_m > trapz_distances_m[max_idx] {
                            return self.grade[max_idx];
                        }
                        for idx in 1..self.time.len() {
                            if dist0_m > trapz_distances_m[idx - 1]
                                && dist0_m <= trapz_distances_m[idx]
                            {
                                return self.grade[idx];
                            }
                        }
                        self.grade[max_idx]
                    } else {
                        // NOTE: we use the following instead of delta_elev_m
                        // as it uses more precise trapezoidal diatance and
                        // elevation at sample points. This also uses the
                        // fully accurate trig functions in case we have large
                        // slope angles. This level of rigor may be overkill.
                        let trapz_elevations_m = {
                            let delta_elevs_m: Vec<f64> = self
                                .grade
                                .iter()
                                .zip(delta_dists_m)
                                .map(|(g, dd)| {
                                    let gr = g.get::<si::ratio>();
                                    gr.atan().cos() * dd * gr
                                })
                                .collect();
                            let mut result = Vec::with_capacity(delta_elevs_m.len());
                            let mut elev_m = 0.0;
                            for de in &delta_elevs_m {
                                elev_m += *de;
                                result.push(elev_m);
                            }
                            result
                        };
                        // dedup adjacent equal distances (e.g. while stopped), which
                        // ninterp's grid coordinates must not contain
                        let (interp_ds, interp_elevs): (Vec<f64>, Vec<f64>) = trapz_distances_m
                            .iter()
                            .zip(&trapz_elevations_m)
                            .fold((Vec::new(), Vec::new()), |(mut ds, mut es), (&d, &e)| {
                                if ds.is_empty() || d > *ds.last().unwrap() {
                                    ds.push(d);
                                    es.push(e);
                                }
                                (ds, es)
                            });
                        let interp: InterpolatorEnum<f64> = InterpolatorEnum::new_1d(
                            interp_ds.into(),
                            interp_elevs.into(),
                            strategy::Linear,
                            Extrapolate::Clamp,
                        )
                        .unwrap();
                        let e0_m = interp.interpolate(&[dist0_m]).unwrap();
                        let e1_m = interp.interpolate(&[dist1_m]).unwrap();
                        ((e1_m - e0_m) / dd_m).asin().tan() * uc::R
                    }
                }
            }
        }
    }

    /// Calculate the distance to next stop from `distance`.
    /// - distance: the distance to calculate distance-to-stop from
    ///
    /// RETURN: returns the distance to the next stop from `distance`
    ///
    /// NOTE: distance may be negative if we're beyond the last stop
    pub fn calc_distance_to_next_stop_from(
        &self,
        distance: si::Length,
        cache: Option<&CycleCache>,
    ) -> si::Length {
        let tol = 1e-6;
        let distance_m = distance.get::<si::meter>();
        match cache {
            Some(cc) => {
                for (&d_m, &v) in cc.trapz_distances_m.iter().zip(self.speed.iter()) {
                    let v_mps = v.get::<si::meter_per_second>();
                    if (v_mps < tol) && (d_m > (distance_m + tol)) {
                        return (d_m - distance_m) * uc::M;
                    }
                }
                (*cc.trapz_distances_m.last().unwrap_or(&0.0) * uc::M) - distance
            }
            None => {
                let ds_m = {
                    let mut result = Vec::with_capacity(self.time.len());
                    let mut d_m = 0.0;
                    for dd in self.trapz_step_distances() {
                        let dd_m = dd.get::<si::meter>();
                        d_m += dd_m;
                        result.push(d_m);
                    }
                    result
                };
                for (&d_m, &v) in ds_m.iter().zip(self.speed.iter()) {
                    let v_mps = v.get::<si::meter_per_second>();
                    if (v_mps < tol) && (d_m > (distance_m + tol)) {
                        return (d_m - distance_m) * uc::M;
                    }
                }
                *ds_m.last().unwrap_or(&0.0) * uc::M
            }
        }
    }

    /// Modify the cycle using the given constant-jerk trajectory.
    /// - i: the index into the cycle to initiate modification
    ///   NOTE: THIS point is modified as trajectory is calculated as
    ///   starting at i-1
    /// - n: the number of steps ahead
    /// - jerk: the jerk (deriviative of acceleration with time)
    /// - accel0: the starting accelartion
    ///
    /// NOTE:
    /// - modifies the cycle in-place. Purpose is to allow hitting
    ///   a rendezvous point in time/speed in the future.
    /// - CAUTION: not robust against variable duration time-steps
    ///
    /// RETURN: the final modified speed
    pub fn modify_by_const_jerk_trajectory(
        &mut self,
        i: usize,
        n: usize,
        jerk: si::Jerk,
        accel0: si::Acceleration,
    ) -> si::Velocity {
        if n == 0 {
            return si::Velocity::ZERO;
        }
        let jerk_m_per_s3 = jerk.get::<si::meter_per_second_cubed>();
        let accel0_m_per_s2 = accel0.get::<si::meter_per_second_squared>();
        let num_samples = self.speed.len();
        if i >= num_samples {
            if num_samples > 0 {
                return self.speed[num_samples - 1];
            }
            return si::Velocity::ZERO;
        }
        let v0 = self.speed[i - 1].get::<si::meter_per_second>();
        let dt = (self.time[i] - self.time[i - 1]).get::<si::second>();
        let mut v = v0;
        for ni in 1..(n + 1) {
            let idx_to_set = (i - 1) + ni;
            if idx_to_set >= num_samples {
                break;
            }
            v = speed_for_constant_jerk(ni, v0, accel0_m_per_s2, jerk_m_per_s3, dt);
            self.speed[idx_to_set] = v.max(0.0) * uc::MPS;
        }
        self.init().unwrap();
        v * uc::MPS
    }

    /// Modify cycle to add a braking trajectory that would cover the same
    /// distance as the given constant brake deceleration.
    /// - brake_accel: the brake acceleration (m/s2); must be negative
    /// - i: index where to initiate the stop trectory; start of the step
    /// - desired_distance_to_stop: the desired distance to stop within. If
    ///   not provided, it is calculated based on the braking deceleration.
    ///
    /// RETURN: (final speed of modified trajectory, number of steps to complete)
    /// - the final speed should be zero ideally
    /// - the number of time-steps required to complete the braking maneuver
    ///
    /// NOTE:
    /// - modifies the cycle in-place.
    pub fn modify_with_braking_trajectory(
        &mut self,
        brake_accel: si::Acceleration,
        i: usize,
        desired_distance_to_stop: Option<si::Length>,
    ) -> (si::Velocity, usize) {
        let brake_accel = if brake_accel > si::Acceleration::ZERO {
            -brake_accel
        } else {
            brake_accel
        };
        assert!(brake_accel < si::Acceleration::ZERO);
        if i >= self.time.len() {
            return (*self.speed.last().unwrap(), 0);
        }
        let i = if i < 1 { 1 } else { i };
        let v0 = self.speed[i - 1].get::<si::meter_per_second>();
        let dt = (self.time[i] - self.time[i - 1]).get::<si::second>();
        let brake_accel_m_per_s2 = brake_accel.get::<si::meter_per_second_squared>();
        // distance-to-stop (m)
        let dts_m = match desired_distance_to_stop {
            Some(value) => {
                let result = value.get::<si::meter>();
                if result > 0.0 {
                    result
                } else {
                    -0.5 * v0 * v0 / brake_accel_m_per_s2
                }
            }
            None => -0.5 * v0 * v0 / brake_accel_m_per_s2,
        };
        if dts_m <= 0.0 {
            return (v0 * uc::MPS, 0);
        }
        // time-to-stop (s)
        let tts_s = -v0 / brake_accel_m_per_s2;
        // number of steps to stop
        let n = (tts_s / dt).round() as usize;
        let n = if n < 2 { 2 } else { n }; // need at least 2 steps
        let traj =
            ConstantJerkTrajectory::from_speed_and_distance_targets(n, 0.0, v0, dts_m, 0.0, dt);
        let v_final = self.modify_by_const_jerk_trajectory(
            i,
            n,
            traj.jerk_m_per_s3 * uc::MPS3,
            traj.acceleration_m_per_s2 * uc::MPS2,
        );
        (v_final, n)
    }

    /// Report the stopped time (i.e., idle) at the end of a cycle.
    ///
    /// RESULT: time vehicle is at zero speed at cycle end
    pub fn ending_idle_time(&self) -> si::Time {
        let mut result = si::Time::ZERO;
        let vzero = si::Velocity::ZERO;
        for idx in (1..self.time.len()).rev() {
            let v0 = self.speed[idx - 1];
            let v1 = self.speed[idx];
            if v0 != vzero || v1 != vzero {
                break;
            } else {
                let dt = self.time[idx] - self.time[idx - 1];
                result += dt;
            }
        }
        result
    }

    /// Remove idel time at end of cycle except for the optionally
    /// specified duration.
    /// - idle_to_keep: optional duration of idle time to keep. Default is 0 s
    ///
    /// RESULT: a new cycle with idle time trimmed.
    pub fn trim_ending_idle(&self, idle_to_keep: Option<si::Time>) -> Cycle {
        let idle_to_keep = idle_to_keep.unwrap_or(si::Time::ZERO).max(si::Time::ZERO);
        let vzero = si::Velocity::ZERO;
        let mut idle_start_idx = 0;
        for idx in (1..self.time.len()).rev() {
            let v0 = self.speed[idx - 1];
            let v1 = self.speed[idx];
            if v0 != vzero || v1 != vzero {
                idle_start_idx = idx + 1;
                break;
            }
        }
        if idle_start_idx >= self.time.len() {
            return self.clone();
        }
        let end_idx = if idle_to_keep == si::Time::ZERO {
            idle_start_idx
        } else {
            let mut dt_idle = si::Time::ZERO;
            let mut idx_drop = idle_start_idx;
            for idx in idle_start_idx..self.time.len() {
                let dt = self.time[idx] - self.time[idx - 1];
                dt_idle += dt;
                if dt_idle > idle_to_keep {
                    idx_drop = idx;
                    break;
                }
            }
            idx_drop
        };
        let mut cyc = Cycle {
            name: self.name.clone(),
            time: self.time[0..end_idx].to_vec(),
            speed: self.speed[0..end_idx].to_vec(),
            init_elev: self.init_elev,
            grade: if self.grade.is_empty() {
                vec![]
            } else {
                self.grade[0..end_idx].to_vec()
            },
            dist: vec![],
            elev: vec![],
            pwr_max_chrg: if self.pwr_max_chrg.is_empty() {
                vec![]
            } else {
                self.pwr_max_chrg[0..end_idx].to_vec()
            },
            temp_amb_air: if self.temp_amb_air.is_empty() {
                vec![]
            } else {
                self.temp_amb_air[0..end_idx].to_vec()
            },
            pwr_solar_load: if self.pwr_solar_load.is_empty() {
                vec![]
            } else {
                self.pwr_solar_load[0..end_idx].to_vec()
            },
            grade_interp: None,
            elev_interp: None,
        };
        cyc.init().unwrap();
        cyc
    }

    /// Resample cycle to a lower or higher frequency.
    /// - dt: the new step duration.
    ///
    /// RETURN: cycle
    /// NOTE: a value of dt <= 0 s implies to just clone the current cycle
    /// "as is"
    pub fn resample(&self, dt: si::Time) -> Cycle {
        if dt <= si::Time::ZERO {
            return self.clone();
        }
        let mut t = si::Time::ZERO;
        let speed_interp: InterpolatorEnum<f64> = InterpolatorEnum::new_1d(
            self.time.iter().map(|x| x.get::<si::second>()).collect(),
            self.speed
                .iter()
                .map(|y| y.get::<si::meter_per_second>())
                .collect(),
            strategy::Linear,
            Extrapolate::Clamp,
        )
        .unwrap();
        let grade_interp: InterpolatorEnum<f64> = InterpolatorEnum::new_1d(
            self.time.iter().map(|x| x.get::<si::second>()).collect(),
            self.grade.iter().map(|y| y.get::<si::ratio>()).collect(),
            strategy::Step::upper(),
            Extrapolate::Clamp,
        )
        .unwrap();
        let temp_interp: Option<InterpolatorEnum<f64>> =
            if self.temp_amb_air.len() == self.time.len() {
                Some(
                    InterpolatorEnum::new_1d(
                        self.time.iter().map(|t| t.get::<si::second>()).collect(),
                        self.temp_amb_air
                            .iter()
                            .map(|temp| temp.get::<si::kelvin_abs>())
                            .collect(),
                        strategy::Linear,
                        Extrapolate::Clamp,
                    )
                    .unwrap(),
                )
            } else {
                None
            };
        let solar_interp: Option<InterpolatorEnum<f64>> =
            if self.pwr_solar_load.len() == self.time.len() {
                Some(
                    InterpolatorEnum::new_1d(
                        self.time.iter().map(|t| t.get::<si::second>()).collect(),
                        self.pwr_solar_load
                            .iter()
                            .map(|p| p.get::<si::kilowatt>())
                            .collect(),
                        strategy::Linear,
                        Extrapolate::Clamp,
                    )
                    .unwrap(),
                )
            } else {
                None
            };
        let chg_pwr_interp: Option<InterpolatorEnum<f64>> =
            if self.pwr_max_chrg.len() == self.time.len() {
                Some(
                    InterpolatorEnum::new_1d(
                        self.time.iter().map(|t| t.get::<si::second>()).collect(),
                        self.pwr_max_chrg
                            .iter()
                            .map(|p| p.get::<si::kilowatt>())
                            .collect(),
                        strategy::Linear,
                        Extrapolate::Clamp,
                    )
                    .unwrap(),
                )
            } else {
                None
            };
        let mut ts = vec![];
        let mut vs = vec![];
        let mut gs = vec![];
        let mut pwr_chg = vec![];
        let mut temps = vec![];
        let mut solars = vec![];
        while t <= self.time[self.time.len() - 1] {
            ts.push(t);
            let t0 = t.get::<si::second>();
            let v = speed_interp.interpolate(&[t0]).unwrap();
            vs.push(v * uc::MPS);
            let g = grade_interp.interpolate(&[t0]).unwrap();
            gs.push(g * uc::R);
            if let Some(ref interp) = chg_pwr_interp {
                let pchg = interp.interpolate(&[t0]).unwrap();
                pwr_chg.push(pchg * uc::KW);
            }
            if let Some(ref interp) = temp_interp {
                let temp = interp.interpolate(&[t0]).unwrap();
                temps.push(temp * uc::KELVIN);
            }
            if let Some(ref interp) = solar_interp {
                let solar = interp.interpolate(&[t0]).unwrap();
                solars.push(solar * uc::KW);
            }
            t += dt;
        }

        let mut cyc = Cycle {
            name: self.name.clone(),
            init_elev: self.init_elev,
            time: ts,
            speed: vs,
            dist: vec![],
            grade: gs,
            elev: vec![],
            pwr_max_chrg: pwr_chg,
            temp_amb_air: temps,
            pwr_solar_load: solars,
            grade_interp: None,
            elev_interp: None,
        };
        cyc.init().unwrap();
        cyc
    }
}

impl TryFrom<CycleBuilder> for Cycle {
    type Error = anyhow::Error;
    fn try_from(value: CycleBuilder) -> anyhow::Result<Self, Self::Error> {
        let mut cyc = Self {
            name: value.name,
            init_elev: None,
            time: value.time,
            speed: value.speed,
            dist: Default::default(),
            grade: Default::default(),
            elev: Default::default(),
            pwr_max_chrg: Default::default(),
            temp_amb_air: Default::default(),
            pwr_solar_load: Default::default(),
            grade_interp: None,
            elev_interp: Default::default(),
        };
        cyc.init()?;
        Ok(cyc)
    }
}

/// Trait for CycleBuilder and Cycle to support builder pattern
pub trait CBTrait {
    /// Return cycle with `grade`
    fn with_grade(&mut self, grade: Vec<si::Ratio>) -> anyhow::Result<Cycle>;

    /// Return cycle with `temp_amb_air`
    fn with_temp_amb_air(&mut self, temp_amb_air: Vec<si::Temperature>) -> anyhow::Result<Cycle>;

    // TODO: add more of these builder helpers
}

impl CBTrait for Cycle {
    fn with_grade(&mut self, grade: Vec<si::Ratio>) -> anyhow::Result<Cycle> {
        ensure!(
            self.len_checked().with_context(|| format_dbg!())? == grade.len(),
            format!(
                "{}\n`self.len()`: `{}\n`grade.len()`",
                self.len_checked().with_context(|| format_dbg!())?,
                grade.len()
            )
        );
        self.grade = grade;
        Ok(self.clone())
    }

    fn with_temp_amb_air(&mut self, temp_amb_air: Vec<si::Temperature>) -> anyhow::Result<Cycle> {
        ensure!(
            self.len_checked().with_context(|| format_dbg!())? == temp_amb_air.len(),
            format!(
                "{}\n`self.len()`: `{}\n`temp_amb_air.len()`",
                self.len_checked().with_context(|| format_dbg!())?,
                temp_amb_air.len()
            )
        );
        self.temp_amb_air = temp_amb_air;
        Ok(self.clone())
    }
}

#[serde_api]
#[derive(Default, Debug, Serialize, Deserialize, PartialEq, Clone)]
#[non_exhaustive]
/// Simple cycle to be converted into [Cycle] with appropriate defaults
pub struct CycleBuilder {
    /// Name of cycle (can be left empty)
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub name: String,
    /// simulation time
    pub time: Vec<si::Time>,
    /// prescribed speed
    pub speed: Vec<si::Velocity>,
}

impl CBTrait for CycleBuilder {
    fn with_grade(&mut self, grade: Vec<si::Ratio>) -> anyhow::Result<Cycle> {
        let mut cyc: Cycle = self.clone().try_into().with_context(|| format_dbg!())?;
        cyc.grade = grade;
        Ok(cyc)
    }

    fn with_temp_amb_air(&mut self, temp_amb_air: Vec<si::Temperature>) -> anyhow::Result<Cycle> {
        let mut cyc: Cycle = self.clone().try_into().with_context(|| format_dbg!())?;
        cyc.temp_amb_air = temp_amb_air;
        Ok(cyc)
    }
}

#[serde_api]
#[derive(Default, Debug, Serialize, Deserialize, PartialEq, Clone)]
#[non_exhaustive]
#[serde(deny_unknown_fields)]
#[cfg_attr(feature = "pyo3", pyclass(module = "fastsim", subclass, eq))]
/// Element of `Cycle`.  Used for vec-like operations.
pub struct CycleElement {
    /// simulation time \[s\]
    #[serde(alias = "cycSecs")]
    pub time: si::Time,
    /// simulation power \[W\]
    #[serde(alias = "speed_mps", alias = "cycMps")]
    pub speed: si::Velocity,
    // `dist` is not included here because it is derived in `Init::init`
    /// road grade
    #[serde(alias = "cycGrade")]
    #[si_unit(percent)]
    pub grade: Option<si::Ratio>,
    // `elev` is not included here because it is derived in `Init::init`
    /// road charging/discharing capacity
    pub pwr_max_charge: Option<si::Power>,
    // TODO: make sure all fields in cycle are represented here, as appropriate
    /// ambient air temperature w.r.t. to time (rather than spatial position)
    pub temp_amb_air: Option<si::Temperature>,
    /// solar heat load w.r.t. to time (rather than spatial position)
    pub pwr_solar_load: Option<si::Power>,
}

impl SerdeAPI for CycleElement {}
impl Init for CycleElement {}

#[pyo3_api]
impl CycleElement {}

#[cfg(test)]
mod tests {
    use super::{manipulation_utils::ConstantJerkTrajectory, *};
    /// Build, initialize, and return 2-element cycle
    fn mock_cyc_len_2() -> Cycle {
        let mut cyc = Cycle {
            name: String::new(),
            init_elev: None,
            time: (0..=2).map(|x| (x as f64) * uc::S).collect(),
            speed: (0..=2).map(|x| (x as f64) * uc::MPS).collect(),
            dist: vec![],
            grade: (0..=2).map(|x| (x as f64 * uc::R) / 100.).collect(),
            elev: vec![],
            pwr_max_chrg: vec![],
            grade_interp: Default::default(),
            elev_interp: Default::default(),
            temp_amb_air: Default::default(),
            pwr_solar_load: Default::default(),
        };
        cyc.init().unwrap();
        cyc
    }

    fn make_two_triangles_cycle() -> Cycle {
        let mut cyc = Cycle {
            name: String::from("Two Triangles"),
            init_elev: Some(0.0 * uc::M),
            time: vec![
                0.0 * uc::S,
                10.0 * uc::S,
                20.0 * uc::S,
                30.0 * uc::S,
                40.0 * uc::S,
                50.0 * uc::S,
            ],
            speed: vec![
                0.0 * uc::MPS,
                4.0 * uc::MPS,
                0.0 * uc::MPS,
                0.0 * uc::MPS,
                5.0 * uc::MPS,
                0.0 * uc::MPS,
            ],
            dist: vec![],
            grade: vec![
                0.0 * uc::R,
                0.0 * uc::R,
                0.0 * uc::R,
                0.0 * uc::R,
                0.01 * uc::R,
                0.01 * uc::R,
            ],
            elev: vec![],
            pwr_max_chrg: vec![],
            grade_interp: Default::default(),
            elev_interp: Default::default(),
            temp_amb_air: Default::default(),
            pwr_solar_load: Default::default(),
        };
        cyc.init().unwrap();
        cyc
    }

    #[test]
    fn test_init() {
        let cyc = mock_cyc_len_2();
        assert_eq!(
            cyc.dist,
            [0., 1., 3.] // meters
                .iter()
                .map(|x| *x * uc::M)
                .collect::<Vec<si::Length>>()
        );
        assert_eq!(
            cyc.elev,
            [121.92, 121.9299995000375, 121.9699915024367] // meters
                .iter()
                .map(|x| *x * uc::M)
                .collect::<Vec<si::Length>>()
        );
    }

    #[test]
    fn test_to_elements() {
        let cyc = mock_cyc_len_2();
        let elements = cyc.to_elements();
        assert_eq!(elements.len(), 3);
        assert_eq!(elements[0].time, 0.0 * uc::S);
        assert_eq!(elements[2].time, cyc.time[2]);
        assert_eq!(elements[2].speed, cyc.speed[2]);
        assert_eq!(elements[2].grade.unwrap(), 0.02 * uc::R);
        assert!(elements[2].pwr_max_charge.is_none());
        assert_eq!(elements[2].temp_amb_air.unwrap(), *TE_STD_AIR);
        assert!(elements[2].pwr_solar_load.is_none());
    }

    #[test]
    fn test_to_microtrips() {
        let cyc = make_two_triangles_cycle();
        let actual = cyc.to_microtrips(Some(0.01 * uc::MPH));
        assert_eq!(actual.len(), 2);
        let cyc0 = &actual[0];
        assert_eq!(
            cyc0.time,
            vec![0.0 * uc::S, 10.0 * uc::S, 20.0 * uc::S, 30.0 * uc::S]
        );
        assert_eq!(
            cyc0.speed,
            vec![0.0 * uc::MPS, 4.0 * uc::MPS, 0.0 * uc::MPS, 0.0 * uc::MPS]
        );
        assert_eq!(
            cyc0.grade,
            vec![0.0 * uc::R, 0.0 * uc::R, 0.0 * uc::R, 0.0 * uc::R]
        );
        let cyc1 = &actual[1];
        assert_eq!(cyc1.time, vec![0.0 * uc::S, 10.0 * uc::S, 20.0 * uc::S]);
        assert_eq!(
            cyc1.speed,
            vec![0.0 * uc::MPS, 5.0 * uc::MPS, 0.0 * uc::MPS]
        );
        assert_eq!(cyc1.grade, vec![0.0 * uc::R, 0.01 * uc::R, 0.01 * uc::R]);
    }

    #[test]
    fn test_distance_and_target_speeds_by_microtrip() {
        let cyc = make_two_triangles_cycle();
        let expected = [
            (0.0 * uc::M, (40.0 / 20.0) * uc::MPS),
            (40.0 * uc::M, (50.0 / 20.0) * uc::MPS),
        ];
        let actual = cyc.distance_and_target_speeds_by_microtrip(None, 1.0, 0.0 * uc::MPS);
        assert_eq!(actual.len(), expected.len());
        for i in 0..expected.len() {
            assert_eq!(actual[i].0, expected[i].0);
            assert_eq!(actual[i].1, expected[i].1);
        }
        let expected = [
            (0.0 * uc::M, (40.0 / 30.0) * uc::MPS),
            (40.0 * uc::M, (50.0 / 20.0) * uc::MPS),
        ];
        let actual = cyc.distance_and_target_speeds_by_microtrip(None, 0.0, 0.0 * uc::MPS);
        assert_eq!(actual.len(), expected.len());
        for i in 0..expected.len() {
            assert_eq!(actual[i].0, expected[i].0);
            assert_eq!(actual[i].1, expected[i].1);
        }
    }

    #[test]
    fn test_extending_cycle_time() {
        let cyc = make_two_triangles_cycle();
        let expected = {
            let mut c = Cycle {
                name: String::from("Two Triangles"),
                init_elev: Some(0.0 * uc::M),
                time: vec![
                    0.0 * uc::S,
                    10.0 * uc::S,
                    20.0 * uc::S,
                    30.0 * uc::S,
                    40.0 * uc::S,
                    50.0 * uc::S,
                    51.0 * uc::S,
                    52.0 * uc::S,
                    53.0 * uc::S,
                    54.0 * uc::S,
                    55.0 * uc::S,
                    56.0 * uc::S,
                    57.0 * uc::S,
                    58.0 * uc::S,
                ],
                speed: vec![
                    0.0 * uc::MPS,
                    4.0 * uc::MPS,
                    0.0 * uc::MPS,
                    0.0 * uc::MPS,
                    5.0 * uc::MPS,
                    0.0 * uc::MPS,
                    0.0 * uc::MPS,
                    0.0 * uc::MPS,
                    0.0 * uc::MPS,
                    0.0 * uc::MPS,
                    0.0 * uc::MPS,
                    0.0 * uc::MPS,
                    0.0 * uc::MPS,
                    0.0 * uc::MPS,
                ],
                dist: vec![],
                grade: vec![
                    0.0 * uc::R,
                    0.0 * uc::R,
                    0.0 * uc::R,
                    0.0 * uc::R,
                    0.01 * uc::R,
                    0.01 * uc::R,
                    0.0 * uc::R,
                    0.0 * uc::R,
                    0.0 * uc::R,
                    0.0 * uc::R,
                    0.0 * uc::R,
                    0.0 * uc::R,
                    0.0 * uc::R,
                    0.0 * uc::R,
                ],
                elev: vec![],
                pwr_max_chrg: vec![],
                grade_interp: Default::default(),
                elev_interp: Default::default(),
                temp_amb_air: Default::default(),
                pwr_solar_load: Default::default(),
            };
            c.init().unwrap();
            c
        };
        let absolute_time = Some(3.0 * uc::S);
        let time_fraction = Some(0.10 * uc::R);
        // extend by 3 s and 10% of existing time (i.e., 5 s)
        // = extend by 8 s
        let actual = cyc.extend_time(absolute_time, time_fraction);
        assert_eq!(actual, expected);
    }

    /// Round the given number n to the given number of digits
    /// - n: the number to round
    /// - digits: the digits to round or defaults to 2; if not positive,
    fn round(n: f64, digits: Option<i32>) -> f64 {
        let digits = digits.unwrap_or(2);
        let digits = if digits < 0 { 0 } else { digits };
        let multiplier = 10.0_f64.powi(digits);
        (n * multiplier).round() / multiplier
    }

    #[test]
    fn cycle_step_distances_are_as_expected() {
        let c = make_two_triangles_cycle();
        let expected = [
            0.0 * uc::M,
            20.0 * uc::M,
            20.0 * uc::M,
            0.0 * uc::M,
            25.0 * uc::M,
            25.0 * uc::M,
        ];
        let actual = c.trapz_step_distances();
        assert_eq!(actual.len(), expected.len());
        for i in 0..expected.len() {
            assert_eq!(actual[i], expected[i], "differ at step {i}");
        }
    }

    #[test]
    fn cycle_elevations_are_as_expected() {
        let c = make_two_triangles_cycle();
        let dh = 0.01_f64.atan().cos() * 25.0_f64 * 0.01_f64;
        let expected = [
            0.0 * uc::M,
            0.0 * uc::M,
            0.0 * uc::M,
            0.0 * uc::M,
            dh * uc::M,
            dh * uc::M,
        ];
        let actual = c.trapz_step_elevations();
        assert_eq!(actual.len(), expected.len());
        for i in 0..expected.len() {
            assert_eq!(actual[i], expected[i], "differ at step {i}");
        }
    }

    #[test]
    fn test_elevation_accumulation() {
        let mut cyc = Cycle {
            name: String::from("elevation test"),
            init_elev: Some(0.0 * uc::M),
            time: Vec::linspace(0., 1000., 1001)
                .iter()
                .map(|x| (*x as f64) * uc::S)
                .collect(),
            speed: vec![20.0 * uc::MPS; 1001],
            dist: vec![],
            grade: vec![0.05 * uc::R; 1001],
            elev: vec![],
            pwr_max_chrg: vec![],
            grade_interp: Default::default(),
            elev_interp: Default::default(),
            temp_amb_air: Default::default(),
            pwr_solar_load: Default::default(),
        };
        cyc.init().unwrap();

        let delta_elev = cyc.elev.last().unwrap().get::<si::meter>();
        // Expected elevation change: 20 m/s * 1000 s * sin(atan(0.05)) = 998.7523388778305 m
        assert!(almost_eq(delta_elev, 998.7523388778305, None));
    }

    #[test]
    fn cycle_cache_yields_same_results() {
        let c = make_two_triangles_cycle();
        let cache = c.build_cache();
        let dist_m = 0.0;
        let e0_expected = 0.0;
        let e0_actual = cache.interp_elevation(dist_m);
        assert_eq!(e0_actual, e0_expected);
        let dist_m = 65.0;
        let e1_expected = 0.01_f64.atan().cos() * 25.0_f64 * 0.01_f64;
        let e1_actual = cache.interp_elevation(dist_m);
        assert_eq!(e1_actual, e1_expected);
    }

    #[test]
    fn average_grade_over_range_is_correct() {
        let c = make_two_triangles_cycle();
        let cache = c.build_cache();
        let d0 = 40.0 * uc::M;
        let dd = 50.0 * uc::M;
        let expected0 = 0.01 * uc::R;
        let actual00 = c.average_grade_over_range(d0, dd, None);
        let actual00 = round(actual00.get::<si::ratio>(), Some(6)) * uc::R;
        assert_eq!(actual00, expected0);
        let actual01 = c.average_grade_over_range(d0, dd, Some(&cache));
        let actual01 = round(actual01.get::<si::ratio>(), Some(6)) * uc::R;
        assert_eq!(actual01, expected0);
    }

    #[test]
    fn distance_to_next_stop_is_correct() {
        let c = make_two_triangles_cycle();
        let cache = c.build_cache();
        let d = 20.0 * uc::M;
        let expected = 20.0 * uc::M;
        let actual = c.calc_distance_to_next_stop_from(d, None);
        assert_eq!(actual, expected);
        let actual = c.calc_distance_to_next_stop_from(d, Some(&cache));
        assert_eq!(actual, expected);
        let d = 65.0 * uc::M;
        let expected = 25.0 * uc::M;
        let actual = c.calc_distance_to_next_stop_from(d, None);
        assert_eq!(actual, expected);
        let actual = c.calc_distance_to_next_stop_from(d, Some(&cache));
        assert_eq!(actual, expected);
        let d = 0.0 * uc::M;
        let expected = 40.0 * uc::M;
        let actual = c.calc_distance_to_next_stop_from(d, None);
        assert_eq!(actual, expected);
        let actual = c.calc_distance_to_next_stop_from(d, Some(&cache));
        assert_eq!(actual, expected);
    }

    #[test]
    fn modifying_a_cycle_with_trajectory() {
        let c0 = make_two_triangles_cycle();
        let mut c = c0.clone();
        let n = 3;
        let d0 = 20.0; // units: m
        let v0 = 4.0; // units: m/s
        let dr = 65.0; // units: m
        let vr = 5.0; // units: m/s
        let dt = 10.0; // units: s
        let traj = ConstantJerkTrajectory::from_speed_and_distance_targets(n, d0, v0, dr, vr, dt);
        c.modify_by_const_jerk_trajectory(
            2,
            n,
            traj.jerk_m_per_s3 * uc::MPS3,
            traj.acceleration_m_per_s2 * uc::MPS2,
        );
        let expected = {
            let mut cyc = Cycle {
                name: String::from("Two Triangles"),
                init_elev: Some(0.0 * uc::M),
                time: vec![
                    0.0 * uc::S,
                    10.0 * uc::S,
                    20.0 * uc::S,
                    30.0 * uc::S,
                    40.0 * uc::S,
                    50.0 * uc::S,
                ],
                speed: vec![
                    0.0 * uc::MPS,
                    4.0 * uc::MPS,
                    traj.speed_at_step(1) * uc::MPS,
                    traj.speed_at_step(2) * uc::MPS,
                    5.0 * uc::MPS,
                    0.0 * uc::MPS,
                ],
                dist: vec![],
                grade: vec![
                    0.0 * uc::R,
                    0.0 * uc::R,
                    0.0 * uc::R,
                    0.0 * uc::R,
                    0.01 * uc::R,
                    0.01 * uc::R,
                ],
                elev: vec![],
                pwr_max_chrg: vec![],
                grade_interp: Default::default(),
                elev_interp: Default::default(),
                temp_amb_air: Default::default(),
                pwr_solar_load: Default::default(),
            };
            cyc.init().expect("initializaiton should not throw");
            cyc
        };
        assert_eq!(c.time.len(), expected.time.len());
        assert_eq!(c.speed.len(), expected.speed.len());
        assert_eq!(c.dist.len(), expected.dist.len());
        assert_eq!(c.grade.len(), expected.grade.len());
        for idx in 0..c.speed.len() {
            assert_eq!(c.time[idx], expected.time[idx]);
            assert_eq!(c.speed[idx], expected.speed[idx]);
            assert_eq!(c.dist[idx], expected.dist[idx]);
            assert_eq!(c.grade[idx], expected.grade[idx]);
        }
    }

    #[test]
    pub fn modify_with_braking_trajectory() {
        let mut actual = {
            let mut cyc = Cycle {
                name: String::from("Test"),
                init_elev: Some(0.0 * uc::M),
                time: vec![
                    0.0 * uc::S,
                    1.0 * uc::S,
                    2.0 * uc::S,
                    3.0 * uc::S,
                    4.0 * uc::S,
                    5.0 * uc::S,
                ],
                speed: vec![
                    0.0 * uc::MPS,
                    4.0 * uc::MPS,
                    4.0 * uc::MPS,
                    1.0 * uc::MPS,
                    1.0 * uc::MPS,
                    0.0 * uc::MPS,
                ],
                dist: vec![],
                grade: vec![],
                elev: vec![],
                pwr_max_chrg: vec![],
                grade_interp: Default::default(),
                elev_interp: Default::default(),
                temp_amb_air: Default::default(),
                pwr_solar_load: Default::default(),
            };
            cyc.init().expect("initializaiton should not throw");
            cyc
        };
        let precision = Some(6);
        let (v_end, n_steps) =
            actual.modify_with_braking_trajectory((-4.0 / 3.0) * uc::MPS2, 3, Some(4.0 * uc::M));
        let v_end = round(v_end.get::<si::meter_per_second>(), precision);
        assert_eq!(v_end, 0.0);
        assert_eq!(n_steps, 3);
        let expected = {
            let n = 3;
            let d0 = 0.0;
            let v0 = 4.0;
            let dr = 4.0;
            let vr = 0.0;
            let dt = 1.0;
            let traj =
                ConstantJerkTrajectory::from_speed_and_distance_targets(n, d0, v0, dr, vr, dt);
            let mut cyc = Cycle {
                name: String::from("Test"),
                init_elev: Some(0.0 * uc::M),
                time: vec![
                    0.0 * uc::S,
                    1.0 * uc::S,
                    2.0 * uc::S,
                    3.0 * uc::S,
                    4.0 * uc::S,
                    5.0 * uc::S,
                ],
                speed: vec![
                    0.0 * uc::MPS,
                    4.0 * uc::MPS,
                    4.0 * uc::MPS,
                    traj.speed_at_step(1) * uc::MPS,
                    traj.speed_at_step(2) * uc::MPS,
                    traj.speed_at_step(3) * uc::MPS,
                ],
                dist: vec![],
                grade: vec![],
                elev: vec![],
                pwr_max_chrg: vec![],
                grade_interp: Default::default(),
                elev_interp: Default::default(),
                temp_amb_air: Default::default(),
                pwr_solar_load: Default::default(),
            };
            cyc.init().expect("initializaiton should not throw");
            cyc
        };
        assert_eq!(actual.time.len(), expected.time.len());
        for i in 0..actual.time.len() {
            let at = round(actual.time[i].get::<si::second>(), precision);
            let et = round(expected.time[i].get::<si::second>(), precision);
            let av = round(actual.speed[i].get::<si::meter_per_second>(), precision);
            let ev = round(expected.speed[i].get::<si::meter_per_second>(), precision);
            let ad = round(actual.dist[i].get::<si::meter>(), precision);
            let ed = round(expected.dist[i].get::<si::meter>(), precision);
            assert_eq!(at, et, "time@t={et}&i={i}");
            assert_eq!(av, ev, "speed@t={et}&i={i}");
            assert_eq!(ad, ed, "dist@t={et}&i={i}");
        }
    }

    #[test]
    pub fn test_trim() {
        let c = make_two_triangles_cycle();
        let cyc = c.extend_time(Some(10.0 * uc::S), None);
        let dt_idle = cyc.ending_idle_time();
        assert_eq!(dt_idle, 10.0 * uc::S);
        // NOTE: extend_time adds time by 1.0 s increments so 10 points
        assert_eq!(cyc.time.len(), c.time.len() + 10);
        assert_eq!(*cyc.time.iter().last().unwrap(), 60.0 * uc::S);
        let cyc_trimmed = cyc.trim_ending_idle(None);
        assert_eq!(cyc_trimmed.time.len(), c.time.len());
    }
    type StructWithResources = Cycle;

    #[test]
    fn test_resources() {
        let resource_list = StructWithResources::list_resources().unwrap();
        assert!(!resource_list.is_empty());

        // verify that resources can all load
        for resource in resource_list {
            StructWithResources::from_resource(resource.clone(), false)
                .with_context(|| format_dbg!(resource))
                .unwrap();
        }
    }

    #[test]
    fn test_resample() {
        let cyc0 = {
            let mut c = Cycle {
                name: String::from("a test"),
                time: vec![0.0 * uc::S, 10.0 * uc::S, 20.0 * uc::S],
                speed: vec![0.0 * uc::MPS, 10.0 * uc::MPS, 0.0 * uc::MPS],
                grade: vec![0.01 * uc::R, 0.01 * uc::R, -0.01 * uc::R],
                init_elev: None,
                dist: vec![],
                elev: vec![],
                pwr_max_chrg: vec![],
                temp_amb_air: vec![],
                pwr_solar_load: vec![],
                grade_interp: None,
                elev_interp: None,
            };
            c.init().unwrap();
            c
        };
        let cyc1 = cyc0.resample(1.0 * uc::S);
        assert_eq!(21, cyc1.time.len());
        assert_eq!(
            cyc1.time[cyc1.time.len() - 1],
            cyc0.time[cyc0.time.len() - 1]
        );
        assert_eq!(cyc1.time[0], cyc0.time[0]);
        assert_eq!(cyc1.time[0], 0.0 * uc::S);
        assert_eq!(cyc1.time[5], 5.0 * uc::S);
        assert_eq!(cyc1.speed[5], 5.0 * uc::MPS);
        assert_eq!(cyc1.grade[5], 0.01 * uc::R);
        assert_eq!(cyc1.time[10], 10.0 * uc::S);
        assert_eq!(cyc1.speed[10], 10.0 * uc::MPS);
        assert_eq!(cyc1.grade[10], 0.01 * uc::R);
        assert_eq!(cyc1.time[11], 11.0 * uc::S);
        assert_eq!(cyc1.speed[11], 9.0 * uc::MPS);
        assert_eq!(cyc1.grade[11], -0.01 * uc::R);
        assert_eq!(cyc1.time[20], 20.0 * uc::S);
        assert_eq!(cyc1.speed[20], 0.0 * uc::MPS);
        assert_eq!(cyc1.grade[20], -0.01 * uc::R);
    }
}

lazy_static! {
    pub static ref CYC_ACCEL: Cycle = Cycle::try_from(CycleBuilder {
        name: String::from("accel test"),
        time: (0..300)
            .map(|t| (t as f64) * uc::S)
            .collect::<Vec<si::Time>>(),
        speed: vec![90.0 * uc::MPH; 300],
    })
    .unwrap();
}
