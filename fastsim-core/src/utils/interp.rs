use crate::imports::*;
use ninterp::error::InterpolateError;

pub(crate) trait InterpolatorScanValues {
    fn try_for_each_value<E, F: FnMut(f64) -> Result<(), E>>(&self, f: F) -> Result<(), E>;
}

impl InterpolatorScanValues for Interp0D<f64> {
    fn try_for_each_value<E, F: FnMut(f64) -> Result<(), E>>(&self, mut f: F) -> Result<(), E> {
        f(self.0)
    }
}

impl<S> InterpolatorScanValues for Interp1D<f64, S>
where
    S: ninterp::strategy::traits::Strategy1D<ndarray::OwnedRepr<f64>> + Clone,
{
    fn try_for_each_value<E, F: FnMut(f64) -> Result<(), E>>(&self, mut f: F) -> Result<(), E> {
        self.data.values.iter().copied().try_for_each(&mut f)
    }
}

impl InterpolatorScanValues for Interp2D<f64, strategy::enums::Strategy2DEnum<f64>> {
    fn try_for_each_value<E, F: FnMut(f64) -> Result<(), E>>(&self, mut f: F) -> Result<(), E> {
        self.data.values.iter().copied().try_for_each(&mut f)
    }
}

impl InterpolatorScanValues for Interp3D<f64, strategy::enums::Strategy3DEnum<f64>> {
    fn try_for_each_value<E, F: FnMut(f64) -> Result<(), E>>(&self, mut f: F) -> Result<(), E> {
        self.data.values.iter().copied().try_for_each(&mut f)
    }
}

impl InterpolatorScanValues for InterpND<f64, strategy::enums::StrategyNDEnum<f64>> {
    fn try_for_each_value<E, F: FnMut(f64) -> Result<(), E>>(&self, mut f: F) -> Result<(), E> {
        self.data.values.iter().copied().try_for_each(&mut f)
    }
}

impl InterpolatorScanValues for InterpolatorEnum<f64> {
    fn try_for_each_value<E, F: FnMut(f64) -> Result<(), E>>(&self, f: F) -> Result<(), E> {
        match self {
            Self::Interp0D(interp) => interp.try_for_each_value(f),
            Self::Interp1D(interp) => interp.try_for_each_value(f),
            Self::Interp2D(interp) => interp.try_for_each_value(f),
            Self::Interp3D(interp) => interp.try_for_each_value(f),
            Self::InterpND(interp) => interp.try_for_each_value(f),
        }
    }
}

/// Borrowed view of a `Linear`-strategy 1-D interpolant that answers, on
/// `interpolate`, the inverse of the ratio `w(x) = x / f(x)` rather than `f(x)`
/// directly: given `w`, returns `f` at the `x` satisfying `x / f(x) == w`, with
/// no second interpolator built. Get one via [RatioInverse::ratio_inverse].
///
/// `f` is affine on each segment, so `w` is a Mobius transform of `x` there; Mobius
/// transforms invert to Mobius transforms, giving the closed form `f(w) = a / (1 - b*w)`.
pub(crate) struct RatioInverseView<'a> {
    interp: &'a Interp1D<f64, strategy::Linear>,
    extrapolate: Extrapolate<f64>,
}

