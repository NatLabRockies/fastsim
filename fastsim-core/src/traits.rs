use crate::error::Error;
use crate::imports::*;
pub mod serde_api;
pub use serde_api::*;

use ninterp::num_traits::{Float, Num, Zero};

pub trait Linspace {
    /// Generate linearly spaced vec
    /// # Arguments
    /// - `start` - starting point
    /// - `stop` - stopping point, inclusive
    /// - `n_elements` - number of array elements
    fn linspace(start: f64, stop: f64, n_elements: usize) -> Vec<f64> {
        let n_steps = n_elements - 1;
        let step_size = (stop - start) / n_steps as f64;
        let v_norm: Vec<f64> = (0..=n_steps)
            .collect::<Vec<usize>>()
            .iter()
            .map(|x| *x as f64)
            .collect();
        let v = v_norm.iter().map(|x| (x * step_size) + start).collect();
        v
    }
}

impl Linspace for Vec<f64> {}

pub trait Min<T: PartialOrd> {
    fn min(&self) -> anyhow::Result<&T>;
}
impl<T: PartialOrd> Min<T> for [T] {
    fn min(&self) -> anyhow::Result<&T> {
        self.iter()
            .min_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal))
            .ok_or_else(|| anyhow!("Empty slice has no minimum"))
    }
}
impl<T: PartialOrd> Min<T> for Vec<T> {
    fn min(&self) -> anyhow::Result<&T> {
        self.as_slice().min()
    }
}
impl<S, D> Min<S::Elem> for ArrayBase<S, D>
where
    S: ndarray::Data,
    S::Elem: PartialOrd,
    D: ndarray::Dimension,
{
    fn min(&self) -> anyhow::Result<&S::Elem> {
        self.iter()
            .min_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal))
            .ok_or_else(|| anyhow!("Empty slice has no minimum"))
    }
}
impl<T> Min<T> for Interp0D<T>
where
    T: PartialOrd,
{
    fn min(&self) -> anyhow::Result<&T> {
        Ok(&self.0)
    }
}
impl<D, S> Min<D::Elem> for Interp1DBase<D, S>
where
    D: ndarray::Data + ndarray::RawDataClone + Clone,
    D::Elem: PartialOrd + std::fmt::Debug,
    S: strategy::traits::Strategy1D<D> + Clone,
{
    fn min(&self) -> anyhow::Result<&D::Elem> {
        self.data.values.min()
    }
}
impl<D, S> Min<D::Elem> for Interp2DBase<D, S>
where
    D: ndarray::Data + ndarray::RawDataClone + Clone,
    D::Elem: PartialOrd + std::fmt::Debug,
    S: strategy::traits::Strategy2D<D> + Clone,
{
    fn min(&self) -> anyhow::Result<&D::Elem> {
        self.data.values.min()
    }
}
impl<D, S> Min<D::Elem> for Interp3DBase<D, S>
where
    D: ndarray::Data + ndarray::RawDataClone + Clone,
    D::Elem: PartialOrd + std::fmt::Debug,
    S: strategy::traits::Strategy3D<D> + Clone,
{
    fn min(&self) -> anyhow::Result<&D::Elem> {
        self.data.values.min()
    }
}
impl<D, S> Min<D::Elem> for InterpNDBase<D, S>
where
    D: ndarray::Data + ndarray::RawDataClone + Clone,
    D::Elem: PartialOrd + std::fmt::Debug,
    S: strategy::traits::StrategyND<D> + Clone,
{
    fn min(&self) -> anyhow::Result<&D::Elem> {
        self.data.values.min()
    }
}
impl<S> Min<S::Elem> for InterpolatorEnumBase<S>
where
    S: ndarray::Data + ndarray::RawDataClone + Clone,
    S::Elem: Num + Float + PartialOrd + Copy + std::fmt::Debug,
{
    fn min(&self) -> anyhow::Result<&S::Elem> {
        match self {
            Self::Interp0D(interp) => interp.min(),
            Self::Interp1D(interp) => interp.min(),
            Self::Interp2D(interp) => interp.min(),
            Self::Interp3D(interp) => interp.min(),
            Self::InterpND(interp) => interp.min(),
        }
    }
}

