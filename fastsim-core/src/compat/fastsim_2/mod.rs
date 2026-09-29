use fastsim_core::traits::SerdeAPI as _;

use super::*;

pub extern crate fastsim_2 as fastsim_core;

use include_dir::{include_dir, Dir};
pub const ASSETS_DIR: &'static Dir<'_> =
    &include_dir!("$CARGO_MANIFEST_DIR/src/compat/fastsim_2/assets");

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    #[cfg(all(feature = "yaml", feature = "csv", feature = "resources"))]
    fn test_deserialize_and_convert_roundtrip() {
        let cyc = Cycle::from_resource("udds.csv", false).unwrap();
        // Vehicles
        ASSETS_DIR
            .get_dir("vehicles")
            .unwrap()
            .files()
            .for_each(|file| {
                let f2_veh =
                    fastsim_core::vehicle::RustVehicle::from_reader(file.contents(), "yaml", false)
                        .unwrap();
                let f3_veh = Vehicle::try_from(f2_veh).unwrap();
                let mut f3_sd = SimDrive::new(f3_veh, cyc.clone(), None);
                assert!(f3_sd.run().is_ok());
                let mut f2_sd = f3_sd.to_fastsim2().unwrap();
                assert!(f2_sd.sim_drive(None, None).is_ok());
            });
    }
}

impl SimDrive {
    pub fn to_fastsim2(&self) -> anyhow::Result<fastsim_core::simdrive::RustSimDrive> {
        let veh2 = self
            .veh
            .to_fastsim2()
            .with_context(|| anyhow!(format_dbg!()))?;
        let cyc2 = self
            .cyc
            .to_fastsim2()
            .with_context(|| anyhow!(format_dbg!()))?;
        Ok(fastsim_core::simdrive::RustSimDrive::new(cyc2, veh2))
    }
}