impl RatioInverseView<'_> {
    pub fn interpolate(&self, point: &[f64]) -> Result<f64, InterpolateError> {
        let w = *point.first().ok_or_else(|| {
            InterpolateError::Other("`RatioInverseView::interpolate` needs one point".into())
        })?;

        let x = &self.interp.data.grid[0];
        let y = &self.interp.data.values;

        let w_grid: Vec<f64> = x.iter().zip(y).map(|(xi, yi)| xi / yi).collect();
        let w_min = *w_grid
            .first()
            .ok_or_else(|| InterpolateError::Other("`RatioInverse` grid is empty".into()))?;
        let w_max = *w_grid.last().unwrap();

        if w < w_min || w > w_max {
            match self.extrapolate {
                // falls through to the segment solve below, which naturally
                // extrapolates along the boundary segment's own slope
                Extrapolate::Enable => {}
                Extrapolate::Clamp => return Ok(if w < w_min { y[0] } else { y[y.len() - 1] }),
                Extrapolate::Fill(value) => return Ok(value),
                Extrapolate::Error => {
                    return Err(InterpolateError::Other(
                        format!("`RatioInverse` query w={w} outside domain [{w_min}, {w_max}]")
                            .into(),
                    ))
                }
                Extrapolate::Wrap => {
                    return Err(InterpolateError::Other(
                        "`RatioInverse` does not support `Extrapolate::Wrap`".into(),
                    ))
                }
                // `Extrapolate` is `#[non_exhaustive]`
                _ => {
                    return Err(InterpolateError::Other(
                        "`RatioInverse` does not support this `Extrapolate` variant".into(),
                    ))
                }
            }
        }

        let lower =
            strategy::utils::locate_lower_index(ndarray::ArrayView1::from(w_grid.as_slice()), &w);
        let (x0, y0, x1, y1) = (x[lower], y[lower], x[lower + 1], y[lower + 1]);
        let b = (y1 - y0) / (x1 - x0); // f(x) = a + b*x on this segment
        let a = y0 - b * x0;

        let denom = 1.0 - b * w;
        if denom == 0.0 {
            return Err(InterpolateError::Other(
                format!("`RatioInverse` singular at w={w} (segment {lower})").into(),
            ));
        }
        Ok(a / denom)
    }
}

pub(crate) trait RatioInverse<T> {
    /// Borrows `self` as a [RatioInverseView], so `.interpolate(&[w])` answers
    /// the ratio-inverse query instead of the usual one.
    fn ratio_inverse(&self, extrapolate: Extrapolate<T>) -> RatioInverseView<'_>;
}

impl RatioInverse<f64> for Interp1D<f64, strategy::Linear> {
    fn ratio_inverse(&self, extrapolate: Extrapolate<f64>) -> RatioInverseView<'_> {
        RatioInverseView {
            interp: self,
            extrapolate,
        }
    }
}

/// Methods for mutating interpolator data, e.g. proportionally scaling
/// interpolator function data
pub trait InterpolatorMutMethods {
    fn set_min(&mut self, min: f64, scaling: Option<ScalingMethods>) -> anyhow::Result<()>;
    fn set_max(&mut self, max: f64, scaling: Option<ScalingMethods>) -> anyhow::Result<()>;
    fn set_range(&mut self, range: f64) -> anyhow::Result<()>;
}

impl InterpolatorMutMethods for Interp0D<f64> {
    fn set_min(&mut self, min: f64, _scaling: Option<ScalingMethods>) -> anyhow::Result<()> {
        self.0 = min;
        Ok(())
    }

    fn set_max(&mut self, max: f64, _scaling: Option<ScalingMethods>) -> anyhow::Result<()> {
        self.0 = max;
        Ok(())
    }

    fn set_range(&mut self, _range: f64) -> anyhow::Result<()> {
        bail!("Cannot set range for 0D interpolator")
    }
}

impl<S> InterpolatorMutMethods for Interp1D<f64, S>
where
    S: ninterp::strategy::traits::Strategy1D<ndarray::OwnedRepr<f64>> + Clone,
{
    fn set_min(&mut self, min: f64, scaling: Option<ScalingMethods>) -> anyhow::Result<()> {
        let old_min = *self.min()?;
        match scaling.unwrap_or_default() {
            utils::interp::ScalingMethods::Proportional => {
                ensure!(
                    old_min != 0.,
                    "Cannot modify min proportionally when old_min == 0."
                );
                self.data.values.map_inplace(|v| *v *= min / old_min);
            }
            utils::interp::ScalingMethods::AnchoredProportional => {
                todo!()
            }
            utils::interp::ScalingMethods::Offset => {
                todo!()
            }
        }
        self.validate()?;
        Ok(())
    }

    fn set_max(&mut self, max: f64, scaling: Option<ScalingMethods>) -> anyhow::Result<()> {
        let old_max = *self.max()?;
        match scaling.unwrap_or_default() {
            utils::interp::ScalingMethods::Proportional => {
                ensure!(
                    old_max != 0.,
                    "Cannot modify max proportionally when old_max == 0."
                );
                self.data.values.map_inplace(|v| *v *= max / old_max);
            }
            utils::interp::ScalingMethods::AnchoredProportional => {
                todo!()
            }
            utils::interp::ScalingMethods::Offset => {
                todo!()
            }
        }
        self.validate()?;
        Ok(())
    }

    fn set_range(&mut self, range: f64) -> anyhow::Result<()> {
        let old_max = *self.max()?;
        let old_range = old_max - self.min()?;
        ensure!(old_range != 0., "Cannot modify range when min == max");
        // if the new range is 0., chooses the max as the value for all elements of the array
        if range == 0. {
            self.data.values = self.data.values.map(|_| old_max);
        } else {
            self.data.values = self
                .data
                .values
                .map(|x| old_max + (x - old_max) * range / old_range);
        }
        self.validate()?;
        Ok(())
    }
}