pub trait Max<T: PartialOrd> {
    fn max(&self) -> anyhow::Result<&T>;
}
impl<T: PartialOrd> Max<T> for [T] {
    fn max(&self) -> anyhow::Result<&T> {
        self.iter()
            .max_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal))
            .ok_or_else(|| anyhow!("Empty slice has no maximum"))
    }
}
impl<T: PartialOrd> Max<T> for Vec<T> {
    fn max(&self) -> anyhow::Result<&T> {
        self.as_slice().max()
    }
}
impl<S, D> Max<S::Elem> for ArrayBase<S, D>
where
    S: ndarray::Data,
    S::Elem: PartialOrd,
    D: ndarray::Dimension,
{
    fn max(&self) -> anyhow::Result<&S::Elem> {
        self.iter()
            .max_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal))
            .ok_or_else(|| anyhow!("Empty slice has no maximum"))
    }
}
impl<T> Max<T> for Interp0D<T>
where
    T: PartialOrd,
{
    fn max(&self) -> anyhow::Result<&T> {
        Ok(&self.0)
    }
}
impl<D, S> Max<D::Elem> for Interp1DBase<D, S>
where
    D: ndarray::Data + ndarray::RawDataClone + Clone,
    D::Elem: PartialOrd + std::fmt::Debug,
    S: strategy::traits::Strategy1D<D> + Clone,
{
    fn max(&self) -> anyhow::Result<&D::Elem> {
        self.data.values.max()
    }
}
impl<D, S> Max<D::Elem> for Interp2DBase<D, S>
where
    D: ndarray::Data + ndarray::RawDataClone + Clone,
    D::Elem: PartialOrd + std::fmt::Debug,
    S: strategy::traits::Strategy2D<D> + Clone,
{
    fn max(&self) -> anyhow::Result<&D::Elem> {
        self.data.values.max()
    }
}
impl<D, S> Max<D::Elem> for Interp3DBase<D, S>
where
    D: ndarray::Data + ndarray::RawDataClone + Clone,
    D::Elem: PartialOrd + std::fmt::Debug,
    S: strategy::traits::Strategy3D<D> + Clone,
{
    fn max(&self) -> anyhow::Result<&D::Elem> {
        self.data.values.max()
    }
}
impl<D, S> Max<D::Elem> for InterpNDBase<D, S>
where
    D: ndarray::Data + ndarray::RawDataClone + Clone,
    D::Elem: PartialOrd + std::fmt::Debug,
    S: strategy::traits::StrategyND<D> + Clone,
{
    fn max(&self) -> anyhow::Result<&D::Elem> {
        self.data.values.max()
    }
}
impl<S> Max<S::Elem> for InterpolatorEnumBase<S>
where
    S: ndarray::Data + ndarray::RawDataClone + Clone,
    S::Elem: Num + Float + PartialOrd + Copy + std::fmt::Debug,
{
    fn max(&self) -> anyhow::Result<&S::Elem> {
        match self {
            Self::Interp0D(interp) => interp.max(),
            Self::Interp1D(interp) => interp.max(),
            Self::Interp2D(interp) => interp.max(),
            Self::Interp3D(interp) => interp.max(),
            Self::InterpND(interp) => interp.max(),
        }
    }
}

pub trait Range<T: PartialOrd + Sub<Output = T>>: Min<T> + Max<T> {
    fn range(&self) -> anyhow::Result<T>;
}
impl<T> Range<T> for [T]
where
    Self: Min<T> + Max<T>,
    T: PartialOrd + Sub<Output = T> + Copy,
{
    fn range(&self) -> anyhow::Result<T> {
        Ok(*self.max()? - *self.min()?)
    }
}
impl<T> Range<T> for Vec<T>
where
    Self: Min<T> + Max<T>,
    T: PartialOrd + Sub<Output = T> + Copy,
{
    fn range(&self) -> anyhow::Result<T> {
        self.as_slice().range()
    }
}
impl<S, D> Range<S::Elem> for ArrayBase<S, D>
where
    S: ndarray::Data,
    S::Elem: PartialOrd + Sub<Output = S::Elem> + Copy,
    D: ndarray::Dimension,
    Self: Min<S::Elem> + Max<S::Elem>,
{
    fn range(&self) -> anyhow::Result<S::Elem> {
        Ok(*self.max()? - *self.min()?)
    }
}
impl<T> Range<T> for Interp0D<T>
where
    T: Zero + PartialOrd + Sub<Output = T>,
{
    fn range(&self) -> anyhow::Result<T> {
        Ok(T::zero())
    }
}
impl<D, S> Range<D::Elem> for Interp1DBase<D, S>
where
    D: ndarray::Data + ndarray::RawDataClone + Clone,
    D::Elem: PartialOrd + Sub<Output = D::Elem> + Copy + std::fmt::Debug,
    S: strategy::traits::Strategy1D<D> + Clone,
{
    fn range(&self) -> anyhow::Result<D::Elem> {
        self.data.values.range()
    }
}
impl<D, S> Range<D::Elem> for Interp2DBase<D, S>
where
    D: ndarray::Data + ndarray::RawDataClone + Clone,
    D::Elem: PartialOrd + Sub<Output = D::Elem> + Copy + std::fmt::Debug,
    S: strategy::traits::Strategy2D<D> + Clone,
{
    fn range(&self) -> anyhow::Result<D::Elem> {
        self.data.values.range()
    }
}
impl<D, S> Range<D::Elem> for Interp3DBase<D, S>
where
    D: ndarray::Data + ndarray::RawDataClone + Clone,
    D::Elem: PartialOrd + Sub<Output = D::Elem> + Copy + std::fmt::Debug,
    S: strategy::traits::Strategy3D<D> + Clone,
{
    fn range(&self) -> anyhow::Result<D::Elem> {
        self.data.values.range()
    }
}
impl<D, S> Range<D::Elem> for InterpNDBase<D, S>
where
    D: ndarray::Data + ndarray::RawDataClone + Clone,
    D::Elem: PartialOrd + Sub<Output = D::Elem> + Copy + std::fmt::Debug,
    S: strategy::traits::StrategyND<D> + Clone,
{
    fn range(&self) -> anyhow::Result<D::Elem> {
        self.data.values.range()
    }
}
impl<S> Range<S::Elem> for InterpolatorEnumBase<S>
where
    S: ndarray::Data + ndarray::RawDataClone + Clone,
    S::Elem: Num + Float + PartialOrd + Copy + std::fmt::Debug,
    ArrayBase<S, Ix1>: Range<S::Elem>,
{
    fn range(&self) -> anyhow::Result<S::Elem> {
        match self {
            Self::Interp0D(interp) => interp.range(),
            Self::Interp1D(interp) => interp.range(),
            Self::Interp2D(interp) => interp.range(),
            Self::Interp3D(interp) => interp.range(),
            Self::InterpND(interp) => interp.range(),
        }
    }
}

