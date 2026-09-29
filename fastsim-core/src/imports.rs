//! Module for crate-local imports to reduce boilerplate in submodules

#![allow(unused_imports)]

#[cfg(feature = "pyo3")]
pub(crate) use crate::pyo3::*;

pub(crate) use crate::error::Error;
pub(crate) use crate::si;
pub(crate) use crate::simdrive::TraceMissOptions;
pub(crate) use crate::traits::*;
pub(crate) use crate::uc;
pub(crate) use crate::utils;
pub(crate) use crate::utils::interp::*;
pub(crate) use crate::utils::{
    abs_checked_x_val, almost_eq, almost_eq_uom, almost_ge_uom, almost_le_uom,
    check_interp_frac_data, check_monotonicity, is_sorted, InterpRange, DIRECT_SET_ERR,
};
pub(crate) use crate::utils::{TrackedState, TrackedStateMethods};
pub(crate) use crate::vehicle::traits::Mass;
pub(crate) use anyhow::{anyhow, bail, ensure, Context};
pub(crate) use derive_more::{FromStr, IsVariant, TryInto};
pub(crate) use duplicate::duplicate_item;
pub(crate) use easy_ext::ext;
pub(crate) use eng_fmt::FormatEng;
pub(crate) use fastsim_proc_macros::{
    pyo3_api, serde_api, HistoryVec, SetCumulative, StateMethods,
};
pub(crate) use lazy_static::lazy_static;
pub(crate) use ndarray::prelude::*;
pub(crate) use ndarray::{IxDynImpl, OwnedRepr};
pub(crate) use ninterp::ndarray;
pub(crate) use ninterp::prelude::*;
pub(crate) use serde::{Deserialize, Serialize};
pub(crate) use std::cmp::{self, Ordering};
pub(crate) use std::f64::consts::PI;
pub(crate) use std::ffi::OsStr;
pub(crate) use std::fmt;
pub(crate) use std::fs::File;
pub(crate) use std::marker::PhantomData;
pub(crate) use std::num::{NonZeroU16, NonZeroUsize};
pub(crate) use std::ops::{Deref, DerefMut, IndexMut, Sub};
pub(crate) use std::path::{Path, PathBuf};
pub(crate) use typenum::{P1, P2, P3};
pub(crate) use uom::typenum;
pub(crate) use uom::ConstZero;