impl<S> InterpolatorMutMethods for Interp2D<f64, S>
where
    S: ninterp::strategy::traits::Strategy2D<ndarray::OwnedRepr<f64>> + Clone,
{
    fn set_min(&mut self, min: f64, scaling: Option<ScalingMethods>) -> anyhow::Result<()> {
        let old_min = *self.min()?;
        match scaling.unwrap_or_default() {
            utils::interp::ScalingMethods::Proportional => {
                ensure!(
                    old_min != 0.,
                    "Cannot modify min proportionally when old_min == 0."
                );
                self.data.values.map_inplace(|v| *v *= min / old_min);
            }
            utils::interp::ScalingMethods::AnchoredProportional => {
                todo!()
            }
            utils::interp::ScalingMethods::Offset => {
                todo!()
            }
        }
        self.validate()?;
        Ok(())
    }

    fn set_max(&mut self, max: f64, scaling: Option<ScalingMethods>) -> anyhow::Result<()> {
        let old_max = *self.max()?;
        match scaling.unwrap_or_default() {
            utils::interp::ScalingMethods::Proportional => {
                ensure!(
                    old_max != 0.,
                    "Cannot modify max proportionally when old_max == 0."
                );
                self.data.values.map_inplace(|v| *v *= max / old_max);
            }
            utils::interp::ScalingMethods::AnchoredProportional => {
                todo!()
            }
            utils::interp::ScalingMethods::Offset => {
                todo!()
            }
        }
        self.validate()?;
        Ok(())
    }

    fn set_range(&mut self, range: f64) -> anyhow::Result<()> {
        let old_max = *self.max()?;
        let old_range = old_max - self.min()?;
        ensure!(old_range != 0., "Cannot modify range when min == max");
        // if the new range is 0., chooses the max as the value for all elements of the array
        if range == 0. {
            self.data.values = self.data.values.map(|_| old_max);
        } else {
            self.data.values = self
                .data
                .values
                .map(|x| old_max + (x - old_max) * range / old_range);
        }
        self.validate()?;
        Ok(())
    }
}

impl<S> InterpolatorMutMethods for Interp3D<f64, S>
where
    S: ninterp::strategy::traits::Strategy3D<ndarray::OwnedRepr<f64>> + Clone,
{
    fn set_min(&mut self, min: f64, scaling: Option<ScalingMethods>) -> anyhow::Result<()> {
        let old_min = *self.min()?;
        match scaling.unwrap_or_default() {
            utils::interp::ScalingMethods::Proportional => {
                ensure!(
                    old_min != 0.,
                    "Cannot modify min proportionally when old_min == 0."
                );
                self.data.values.map_inplace(|v| *v *= min / old_min);
            }
            utils::interp::ScalingMethods::AnchoredProportional => {
                todo!()
            }
            utils::interp::ScalingMethods::Offset => {
                todo!()
            }
        }
        self.validate()?;
        Ok(())
    }

    fn set_max(&mut self, max: f64, scaling: Option<ScalingMethods>) -> anyhow::Result<()> {
        let old_max = *self.max()?;
        match scaling.unwrap_or_default() {
            utils::interp::ScalingMethods::Proportional => {
                ensure!(
                    old_max != 0.,
                    "Cannot modify max proportionally when old_max == 0."
                );
                self.data.values.map_inplace(|v| *v *= max / old_max);
            }
            utils::interp::ScalingMethods::AnchoredProportional => {
                todo!()
            }
            utils::interp::ScalingMethods::Offset => {
                todo!()
            }
        }
        self.validate()?;
        Ok(())
    }

    fn set_range(&mut self, range: f64) -> anyhow::Result<()> {
        let old_max = *self.max()?;
        let old_range = old_max - self.min()?;
        ensure!(old_range != 0., "Cannot modify range when min == max");
        // if the new range is 0., chooses the max as the value for all elements of the array
        if range == 0. {
            self.data.values = self.data.values.map(|_| old_max);
        } else {
            self.data.values = self
                .data
                .values
                .map(|x| old_max + (x - old_max) * range / old_range);
        }
        self.validate()?;
        Ok(())
    }
}

