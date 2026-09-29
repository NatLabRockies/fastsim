// TODO: uncomment when docs are somewhat mature to check for missing docs
// #![warn(missing_docs)]
// #![warn(missing_docs_in_private_items)]
//! Module containing submodules for vehicle and powertrain models

pub(crate) use crate::imports::*;
pub use crate::prelude::*;

pub mod bev;
pub mod cabin;
pub mod chassis;
pub mod common;
pub mod conv;
pub mod database;
pub mod hev;
pub mod hvac;
pub mod powertrain;
pub mod powertrain_type;
pub mod traits;
pub mod vehicle_model;
pub use bev::BatteryElectricVehicle;
pub use chassis::Chassis;
pub use conv::ConventionalVehicle;
pub use database::*;
pub use hev::HybridElectricVehicle;
pub use powertrain::electric_machine::{EMEfficiency, ElectricMachine};
pub use powertrain::fuel_converter::FuelConverter;
pub use powertrain::fuel_storage::FuelStorage;
pub use powertrain::fuel_storage::FuelType;
pub use powertrain::reversible_energy_storage::ReversibleEnergyStorage;
pub use powertrain::traits::Powertrain;
pub use powertrain::transmission::Transmission;
pub use powertrain_type::PowertrainType;
pub use traits::*;
pub use vehicle_model::{Vehicle, VehicleState};