impl Vehicle {
    /// Function to convert back to fastsim-2 format.  Note that this is
    /// probably not 100% reliable.
    pub fn to_fastsim2(&self) -> anyhow::Result<fastsim_core::vehicle::RustVehicle> {
        let mut veh = fastsim_core::vehicle::RustVehicle {
            alt_eff: match &self.pt_type {
                PowertrainType::ConventionalVehicle(conv) => conv.alt_eff.get::<si::ratio>(),
                _ => 1.0,
            },
            alt_eff_doc: None,
            aux_kw: self.pwr_aux_base.get::<si::kilowatt>(),
            aux_kw_doc: None,
            cargo_kg: self
                .chassis
                .cargo_mass
                .unwrap_or_default()
                .get::<si::kilogram>(),
            cargo_kg_doc: None,
            charging_on: false,
            chg_eff: 0.86, // TODO: revisit?
            chg_eff_doc: None,
            comp_mass_multiplier: 1.4,
            comp_mass_multiplier_doc: None,
            // TODO: replace with `doc` field once implemented in fastsim-3
            doc: None,
            drag_coef: self.chassis.drag_coef.get::<si::ratio>(),
            drag_coef_doc: None,
            drive_axle_weight_frac: self.chassis.drive_axle_weight_frac.get::<si::ratio>(),
            drive_axle_weight_frac_doc: None,
            ess_base_kg: 75.0, // NOTE: this hardcoded value could cause trouble
            ess_base_kg_doc: None,
            ess_chg_to_fc_max_eff_perc: 0.0, // TODO: ??? update later
            ess_chg_to_fc_max_eff_perc_doc: None,
            ess_dischg_to_fc_max_eff_perc: 0.0, // TODO: ??? update later
            ess_dischg_to_fc_max_eff_perc_doc: None,
            ess_kg_per_kwh: 8.0, // TODO: revisit
            ess_kg_per_kwh_doc: None,
            ess_life_coef_a: 110.,
            ess_life_coef_a_doc: None,
            ess_life_coef_b: -0.6811,
            ess_life_coef_b_doc: None,
            ess_mass_kg: self.res().map_or(anyhow::Ok(0.), |res| {
                Ok(res.mass()?.unwrap_or_default().get::<si::kilogram>())
            })?,
            ess_max_kw: self
                .res()
                .map(|res| res.pwr_out_max.get::<si::kilowatt>())
                .unwrap_or_default(),
            ess_max_kw_doc: None,
            ess_max_kwh: self
                .res()
                .map(|res| res.energy_capacity.get::<si::kilowatt_hour>())
                .unwrap_or_default(),
            ess_max_kwh_doc: None,
            ess_round_trip_eff: self
                .res()
                .map(|res| {
                    if let crate::vehicle::powertrain::reversible_energy_storage::RESEfficiency::Constant(Interp0D(eff)) = res.eff_interp {
                        Ok(eff.powi(2))
                    } else {
                        bail!("`to_fastsim2` is not implemented for non-0D `res.eff_interp`")
                    }
                })
                .transpose()?
                .unwrap_or(f64::NAN),
            ess_round_trip_eff_doc: None,
            ess_to_fuel_ok_error: 0.005, // TODO: update when hybrid logic is implemented
            ess_to_fuel_ok_error_doc: None,
            fc_base_kg: 61.0, // TODO: revisit
            fc_base_kg_doc: None,
            fc_eff_array: Default::default(),
            fc_eff_map: self
                .fc()
                .map(|fc| match &fc.eff_interp_from_pwr_out {
                    InterpolatorEnum::Interp1D(interp) => Ok(interp.data.values.to_vec().into()),
                    _ => bail!(format_dbg!(
                        "Only 1-D interpolators can be converted to FASTSim 2"
                    )),
                })
                .transpose()?
                .unwrap_or_else(|| vec![0., 0.].into()),
            fc_eff_map_doc: None,
            fc_eff_type: match &self.pt_type {
                PowertrainType::ConventionalVehicle(_) => "SI".into(),
                PowertrainType::HybridElectricVehicle(_) => "Atkinson".into(),
                PowertrainType::PlugInHybridElectricVehicle(_) => "Atkinson".into(),
                PowertrainType::BatteryElectricVehicle(_) => "SI".into(),
            },
            fc_eff_type_doc: None,
            fc_kw_out_array: Default::default(),
            fc_kw_per_kg: 2.13, // TODO: revisit
            fc_kw_per_kg_doc: None,
            fc_mass_kg: self.fc().map_or(anyhow::Ok(0.), |fc| {
                Ok(fc.mass()?.unwrap_or_default().get::<si::kilogram>())
            })?,
            fc_max_kw: self
                .fc()
                .map(|fc| fc.pwr_out_max.get::<si::kilowatt>())
                .unwrap_or_default(),
            fc_max_kw_doc: None,
            fc_peak_eff_override: None,
            fc_peak_eff_override_doc: None,
            fc_perc_out_array: Default::default(),
            fc_pwr_out_perc: self
                .fc()
                .map(|fc| match &fc.eff_interp_from_pwr_out {
                    InterpolatorEnum::Interp1D(interp) => Ok(interp.data.grid[0].to_vec().into()),
                    _ => bail!(format_dbg!(
                        "Only 1-D interpolators can be converted to FASTSim 2"
                    )),
                })
                .transpose()?
                .unwrap_or_else(|| vec![0., 1.].into()),
            fc_pwr_out_perc_doc: None,
            fc_sec_to_peak_pwr: self
                .fc()
                .map(|fc| fc.pwr_ramp_lag.get::<si::second>())
                .unwrap_or_default(),
            fc_sec_to_peak_pwr_doc: None,
            force_aux_on_fc: matches!(self.pt_type, PowertrainType::ConventionalVehicle(_)),
            force_aux_on_fc_doc: None,
            frontal_area_m2: self.chassis.frontal_area.get::<si::square_meter>(),
            frontal_area_m2_doc: None,
            fs_kwh: self
                .fs()
                .map(|fs| fs.energy_capacity.get::<si::kilowatt_hour>())
                .unwrap_or_default(),
            fs_kwh_doc: None,
            fs_kwh_per_kg: self
                .fs()
                .and_then(|fs| fs.specific_energy)
                .map(|specific_energy| specific_energy.get::<si::kilojoule_per_kilogram>() / 3600.)
                .unwrap_or_default(),
            fs_kwh_per_kg_doc: None,
            fs_mass_kg: self.fs().map_or(anyhow::Ok(0.), |fs| {
                Ok(fs.mass()?.unwrap_or_default().get::<si::kilogram>())
            })?,
            fs_max_kw: self
                .fs()
                .map(|fs| fs.pwr_out_max.get::<si::kilowatt>())
                .unwrap_or_default(),
            fs_max_kw_doc: None,
            fs_secs_to_peak_pwr: self
                .fs()
                .map(|fs| fs.pwr_ramp_lag.get::<si::second>())
                .unwrap_or_default(),
            fs_secs_to_peak_pwr_doc: None,
            glider_kg: self
                .chassis
                .glider_mass
                .unwrap_or_default()
                .get::<si::kilogram>(),
            glider_kg_doc: None,
            // 4.3 is for 2016 Toyota Prius Two, not sure this matters, though
            idle_fc_kw: 4.3,
            idle_fc_kw_doc: None,
            input_kw_out_array: Default::default(), // calculated in `set_derived()`
            kw_demand_fc_on: match &self.pt_type {
                PowertrainType::HybridElectricVehicle(hev) => match &hev.pt_cntrl {
                    HEVPowertrainControls::RGWDB(rgwb) => (rgwb
                        .frac_pwr_demand_fc_forced_on
                        .with_context(|| format_dbg!("Expected `Some`."))?
                        * (hev.fc.pwr_out_max + hev.res.pwr_out_max.min(hev.em.pwr_out_max)))
                    .get::<si::kilowatt>(),
                    HEVPowertrainControls::StartStop(_) => 0.0,
                },
                _ => 0.0,
            },
            kw_demand_fc_on_doc: None,
            large_motor_power_kw: 75.0,
            max_accel_buffer_mph: 60.0, // TODO: placeholder, revisit
            max_accel_buffer_mph_doc: None,
            max_accel_buffer_perc_of_useable_soc: 0.2, // TODO: placeholder, revisit
            max_accel_buffer_perc_of_useable_soc_doc: None,
            max_regen: 0.98, // TODO: placeholder, revisit
            max_regen_doc: None,
            max_regen_kwh: Default::default(),
            max_roadway_chg_kw: Default::default(),
            max_soc: self
                .res()
                .map(|res| res.max_soc.get::<si::ratio>())
                .unwrap_or_else(|| 1.0),
            max_soc_doc: None,
            max_trac_mps2: Default::default(),
            mc_eff_array: Default::default(), // calculated in `set_derived`
            mc_eff_map: self
                .em()
                .map(|em| match &em.eff_interp {
                    EMEfficiency::PwrOutFrac(interp) => Ok(interp.data.values.clone()),
                    EMEfficiency::Constant(_) => bail!(format_dbg!(
                        "Only `PwrOutFrac` efficiency maps can be converted to FASTSim 2"
                    )),
                })
                .transpose()?
                .map(|f_x| f_x.to_vec())
                .unwrap_or_else(|| vec![0., 1.])
                .into(),
            mc_eff_map_doc: None,
            mc_full_eff_array: Default::default(), // calculated in `set_derived`
            mc_kw_in_array: Default::default(),    // calculated in `set_derived`
            mc_kw_out_array: Default::default(),   // calculated in `set_derived`
            mc_mass_kg: self.em().map_or(anyhow::Ok(0.), |em| {
                Ok(em.mass()?.unwrap_or_default().get::<si::kilogram>())
            })?,
            mc_max_elec_in_kw: Default::default(), // calculated in `set_derived`
            mc_max_kw: self
                .em()
                .map(|em| em.pwr_out_max.get::<si::kilowatt>())
                .unwrap_or_default(),
            mc_max_kw_doc: None,
            mc_pe_base_kg: 0.0, // placeholder, TODO: review when implementing xEVs
            mc_pe_base_kg_doc: None,
            mc_pe_kg_per_kw: 0.833, // placeholder, TODO: review when implementing xEVs
            mc_pe_kg_per_kw_doc: None,
            mc_peak_eff_override: Default::default(),
            mc_peak_eff_override_doc: None,
            mc_perc_out_array: Default::default(),
            // short array that can use xEV when implented.  TODO: fix this when implementing xEV
            mc_pwr_out_perc: self
                .em()
                .map(|em| match &em.eff_interp {
                    EMEfficiency::PwrOutFrac(interp) => Ok(interp.data.grid[0].clone()),
                    EMEfficiency::Constant(_) => bail!(format_dbg!(
                        "Only `PwrOutFrac` efficiency maps can be converted to FASTSim 2"
                    )),
                })
                .transpose()?
                .map(|x| x.to_vec())
                .unwrap_or_else(|| vec![0., 1.])
                .into(),
            mc_pwr_out_perc_doc: None,
            // 4.8 is hardcoded for 2016 Toyota Prius Two
            mc_sec_to_peak_pwr: 4.8,
            mc_sec_to_peak_pwr_doc: None,
            // NOTE: this seems to have no effect in fastsim-2 so it can be anything
            min_fc_time_on: 60.0,
            min_fc_time_on_doc: None,
            min_soc: self
                .res()
                .map(|res| res.min_soc.get::<si::ratio>())
                .unwrap_or_default(),
            min_soc_doc: None,
            modern_max: 0.95,
            mph_fc_on: match &self.pt_type {
                PowertrainType::HybridElectricVehicle(hev) => match &hev.pt_cntrl {
                    HEVPowertrainControls::RGWDB(rgwb) => rgwb
                        .speed_fc_forced_on
                        .with_context(|| format_dbg!("Expected Some"))?
                        .get::<si::mile_per_hour>(),
                    HEVPowertrainControls::StartStop(_) => 0.0,
                },
                _ => 0.0,
            },
            mph_fc_on_doc: None,
            no_elec_aux: false, // TODO: revisit when implemementing HEV
            no_elec_sys: false, // TODO: revisit when implemementing HEV
            num_wheels: self.chassis.num_wheels as f64,
            num_wheels_doc: None,
            orphaned: false,
            perc_high_acc_buf: Default::default(), // TODO: revisit when implemementing HEV
            perc_high_acc_buf_doc: None,
            props: fastsim_core::params::RustPhysicalProperties::default(),
            regen_a: 500.0, //TODO: placeholder
            regen_b: 0.99,  //TODO: placeholder
            scenario_name: self.name.clone(),
            selection: 0, // there is no equivalent in fastsim-3
            small_motor_power_kw: 7.5,
            stop_start: false,
            stop_start_doc: None,
            trans_eff: {
                match self
                    .trans()
                    .cloned()
                    .with_context(|| format!("{}\nExpected `Some`", format_dbg!()))?
                    .eff_interp
                {
                    InterpolatorEnum::Interp0D(eff) => eff.0,
                    _ => todo!(),
                }
            },
            trans_eff_doc: None,
            trans_kg: 114.0, // TODO: replace with actual transmission mass
            trans_kg_doc: None,
            val0_to60_mph: f64::NAN,
            val_cd_range_mi: f64::NAN,
            val_comb_kwh_per_mile: f64::NAN,
            val_comb_mpgge: f64::NAN,
            val_const45_mph_kwh_per_mile: f64::NAN,
            val_const55_mph_kwh_per_mile: f64::NAN,
            val_const60_mph_kwh_per_mile: f64::NAN,
            val_const65_mph_kwh_per_mile: f64::NAN,
            val_ess_life_miles: f64::NAN,
            val_hwy_kwh_per_mile: f64::NAN,
            val_hwy_mpgge: f64::NAN,
            val_msrp: f64::NAN,
            val_range_miles: f64::NAN,
            val_udds_kwh_per_mile: f64::NAN,
            val_udds_mpgge: f64::NAN,
            val_unadj_hwy_kwh_per_mile: f64::NAN,
            val_unadj_udds_kwh_per_mile: f64::NAN,
            val_veh_base_cost: f64::NAN,
            veh_cg_m: self.chassis.cg_height.get::<si::meter>()
                * match self.chassis.drive_type {
                    chassis::DriveTypes::FWD => 1.0,
                    chassis::DriveTypes::RWD
                    | chassis::DriveTypes::AWD
                    | chassis::DriveTypes::FourWD => -1.0,
                },
            veh_cg_m_doc: None,
            veh_kg: self
                .mass()?
                .context("Vehicle mass is `None`")?
                .get::<si::kilogram>(),
            veh_override_kg: self.mass()?.map(|m| m.get::<si::kilogram>()),
            veh_override_kg_doc: None,
            veh_pt_type: match &self.pt_type {
                PowertrainType::ConventionalVehicle(_) => "Conv".into(),
                PowertrainType::HybridElectricVehicle(_) => "HEV".into(),
                PowertrainType::PlugInHybridElectricVehicle(_) => "PHEV".into(),
                PowertrainType::BatteryElectricVehicle(_) => "BEV".into(),
            },
            veh_year: self
                .year
                .as_ref()
                .map(|y| y.parse::<u32>())
                .transpose()?
                .with_context(|| anyhow!(format_dbg!()))?,
            wheel_base_m: self.chassis.wheel_base.get::<si::meter>(),
            wheel_base_m_doc: None,
            wheel_coef_of_fric: self.chassis.wheel_fric_coef.get::<si::ratio>(),
            wheel_coef_of_fric_doc: None,
            wheel_inertia_kg_m2: self
                .chassis
                .wheel_inertia
                .get::<si::kilogram_square_meter>(),
            wheel_inertia_kg_m2_doc: None,
            wheel_radius_m: self.chassis.wheel_radius.unwrap().get::<si::meter>(),
            wheel_radius_m_doc: None,
            wheel_rr_coef: self.chassis.wheel_rr_coef.get::<si::ratio>(),
            wheel_rr_coef_doc: None,
        };
        veh.set_derived().with_context(|| anyhow!(format_dbg!()))?;
        Ok(veh)
    }
}