pub trait Init {
    /// Specialized code to execute upon initialization.  For any struct with fields
    /// that implement `Init`, this should propagate down the hierarchy.
    fn init(&mut self) -> Result<(), Error> {
        Ok(())
    }
}

pub trait Diff<T> {
    /// Returns vec of length `self.len() - 1` where each element in the returned vec at index i is
    /// `self[i + 1] - self[i]`
    fn diff(&self) -> Vec<T>;
}

impl<T: Clone + Sub<T, Output = T> + Default> Diff<T> for Vec<T> {
    fn diff(&self) -> Vec<T> {
        let mut v_diff: Vec<T> = vec![Default::default()];
        v_diff.extend::<Vec<T>>(
            self.windows(2)
                .map(|vs| {
                    let x = &vs[0];
                    let y = &vs[1];
                    y.clone() - x.clone()
                })
                .collect(),
        );
        v_diff
    }
}

/// Marker trait implemented for leaf state structs (i.e. structs derived with
/// `#[is_state]`, e.g. `FooState`). Used to give a compile error, rather than silent
/// drift, when a field tagged `#[is_state]` in `#[derive(StateMethods)]` /
/// `#[derive(SetCumulative)]` does not actually point at a state struct.
pub trait IsState {}

/// Super trait to ensure that related traits are implemented together
pub trait StateMethods: SetCumulative + SaveState + Step + TrackedStateMethods {}

/// Trait for setting cumulative values based on rate values
pub trait SetCumulative {
    /// Sets cumulative values based on rate values
    fn set_cumulative<F: Fn() -> String>(&mut self, dt: si::Time, loc: F) -> anyhow::Result<()>;

    /// Resets cumulative and corresponding rate values to zero
    fn reset_cumulative<F: Fn() -> String>(&mut self, loc: F) -> anyhow::Result<()>;
}

/// Provides method that saves `self.state` to `self.history` and propagates to any fields with
/// `state`
pub trait SaveState {
    /// Saves `self.state` to `self.history` and propagates to any fields with `state`
    /// # Arguments
    /// - `loc`: closure that returns file and line number where called
    fn save_state<F: Fn() -> String>(&mut self, loc: F) -> anyhow::Result<()>;
}

/// Trait that provides method for incrementing `i` field of this and all contained structs,
/// recursively
pub trait Step {
    /// Increments `i` field of this and all contained structs, recursively
    /// # Arguments
    /// - `loc`: closure that returns file and line number where called
    fn step<F: Fn() -> String>(&mut self, loc: F) -> anyhow::Result<()>;

    /// Resets `i` field of this and all contained structs, recursively
    /// # Arguments
    /// - `loc`: closure that returns file and line number where called
    fn reset_step<F: Fn() -> String>(&mut self, loc: F) -> anyhow::Result<()>;
}

/// Provides methods for getting and setting the save interval
pub trait HistoryMethods: SaveState {
    /// Recursively sets save interval
    /// # Arguments
    /// - `save_interval`: time step interval at which to save `self.state` to `self.history`
    fn set_save_interval(&mut self, save_interval: Option<usize>) -> anyhow::Result<()>;
    /// Returns save interval for `self` but does not guarantee recursive consistency in nested
    /// objects
    fn save_interval(&self) -> anyhow::Result<Option<usize>>;
    /// Remove all history
    fn clear(&mut self);
}

/// Provides method for checking if struct is default
pub trait EqDefault: std::default::Default + PartialEq {
    /// If `self` is default, returns true
    fn eq_default(&self) -> bool {
        *self == Self::default()
    }
}

impl<T: Default + PartialEq> EqDefault for T {}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_linspace() {
        assert_eq!(Vec::linspace(0., 2., 3), vec![0., 1., 2.]);
    }

    #[test]
    fn test_max_for_vec_f64() {
        assert_eq!(Vec::linspace(-10., 12., 5).max().unwrap(), &12.);
    }
    #[test]
    fn test_min_for_vec_f64() {
        assert_eq!(Vec::linspace(-10., 12., 5).min().unwrap(), &-10.);
    }

    #[test]
    fn test_diff() {
        let diff = Vec::linspace(0., 2., 3).diff();
        let ref_diff = vec![0., 1., 1.];
        assert_eq!(diff, ref_diff);
    }
}