impl<S> InterpolatorMutMethods for InterpND<f64, S>
where
    S: ninterp::strategy::traits::StrategyND<ndarray::OwnedRepr<f64>> + Clone,
{
    fn set_min(&mut self, min: f64, scaling: Option<ScalingMethods>) -> anyhow::Result<()> {
        let old_min = *self.min()?;
        match scaling.unwrap_or_default() {
            utils::interp::ScalingMethods::Proportional => {
                ensure!(
                    old_min != 0.,
                    "Cannot modify min proportionally when old_min == 0."
                );
                self.data.values.map_inplace(|v| *v *= min / old_min);
            }
            utils::interp::ScalingMethods::AnchoredProportional => {
                todo!()
            }
            utils::interp::ScalingMethods::Offset => {
                todo!()
            }
        }
        self.validate()?;
        Ok(())
    }

    fn set_max(&mut self, max: f64, scaling: Option<ScalingMethods>) -> anyhow::Result<()> {
        let old_max = *self.max()?;
        match scaling.unwrap_or_default() {
            utils::interp::ScalingMethods::Proportional => {
                ensure!(
                    old_max != 0.,
                    "Cannot modify max proportionally when old_max == 0."
                );
                self.data.values.map_inplace(|v| *v *= max / old_max);
            }
            utils::interp::ScalingMethods::AnchoredProportional => {
                todo!()
            }
            utils::interp::ScalingMethods::Offset => {
                todo!()
            }
        }
        self.validate()?;
        Ok(())
    }

    fn set_range(&mut self, range: f64) -> anyhow::Result<()> {
        let old_max = *self.max()?;
        let old_range = old_max - self.min()?;
        ensure!(old_range != 0., "Cannot modify range when min == max");
        // if the new range is 0., chooses the max as the value for all elements of the array
        if range == 0. {
            self.data.values = self.data.values.map(|_| old_max);
        } else {
            self.data.values = self
                .data
                .values
                .map(|x| old_max + (x - old_max) * range / old_range);
        }
        self.validate()?;
        Ok(())
    }
}

// This can be made more generic by using a `ninterp::num_traits` bound instead of f64
// If there are future methods that *do not* mutate the interpolator,
// we should define a new trait and impl it for `InterpolatorEnum<D> where D: ndarray::Data`
impl InterpolatorMutMethods for InterpolatorEnum<f64> {
    // scale all values so that the min is the new min
    // (Note: this may change the max, depending on what scaling method is chosen)
    fn set_min(&mut self, min: f64, scaling: Option<ScalingMethods>) -> anyhow::Result<()> {
        match self {
            Self::Interp0D(interp) => interp.set_min(min, scaling),
            Self::Interp1D(interp) => interp.set_min(min, scaling),
            Self::Interp2D(interp) => interp.set_min(min, scaling),
            Self::Interp3D(interp) => interp.set_min(min, scaling),
            Self::InterpND(interp) => interp.set_min(min, scaling),
        }
    }

    // scale all values so that the max is the new max
    // (Note: may change the min, depending on what scaling method is chosen)
    fn set_max(&mut self, max: f64, scaling: Option<ScalingMethods>) -> anyhow::Result<()> {
        match self {
            Self::Interp0D(interp) => interp.set_max(max, scaling),
            Self::Interp1D(interp) => interp.set_max(max, scaling),
            Self::Interp2D(interp) => interp.set_max(max, scaling),
            Self::Interp3D(interp) => interp.set_max(max, scaling),
            Self::InterpND(interp) => interp.set_max(max, scaling),
        }
    }