impl Cycle {
    pub fn to_fastsim2(&self) -> anyhow::Result<fastsim_core::cycle::RustCycle> {
        let cyc2 = fastsim_core::cycle::RustCycle {
            name: self.name.clone(),
            time_s: self.time.iter().map(|t| t.get::<si::second>()).collect(),
            mps: self
                .speed
                .iter()
                .map(|s| s.get::<si::meter_per_second>())
                .collect(),
            grade: self.grade.iter().map(|g| g.get::<si::ratio>()).collect(),
            orphaned: false,
            road_type: vec![0.; self.len_checked().with_context(|| format_dbg!())?].into(),
        };

        Ok(cyc2)
    }
}

impl TryFrom<fastsim_core::vehicle::RustVehicle> for Vehicle {
    type Error = anyhow::Error;
    fn try_from(f2veh: fastsim_core::vehicle::RustVehicle) -> anyhow::Result<Self> {
        let mut f2veh = f2veh.clone();
        f2veh
            .set_derived()
            .with_context(|| anyhow!(format_dbg!()))?;
        let save_interval = Some(1);
        let pt_type = PowertrainType::try_from(&f2veh).with_context(|| anyhow!(format_dbg!()))?;

        Ok(Vehicle::new(
            f2veh.scenario_name.clone(),
            None,
            None,
            f2veh.doc.clone(),
            if f2veh.veh_year == 0 {
                None
            } else {
                Some(f2veh.veh_year.to_string())
            },
            None,
            None,
            None,
            pt_type,
            Chassis::try_from(&f2veh).with_context(|| format_dbg!())?,
            Default::default(),
            Default::default(),
            Some(f2veh.veh_kg * uc::KG),
            f2veh.aux_kw * uc::KW,
            save_interval,
        )?)
    }
}

impl TryFrom<&fastsim_core::vehicle::RustVehicle> for PowertrainType {
    type Error = anyhow::Error;
    /// Returns fastsim-3 vehicle given fastsim-2 vehicle
    ///
    /// # Arguments
    /// * `f2veh` - fastsim-2 vehicle
    fn try_from(f2veh: &fastsim_core::vehicle::RustVehicle) -> anyhow::Result<PowertrainType> {
        match f2veh.veh_pt_type.as_str() {
            fastsim_core::vehicle::CONV => {
                let conv = ConventionalVehicle::try_from(f2veh)?;
                Ok(PowertrainType::ConventionalVehicle(Box::new(conv)))
            }
            fastsim_core::vehicle::HEV => {
                let hev = HybridElectricVehicle::try_from(f2veh)?;
                Ok(PowertrainType::HybridElectricVehicle(Box::new(hev)))
            }
            fastsim_core::vehicle::PHEV => {
                let phev = HybridElectricVehicle::try_from(f2veh)?;
                Ok(PowertrainType::PlugInHybridElectricVehicle(Box::new(phev)))
            }
            fastsim_core::vehicle::BEV => {
                let bev = BatteryElectricVehicle::try_from(f2veh)?;
                Ok(PowertrainType::BatteryElectricVehicle(Box::new(bev)))
            }
            _ => {
                bail!(
                    "Invalid powertrain type: {}.
Expected one of {}",
                    f2veh.veh_pt_type,
                    [
                        fastsim_core::vehicle::CONV,
                        fastsim_core::vehicle::HEV,
                        fastsim_core::vehicle::PHEV,
                        fastsim_core::vehicle::BEV
                    ]
                    .join(", "),
                )
            }
        }
    }
}