    fn set_range(&mut self, range: f64) -> anyhow::Result<()> {
        match self {
            Self::Interp0D(interp) => interp.set_range(range),
            Self::Interp1D(interp) => interp.set_range(range),
            Self::Interp2D(interp) => interp.set_range(range),
            Self::Interp3D(interp) => interp.set_range(range),
            Self::InterpND(interp) => interp.set_range(range),
        }
    }
}

impl<D> Init for InterpolatorEnumBase<D>
where
    D: ndarray::Data + ndarray::RawDataClone + Clone,
    D::Elem: ninterp::num_traits::Float + ninterp::num_traits::Euclid + std::fmt::Debug,
{
    fn init(&mut self) -> Result<(), Error> {
        self.validate()
            .map_err(|e| Error::NinterpError(e.to_string()))
    }
}
impl<D> SerdeAPI for InterpolatorEnumBase<D>
where
    D: ndarray::Data + ndarray::RawDataClone + Clone + ndarray::DataOwned,
    D::Elem: ninterp::num_traits::Float
        + ninterp::num_traits::Euclid
        + std::fmt::Debug
        + Serialize
        + serde::de::DeserializeOwned,
{
    #[cfg(feature = "resources")]
    const RESOURCES_SUBDIR: &'static str = "interpolators";
}

#[derive(Default, Debug, Serialize, Deserialize, Clone)]
pub enum ScalingMethods {
    #[default]
    /// Scales everything by the same factor -- e.g. setting min of [1, 2, 3] to 0.5 yields [0.5, 1, 1.5]
    Proportional,
    /// Scales proportionally to distance from min/max -- e.g. setting min of [1, 2, 3] to 0.5 yields [0.5, 1.5, 3]
    AnchoredProportional,
    /// Scaling by sliding all values up or down by the same offset -- e.g. setting min of [1, 2, 3] to 0.5 yields [0.5, 1.5, 2.5]
    Offset,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_min() {
        let x = array![0.05, 0.10, 0.15];
        let y = array![0.10, 0.20, 0.30];
        let z = array![0.20, 0.40, 0.60];
        let f_xy = array![[0.1, 1., 2.], [3., 4., 5.], [6., 7., 8.]];
        let f_xyz = array![
            [[0.1, 1., 2.], [3., 4., 5.], [6., 7., 8.]],
            [[9., 10., 11.], [12., 13., 14.], [15., 16., 17.]],
            [[18., 19., 20.], [21., 22., 23.], [24., 25., 26.],],
        ];
        let mut interp_1d = InterpolatorEnum::new_1d(
            array![85.0, 90.0],
            array![0.2, 1.0],
            strategy::Linear,
            Extrapolate::Clamp,
        )
        .unwrap();
        let mut interp_2d = InterpolatorEnum::new_2d(
            x.clone(),
            y.clone(),
            f_xy.clone(),
            strategy::Linear,
            Extrapolate::Clamp,
        )
        .unwrap();
        let mut interp_3d = InterpolatorEnum::new_3d(
            x.clone(),
            y.clone(),
            z.clone(),
            f_xyz.clone(),
            strategy::Linear,
            Extrapolate::Clamp,
        )
        .unwrap();
        assert_eq!(interp_1d.min().unwrap(), &0.2);
        assert_eq!(interp_2d.min().unwrap(), &0.1);
        assert_eq!(interp_3d.min().unwrap(), &0.1);
        interp_1d.set_min(0.1, None).unwrap();
        interp_2d.set_min(0.3, None).unwrap();
        interp_3d.set_min(0.3, None).unwrap();
        println!("{:?}", interp_1d.min().unwrap());
        println!("{:?}", interp_2d.min().unwrap());
        println!("{:?}", interp_3d.min().unwrap());
        assert!(almost_eq(*interp_1d.min().unwrap(), 0.1, Some(1e-3)));
        assert!(almost_eq(*interp_2d.min().unwrap(), 0.3, Some(1e-3)));
        assert!(almost_eq(*interp_3d.min().unwrap(), 0.3, Some(1e-3)));
    }