impl Vehicle {
    #[allow(dead_code)]
    #[deprecated(
        since = "3.1.0",
        note = "Use Vehicle::from_reader / from_yaml / from_json / from_toml with the compat feature enabled instead"
    )]
    pub fn from_f2_file(file: PathBuf) -> anyhow::Result<Self> {
        let f2veh = fastsim_core::vehicle::RustVehicle::from_file(file, false)
            .with_context(|| format_dbg!())?;
        Self::try_from(f2veh)
    }
}

pub const FUEL_LHV_MJ_PER_KG: f64 = 43.2;

impl TryFrom<&fastsim_core::vehicle::RustVehicle> for ConventionalVehicle {
    type Error = anyhow::Error;
    fn try_from(f2veh: &fastsim_core::vehicle::RustVehicle) -> anyhow::Result<ConventionalVehicle> {
        let conv = ConventionalVehicle {
            fs: {
                let fs = FuelStorage {
                    pwr_out_max: f2veh.fs_max_kw * uc::KW,
                    pwr_ramp_lag: f2veh.fs_secs_to_peak_pwr * uc::S,
                    fuel_type: None,
                    energy_capacity: f2veh.fs_kwh * uc::KWH,
                    specific_energy: Some(FUEL_LHV_MJ_PER_KG * uc::MJ / uc::KG),
                    mass: None,
                };
                fs
            },
            fc: FuelConverter::try_from(f2veh.clone())?,
            transmission: Transmission::try_from(f2veh.clone())?,
            pt_cntrl: ConvPowertrainControls::Normal,
            dfco_cntrl: DfcoControls::default(),
            mass: None,
            alt_eff: f2veh.alt_eff * uc::R,
        };
        Ok(conv)
    }
}