    #[test]
    fn test_max() {
        let x = array![0.05, 0.10, 0.15];
        let y = array![0.10, 0.20, 0.30];
        let z = array![0.20, 0.40, 0.60];
        let f_xy = array![[0., 1., 2.], [3., 4., 5.], [6., 7., 8.]];
        let f_xyz = array![
            [[0., 1., 2.], [3., 4., 5.], [6., 7., 8.]],
            [[9., 10., 11.], [12., 13., 14.], [15., 16., 17.]],
            [[18., 19., 20.], [21., 22., 23.], [24., 25., 26.]],
        ];
        let mut interp_1d = InterpolatorEnum::new_1d(
            array![85.0, 90.0],
            array![0.2, 1.0],
            strategy::Linear,
            Extrapolate::Clamp,
        )
        .unwrap();
        let mut interp_2d = InterpolatorEnum::new_2d(
            x.clone(),
            y.clone(),
            f_xy.clone(),
            strategy::Linear,
            Extrapolate::Clamp,
        )
        .unwrap();
        let mut interp_3d = InterpolatorEnum::new_3d(
            x.clone(),
            y.clone(),
            z.clone(),
            f_xyz.clone(),
            strategy::Linear,
            Extrapolate::Clamp,
        )
        .unwrap();
        assert_eq!(interp_1d.max().unwrap(), &1.0);
        assert_eq!(interp_2d.max().unwrap(), &8.);
        assert_eq!(interp_3d.max().unwrap(), &26.);
        interp_1d.set_max(2., None).unwrap();
        interp_2d.set_max(7., None).unwrap();
        interp_3d.set_max(5., None).unwrap();
        println!("{:?}", interp_1d.max().unwrap());
        println!("{:?}", interp_2d.max().unwrap());
        println!("{:?}", interp_3d.max().unwrap());
        assert!(almost_eq(*interp_1d.max().unwrap(), 2., Some(1e-3)));
        assert!(almost_eq(*interp_2d.max().unwrap(), 7., Some(1e-3)));
        assert!(almost_eq(*interp_3d.max().unwrap(), 5., Some(1e-3)));
    }

    #[test]
    fn test_range() {
        let x = array![0.05, 0.10, 0.15];
        let y = array![0.10, 0.20, 0.30];
        let z = array![0.20, 0.40, 0.60];
        let f_xy = array![[0., 1., 2.], [3., 4., 5.], [6., 7., 8.]];
        let f_xyz = array![
            [[0., 1., 2.], [3., 4., 5.], [6., 7., 8.]],
            [[9., 10., 11.], [12., 13., 14.], [15., 16., 17.]],
            [[18., 19., 20.], [21., 22., 23.], [24., 25., 26.]],
        ];
        let mut interp_1d = InterpolatorEnum::new_1d(
            array![85.0, 90.0],
            array![0.2, 1.0],
            strategy::Linear,
            Extrapolate::Clamp,
        )
        .unwrap();
        let mut interp_2d = InterpolatorEnum::new_2d(
            x.clone(),
            y.clone(),
            f_xy.clone(),
            strategy::Linear,
            Extrapolate::Clamp,
        )
        .unwrap();
        let mut interp_3d = InterpolatorEnum::new_3d(
            x.clone(),
            y.clone(),
            z.clone(),
            f_xyz.clone(),
            strategy::Linear,
            Extrapolate::Clamp,
        )
        .unwrap();
        assert_eq!(interp_1d.range().unwrap(), 0.8);
        assert_eq!(interp_2d.range().unwrap(), 8.);
        assert_eq!(interp_3d.range().unwrap(), 26.);
        interp_1d.set_range(2.).unwrap();
        interp_2d.set_range(7.).unwrap();
        interp_3d.set_range(5.).unwrap();
        println!("{:?}", interp_1d.range().unwrap());
        println!("{:?}", interp_2d.range().unwrap());
        println!("{:?}", interp_3d.range().unwrap());
        assert!(almost_eq(interp_1d.range().unwrap(), 2., Some(1e-3)));
        assert!(almost_eq(interp_2d.range().unwrap(), 7., Some(1e-3)));
        assert!(almost_eq(interp_3d.range().unwrap(), 5., Some(1e-3)));
    }

    type StructWithResources = InterpolatorEnum<f64>;

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
}