impl TryFrom<fastsim_core::vehicle::RustVehicle> for Transmission {
    type Error = anyhow::Error;
    fn try_from(f2veh: fastsim_core::vehicle::RustVehicle) -> anyhow::Result<Transmission> {
        let transmission = Transmission {
            mass: None,
            eff_interp: InterpolatorEnum::new_0d(f2veh.trans_eff),
            state: Default::default(),
            history: Default::default(),
            save_interval: Some(1),
        };
        Ok(transmission)
    }
}

impl TryFrom<&fastsim_core::vehicle::RustVehicle> for BatteryElectricVehicle {
    type Error = anyhow::Error;
    fn try_from(
        f2veh: &fastsim_core::vehicle::RustVehicle,
    ) -> anyhow::Result<BatteryElectricVehicle> {
        let bev = BatteryElectricVehicle {
            res: ReversibleEnergyStorage::try_from(f2veh.clone()).with_context(|| format_dbg!())?,
            em: ElectricMachine {
                state: Default::default(),
                eff_interp: EMEfficiency::PwrOutFrac(Interp1D::new(
                    f2veh.mc_pwr_out_perc.to_vec().into(),
                    f2veh.mc_eff_array.to_vec().into(),
                    strategy::Linear,
                    Extrapolate::Error,
                )?),
                _eff_interp_at_max_input_legacy: None,
                pwr_out_max: f2veh.mc_max_kw * uc::KW,
                specific_pwr: None,
                mass: None,
                save_interval: Some(1),
                history: Default::default(),
            },
            transmission: Transmission::try_from(f2veh.clone())?,
            mass: None,
        };
        Ok(bev)
    }
}

impl TryFrom<&fastsim_core::vehicle::RustVehicle> for Chassis {
    type Error = anyhow::Error;
    fn try_from(f2veh: &fastsim_core::vehicle::RustVehicle) -> anyhow::Result<Self> {
        let drive_type = if f2veh.drive_axle_weight_frac > 0.9 {
            chassis::DriveTypes::AWD
        } else if f2veh.veh_cg_m < 0. {
            chassis::DriveTypes::RWD
        } else {
            chassis::DriveTypes::FWD
        };

        Ok(Self {
            drag_coef: f2veh.drag_coef * uc::R,
            frontal_area: f2veh.frontal_area_m2 * uc::M2,
            cg_height: f2veh.veh_cg_m.abs() * uc::M,
            wheel_fric_coef: f2veh.wheel_coef_of_fric * uc::R,
            drive_type,
            drive_axle_weight_frac: f2veh.drive_axle_weight_frac * uc::R,
            wheel_base: f2veh.wheel_base_m * uc::M,
            wheel_inertia: f2veh.wheel_inertia_kg_m2 * uc::KGM2,
            wheel_rr_coef: f2veh.wheel_rr_coef * uc::R,
            num_wheels: f2veh.num_wheels as u8,
            wheel_radius: Some(f2veh.wheel_radius_m * uc::M),
            tire_code: None,
            mass: None,
            glider_mass: Some(f2veh.glider_kg * uc::KG),
            cargo_mass: Some(f2veh.cargo_kg * uc::KG),
        })
    }
}

impl TryFrom<&fastsim_core::vehicle::RustVehicle> for HybridElectricVehicle {
    type Error = anyhow::Error;
    fn try_from(
        f2veh: &fastsim_core::vehicle::RustVehicle,
    ) -> anyhow::Result<HybridElectricVehicle> {
        let pt_cntrl = HEVPowertrainControls::RGWDB(Box::new(hev::RESGreedyWithDynamicBuffers {
            speed_soc_fc_on_buffer: None,
            speed_soc_fc_on_buffer_coeff: None,
            speed_soc_disch_buffer: None,
            speed_soc_disch_buffer_coeff: None,
            speed_soc_regen_buffer: None,
            speed_soc_regen_buffer_coeff: None,
            // note that this exists in `fastsim-2` but has no apparent effect!
            fc_min_time_on: None,
            speed_fc_forced_on: Some(f2veh.mph_fc_on * uc::MPH),
            frac_pwr_demand_fc_forced_on: Some(
                f2veh.kw_demand_fc_on / (f2veh.fc_max_kw + f2veh.ess_max_kw.min(f2veh.mc_max_kw))
                    * uc::R,
            ),
            frac_of_most_eff_pwr_to_run_fc: None,
            temp_fc_forced_on: None,
            temp_fc_allowed_off: None,
            save_interval: Some(1),
            state: Default::default(),
            history: Default::default(),
        }));
        let mut hev = HybridElectricVehicle {
            fs: FuelStorage {
                pwr_out_max: f2veh.fs_max_kw * uc::KW,
                pwr_ramp_lag: f2veh.fs_secs_to_peak_pwr * uc::S,
                fuel_type: None,
                energy_capacity: f2veh.fs_kwh * 3.6 * uc::MJ,
                specific_energy: None,
                mass: None,
            },
            fc: FuelConverter::try_from(f2veh.clone())?,
            res: ReversibleEnergyStorage::try_from(f2veh.clone()).with_context(|| format_dbg!())?,
            em: ElectricMachine::try_from(f2veh.clone())?,
            transmission: Transmission::try_from(f2veh.clone())?,
            pt_cntrl,
            mass: None,
            sim_params: Default::default(),
            aux_cntrl: Default::default(),
            soc_bal_iter_history: Default::default(),
            soc_bal_iters: Default::default(),
        };
        hev.init()?;
        Ok(hev)
    }
}

impl TryFrom<fastsim_core::vehicle::RustVehicle> for ElectricMachine {
    type Error = anyhow::Error;
    fn try_from(
        f2veh: fastsim_core::vehicle::RustVehicle,
    ) -> Result<ElectricMachine, anyhow::Error> {
        Ok(powertrain::electric_machine::EMBuilder {
            eff_interp: {
                // fastsim-2's hard-coded short vector of percent of peak power
                let short_perc_out_vec =
                    vec![0.0, 0.02, 0.04, 0.06, 0.08, 0.1, 0.2, 0.4, 0.6, 0.8, 1.0];
                Interp1D::new(
                    short_perc_out_vec.clone().into(),
                    {
                        // convert 101 element f2 array to shorter f2 array and use
                        // linear rather than left-nearest interpolation
                        let mc_full_eff = Array1::from_vec(f2veh.mc_full_eff_array.clone());
                        ensure!(mc_full_eff.len() == 101);
                        let shortener = Interp1D::new(
                            fastsim_core::params::MC_PERC_OUT_ARRAY.to_vec().into(),
                            mc_full_eff,
                            strategy::Linear,
                            Extrapolate::Error,
                        )
                        .with_context(|| format_dbg!())?;
                        let mut short_eff: Vec<f64> = short_perc_out_vec
                            .iter()
                            .map(|x| shortener.interpolate(&[*x]).unwrap())
                            .collect();
                        short_eff[0] = short_eff[1];
                        short_eff.into()
                    },
                    strategy::Linear,
                    Extrapolate::Error,
                )
                .map(EMEfficiency::PwrOutFrac)
            }
            .with_context(|| {
                format!(
                    "{}\n{}",
                    format_dbg!(f2veh.mc_full_eff_array.len()),
                    format_dbg!(f2veh.mc_perc_out_array.len())
                )
            })?,
            pwr_out_max: f2veh.mc_max_kw * uc::KW,
        }
        .try_into()
        .with_context(|| format_dbg!())?)
    }
}

impl TryFrom<fastsim_core::vehicle::RustVehicle> for FuelConverter {
    type Error = anyhow::Error;
    fn try_from(f2veh: fastsim_core::vehicle::RustVehicle) -> Result<FuelConverter, anyhow::Error> {
        let mut fc: FuelConverter = powertrain::fuel_converter::FCBuilder {
            pwr_out_max: f2veh.fc_max_kw * uc::KW,
            pwr_ramp_lag: f2veh.fc_sec_to_peak_pwr * uc::S,
            eff_interp_from_pwr_out: InterpolatorEnum::new_1d(
                // hard-coded vec from fastsim-2
                vec![
                    0.0, 0.005, 0.015, 0.04, 0.06, 0.1, 0.14, 0.2, 0.4, 0.6, 0.8, 1.0,
                ]
                .into(),
                f2veh.fc_eff_map.to_vec().into(),
                strategy::Linear,
                Extrapolate::Error,
            )
            .with_context(|| format_dbg!())?,
            pwr_for_peak_eff: uc::KW * f64::NAN, // this gets updated in `init`
            // this means that aux power must include idle fuel
            pwr_idle_fuel: si::Power::ZERO,
            save_interval: Some(1),
        }
        .try_into()
        .with_context(|| format_dbg!())?;
        fc.init()?;
        Ok(fc)
    }
}

impl TryFrom<fastsim_core::vehicle::RustVehicle> for ReversibleEnergyStorage {
    type Error = anyhow::Error;
    fn try_from(
        f2veh: fastsim_core::vehicle::RustVehicle,
    ) -> anyhow::Result<ReversibleEnergyStorage> {
        let f3_res = ReversibleEnergyStorage {
            thrml: Default::default(),
            state: Default::default(),
            mass: None,
            specific_energy: None,
            pwr_out_max: f2veh.ess_max_kw * uc::KW,
            energy_capacity: f2veh.ess_max_kwh * uc::KWH,
            eff_interp: powertrain::reversible_energy_storage::RESEfficiency::Constant(
                Interp0D::new(f2veh.ess_round_trip_eff.sqrt()),
            ),
            min_soc: f2veh.min_soc * uc::R,
            max_soc: f2veh.max_soc * uc::R,
            save_interval: Some(1),
            history: Default::default(),
        };
        Ok(f3_res)
    }
}
