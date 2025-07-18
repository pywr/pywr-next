use crate::parameters::errors::{ParameterSetupError, SimpleCalculationError};
use crate::parameters::{
    Parameter, ParameterMeta, ParameterName, ParameterState, SimpleParameter, VariableConfig, VariableParameter,
    VariableParameterError, downcast_internal_state_mut, downcast_internal_state_ref, downcast_variable_config_ref,
};
use crate::scenario::ScenarioIndex;
use crate::state::SimpleParameterValues;
use crate::timestep::Timestep;
use nalgebra::DMatrix;

pub struct RbfProfileVariableConfig {
    days_of_year_range: Option<u32>,
    value_upper_bounds: f64,
    value_lower_bounds: f64,
}

impl RbfProfileVariableConfig {
    pub fn new(days_of_year_range: Option<u32>, value_upper_bounds: f64, value_lower_bounds: f64) -> Self {
        Self {
            days_of_year_range,
            value_upper_bounds,
            value_lower_bounds,
        }
    }

    pub fn days_of_year_range(&self) -> Option<u32> {
        self.days_of_year_range
    }

    pub fn value_lower_bounds(&self) -> f64 {
        self.value_lower_bounds
    }

    pub fn value_upper_bounds(&self) -> f64 {
        self.value_upper_bounds
    }
}

/// A parameter that interpolates between a set of points using a radial basis function to
/// create a daily profile.
pub struct RbfProfileParameter {
    meta: ParameterMeta,
    points: Vec<(u32, f64)>,
    function: RadialBasisFunction,
}

/// The internal state of the RbfProfileParameter.
///
/// This holds the interpolated profile along with any points that have been updated via the optimisation API.
#[derive(Clone)]
struct RbfProfileInternalState {
    /// The interpolated profile.
    profile: [f64; 366],
    /// Optional updated x values of the points.
    points_x: Option<Vec<u32>>,
    /// Optional updated y values of the points.
    points_y: Option<Vec<f64>>,
}

impl RbfProfileInternalState {
    fn new(points: &[(u32, f64)], function: &RadialBasisFunction) -> Self {
        let profile = interpolate_rbf_profile(points, function);

        Self {
            profile,
            points_x: None,
            points_y: None,
        }
    }

    /// Update the x values of the points.
    ///
    /// This does not update the profile.
    fn update_x(&mut self, x: Vec<u32>) {
        self.points_x = Some(x);
    }

    /// Update the y values of the points.
    ///
    /// This does not update the profile.
    fn update_y(&mut self, y: Vec<f64>) {
        self.points_y = Some(y);
    }

    /// Update the profile with the given points used as default. Any locally stored x and y values are
    /// used in preference to the default points when interpolating the profile.
    fn update_profile(&mut self, points: &[(u32, f64)], function: &RadialBasisFunction) {
        let points: Vec<_> = match (&self.points_x, &self.points_y) {
            (Some(x), Some(y)) => x.iter().zip(y.iter()).map(|(x, y)| (*x, *y)).collect(),
            (Some(x), None) => x
                .iter()
                .zip(points.iter().map(|(_, y)| *y))
                .map(|(x, y)| (*x, y))
                .collect(),
            (None, Some(y)) => points.iter().zip(y.iter()).map(|((x, _), y)| (*x, *y)).collect(),
            (None, None) => points.to_vec(),
        };

        self.profile = interpolate_rbf_profile(&points, function);
    }
}

impl RbfProfileParameter {
    pub fn new(name: ParameterName, points: Vec<(u32, f64)>, function: RadialBasisFunction) -> Self {
        Self {
            meta: ParameterMeta::new(name),
            points,
            function,
        }
    }
}

impl Parameter for RbfProfileParameter {
    fn meta(&self) -> &ParameterMeta {
        &self.meta
    }

    fn setup(
        &self,
        _timesteps: &[Timestep],
        _scenario_index: &ScenarioIndex,
    ) -> Result<Option<Box<dyn ParameterState>>, ParameterSetupError> {
        let internal_state = RbfProfileInternalState::new(&self.points, &self.function);
        Ok(Some(Box::new(internal_state)))
    }
    fn as_f64_variable(&self) -> Option<&dyn VariableParameter<f64>> {
        Some(self)
    }

    fn as_f64_variable_mut(&mut self) -> Option<&mut dyn VariableParameter<f64>> {
        Some(self)
    }

    fn as_u32_variable(&self) -> Option<&dyn VariableParameter<u32>> {
        Some(self)
    }

    fn as_u32_variable_mut(&mut self) -> Option<&mut dyn VariableParameter<u32>> {
        Some(self)
    }
}

impl SimpleParameter<f64> for RbfProfileParameter {
    fn compute(
        &self,
        timestep: &Timestep,
        _scenario_index: &ScenarioIndex,
        _values: &SimpleParameterValues,
        internal_state: &mut Option<Box<dyn ParameterState>>,
    ) -> Result<f64, SimpleCalculationError> {
        // Get the profile from the internal state
        let internal_state = downcast_internal_state_ref::<RbfProfileInternalState>(internal_state);
        // Return today's value from the profile
        Ok(internal_state.profile[timestep.day_of_year_index()])
    }

    fn as_parameter(&self) -> &dyn Parameter
    where
        Self: Sized,
    {
        self
    }
}

impl VariableParameter<f64> for RbfProfileParameter {
    fn meta(&self) -> &ParameterMeta {
        &self.meta
    }

    /// The size is the number of points that define the profile.
    fn size(&self, _variable_config: &dyn VariableConfig) -> usize {
        self.points.len()
    }

    /// The f64 values update the profile value of each point.
    ///
    /// # Arguments
    ///
    /// * `values`: The y value to set for the points. This is an array of size equal to the
    ///   number of points in the RBF profile.
    /// * `_variable_config`:
    /// * `internal_state`:
    ///
    /// returns: Result<(), PywrError>
    fn set_variables(
        &self,
        values: &[f64],
        _variable_config: &dyn VariableConfig,
        internal_state: &mut Option<Box<dyn ParameterState>>,
    ) -> Result<(), VariableParameterError> {
        if values.len() == self.points.len() {
            let value = downcast_internal_state_mut::<RbfProfileInternalState>(internal_state);

            value.update_y(values.to_vec());
            value.update_profile(&self.points, &self.function);

            Ok(())
        } else {
            Err(VariableParameterError::IncorrectNumberOfValues {
                expected: self.points.len(),
                received: values.len(),
            })
        }
    }

    /// The f64 values are the profile values of each point.
    fn get_variables(&self, internal_state: &Option<Box<dyn ParameterState>>) -> Option<Vec<f64>> {
        let value = downcast_internal_state_ref::<RbfProfileInternalState>(internal_state);
        value.points_y.clone()
    }

    fn get_lower_bounds(&self, variable_config: &dyn VariableConfig) -> Option<Vec<f64>> {
        let config = downcast_variable_config_ref::<RbfProfileVariableConfig>(variable_config);
        let lb = (0..self.points.len()).map(|_| config.value_lower_bounds).collect();
        Some(lb)
    }

    fn get_upper_bounds(&self, variable_config: &dyn VariableConfig) -> Option<Vec<f64>> {
        let config = downcast_variable_config_ref::<RbfProfileVariableConfig>(variable_config);
        let lb = (0..self.points.len()).map(|_| config.value_upper_bounds).collect();
        Some(lb)
    }
}

impl VariableParameter<u32> for RbfProfileParameter {
    fn meta(&self) -> &ParameterMeta {
        &self.meta
    }
    /// The size is the number of points that define the profile.
    fn size(&self, variable_config: &dyn VariableConfig) -> usize {
        let config = downcast_variable_config_ref::<RbfProfileVariableConfig>(variable_config);
        match config.days_of_year_range {
            Some(_) => self.points.len(),
            None => 0,
        }
    }

    /// Sets the day of year for each point.
    fn set_variables(
        &self,
        values: &[u32],
        _variable_config: &dyn VariableConfig,
        internal_state: &mut Option<Box<dyn ParameterState>>,
    ) -> Result<(), VariableParameterError> {
        if values.len() == self.points.len() {
            let value = downcast_internal_state_mut::<RbfProfileInternalState>(internal_state);

            value.update_x(values.to_vec());
            value.update_profile(&self.points, &self.function);

            Ok(())
        } else {
            Err(VariableParameterError::IncorrectNumberOfValues {
                expected: self.points.len(),
                received: values.len(),
            })
        }
    }

    /// Returns the day of year for each point.
    fn get_variables(&self, internal_state: &Option<Box<dyn ParameterState>>) -> Option<Vec<u32>> {
        let value = downcast_internal_state_ref::<RbfProfileInternalState>(internal_state);
        value.points_x.clone()
    }

    fn get_lower_bounds(&self, variable_config: &dyn VariableConfig) -> Option<Vec<u32>> {
        let config = downcast_variable_config_ref::<RbfProfileVariableConfig>(variable_config);

        if let Some(days_of_year_range) = &config.days_of_year_range {
            // Make sure the lower bound is not less than 1 and handle integer underflow
            let lb = self
                .points
                .iter()
                .map(|p| p.0.checked_sub(*days_of_year_range).unwrap_or(1).max(1))
                .collect();

            Some(lb)
        } else {
            None
        }
    }

    fn get_upper_bounds(&self, variable_config: &dyn VariableConfig) -> Option<Vec<u32>> {
        let config = downcast_variable_config_ref::<RbfProfileVariableConfig>(variable_config);

        if let Some(days_of_year_range) = &config.days_of_year_range {
            // Make sure the upper bound is not greater than 365 and handle integer overflow
            let lb = self
                .points
                .iter()
                .map(|p| p.0.checked_add(*days_of_year_range).unwrap_or(365).min(365))
                .collect();

            Some(lb)
        } else {
            None
        }
    }
}

/// Radial basis functions for interpolation.
pub enum RadialBasisFunction {
    Linear,
    Cubic,
    Quintic,
    ThinPlateSpline,
    Gaussian { epsilon: f64 },
    MultiQuadric { epsilon: f64 },
    InverseMultiQuadric { epsilon: f64 },
}

impl RadialBasisFunction {
    fn compute(&self, r: f64) -> f64 {
        match self {
            RadialBasisFunction::Linear => r,
            RadialBasisFunction::Cubic => r.powi(3),
            RadialBasisFunction::Quintic => r.powi(5),
            RadialBasisFunction::ThinPlateSpline => r.powi(2) * r.ln(),
            RadialBasisFunction::Gaussian { epsilon } => (-(epsilon * r).powi(2)).exp(),
            RadialBasisFunction::MultiQuadric { epsilon } => (1.0 + (epsilon * r).powi(2)).sqrt(),
            RadialBasisFunction::InverseMultiQuadric { epsilon } => (1.0 + (epsilon * r).powi(2)).powf(-0.5),
        }
    }
}

/// Perform radial-basis function interpolation from the given points.
///
/// The provided points are a tuple of observed (x, y) values.
fn interpolate_rbf<const N: usize>(points: &[(f64, f64)], function: &RadialBasisFunction, x: &[f64; N]) -> [f64; N] {
    let n = points.len();

    let matrix = DMatrix::from_fn(n, n, |r, c| {
        let r = (points[c].0 - points[r].0).abs();
        function.compute(r)
    });

    let b = DMatrix::from_fn(n, 1, |r, _| points[r].1);

    let weights = matrix
        .lu()
        .solve(&b)
        .expect("Failed to solve RBF system for interpolation weights.");

    let mut profile = [f64::default(); N];

    for (profile, &doy) in profile.iter_mut().zip(x) {
        *profile = points
            .iter()
            .enumerate()
            .map(|(i, p)| {
                let r = (doy - p.0).abs();
                let distance = function.compute(r);
                distance * weights[(i, 0)]
            })
            .sum();
    }

    profile
}

/// Calculate the interpolation weights for the given points.
///
/// This method repeats the point 365 days before and after the user provided points. This
/// helps create a cyclic interpolation suitable for a annual profile. It then repeats the
/// value for the 58th day to create a daily profile 366 days long.
fn interpolate_rbf_profile(points: &[(u32, f64)], function: &RadialBasisFunction) -> [f64; 366] {
    // Replicate the points in the year before and after.
    let year_before = points.iter().map(|p| (p.0 as f64 - 365.0, p.1));
    let year_after = points.iter().map(|p| (p.0 as f64 + 365.0, p.1));
    let points: Vec<_> = year_before
        .chain(points.iter().map(|p| (p.0 as f64, p.1)))
        .chain(year_after)
        .collect();

    let mut x_out = [f64::default(); 365];
    for (i, v) in x_out.iter_mut().enumerate() {
        *v = i as f64;
    }
    let short_profile = interpolate_rbf(&points, function, &x_out);

    let (start, end) = short_profile.split_at(58);

    let profile = [start, &[end[0]], end].concat();

    profile.try_into().unwrap()
}

#[cfg(test)]
mod tests {
    use crate::parameters::profiles::rbf::{RadialBasisFunction, interpolate_rbf, interpolate_rbf_profile};
    use float_cmp::{F64Margin, assert_approx_eq};
    use std::f64::consts::PI;

    /// Test example from Wikipedia on Rbf interpolation
    ///
    /// This test compares values to those produced by Scipy's Rbf interpolation.
    ///
    /// For future reference, the Scipy code used to produce the expected values is as follows:
    /// ```python
    /// from scipy.interpolate import Rbf
    /// import numpy as np
    /// x = np.array([k / 14.0 for k in range(15)])
    /// f = np.exp(x * np.cos(3.0 * x * np.pi))
    ///
    /// rbf = Rbf(x, f, function='gaussian', epsilon=1/3.0)
    ///
    /// x_out = np.array([k / 149.0 for k in range(150)])
    /// f_interp = rbf(x_out)
    /// print(f_interp)
    /// ```
    #[test]
    fn test_rbf_interpolation() {
        let points: Vec<(f64, f64)> = (0..15)
            .map(|k| {
                let x = k as f64 / 14.0;
                let f = (x * (3.0 * x * PI).cos()).exp();
                (x, f)
            })
            .collect();

        let mut x_out = [f64::default(); 150];
        for (i, v) in x_out.iter_mut().enumerate() {
            *v = i as f64 / 149.0;
        }

        let rbf = RadialBasisFunction::Gaussian { epsilon: 3.0 };
        let f_interp = interpolate_rbf(&points, &rbf, &x_out);

        // Values computed from the Scipy RBF interpolation function for the same problem.
        let f_expected = [
            0.99999999, 1.02215444, 1.03704224, 1.04658357, 1.05232959, 1.0555025, 1.05703598, 1.05761412, 1.05770977,
            1.05762023, 1.0575012, 1.05739784, 1.05727216, 1.0570282, 1.0565335, 1.05563715, 1.05418473, 1.05203042,
            1.04904584, 1.04512659, 1.04019611, 1.03420771, 1.02714462, 1.0190189, 1.00986897, 0.99975608, 0.98876095,
            0.97697989, 0.96451978, 0.9514951, 0.93802364, 0.92422356, 0.91021058, 0.89609542, 0.88198282, 0.86796961,
            0.85414519, 0.8405903, 0.82737825, 0.81457486, 0.80224023, 0.79042854, 0.77919009, 0.76857191, 0.75861923,
            0.74937591, 0.74088519, 0.73319047, 0.72633599, 0.72036607, 0.71532606, 0.71126198, 0.70821968, 0.7062455,
            0.70538494, 0.70568346, 0.7071849, 0.70993231, 0.71396743, 0.71933052, 0.72606058, 0.73419586, 0.74377345,
            0.75483021, 0.76740264, 0.78152758, 0.79724185, 0.81458285, 0.83358751, 0.85429299, 0.87673482, 0.90094656,
            0.926958, 0.95479321, 0.98446917, 1.01599247, 1.04935705, 1.08454095, 1.12150386, 1.16018313, 1.20049191,
            1.24231544, 1.28550918, 1.32989614, 1.37526651, 1.42137569, 1.46794495, 1.51466233, 1.56118419, 1.6071376,
            1.65212512, 1.69572785, 1.7375121, 1.77703531, 1.81385273, 1.84752542, 1.87762766, 1.90375533, 1.92553407,
            1.94262687, 1.95474147, 1.96163779, 1.96313291, 1.95910686, 1.94950578, 1.93434466, 1.91370844, 1.88775047,
            1.85669197, 1.82081727, 1.78046916, 1.73604268, 1.68797763, 1.63674943, 1.58286071, 1.52683076, 1.46918569,
            1.41044858, 1.35112887, 1.29171453, 1.23266261, 1.17439264, 1.11728046, 1.06165402, 1.00779065, 0.95591582,
            0.90620394, 0.8587805, 0.81372578, 0.77108031, 0.73085073, 0.69301704, 0.6575401, 0.62436898, 0.59344848,
            0.56472532, 0.53815332, 0.51369657, 0.49133094, 0.47104256, 0.45282388, 0.43666555, 0.42254569, 0.4104155,
            0.40018055, 0.39167888, 0.38465535, 0.37873281, 0.3733805, 0.36787943,
        ];

        for (i, e) in f_interp.iter().zip(f_expected) {
            assert_approx_eq!(f64, *i, e, F64Margin { ulps: 2, epsilon: 1e-6 });
        }
    }

    /// Test cyclical daily profile interpolation
    ///
    /// This test compares values to those produced by Scipy's Rbf interpolation.
    ///
    /// For future reference, the Scipy code used to produce the expected values is as follows:
    /// ```python
    /// from scipy.interpolate import Rbf
    /// import numpy as np
    /// x = np.array([90, 180, 270])
    /// f = np.array([0.5, 0.3, 0.7])
    ///
    /// x = np.concatenate([x - 365, x, x + 365])
    /// f = np.concatenate([f, f, f])
    ///
    /// rbf = Rbf(x, f, function='multiquadric', epsilon=50.0)
    /// x_out = np.array([k for k in range(365)])
    /// f_interp = rbf(x_out)
    /// print(f_interp)
    /// ```
    #[test]
    fn test_rbf_interpolation_profile() {
        let points: Vec<(u32, f64)> = vec![(90, 0.5), (180, 0.3), (270, 0.7)];

        let rbf = RadialBasisFunction::MultiQuadric { epsilon: 1.0 / 50.0 };
        let f_interp = interpolate_rbf_profile(&points, &rbf);

        let f_expected = [
            0.69464463, 0.69308183, 0.69150736, 0.68992139, 0.68832406, 0.68671551, 0.68509589, 0.68346531, 0.68182389,
            0.68017171, 0.67850888, 0.67683548, 0.67515156, 0.6734572, 0.67175245, 0.67003733, 0.66831189, 0.66657615,
            0.66483011, 0.66307377, 0.66130712, 0.65953014, 0.65774281, 0.65594508, 0.6541369, 0.65231821, 0.65048893,
            0.64864899, 0.64679829, 0.64493672, 0.64306417, 0.64118051, 0.63928561, 0.63737931, 0.63546146, 0.63353187,
            0.63159038, 0.62963677, 0.62767084, 0.62569237, 0.62370112, 0.62169685, 0.61967931, 0.61764821, 0.61560328,
            0.61354422, 0.61147072, 0.60938246, 0.60727911, 0.60516031, 0.60302571, 0.60087495, 0.59870763, 0.59652337,
            0.59432175, 0.59210238, 0.58986482, 0.58760865, 0.58533341, 0.58533341, 0.58303867, 0.58072398, 0.57838887,
            0.57603288, 0.57365555, 0.57125641, 0.568835, 0.56639087, 0.56392355, 0.5614326, 0.55891758, 0.55637805,
            0.55381361, 0.55122386, 0.54860842, 0.54596693, 0.54329907, 0.54060452, 0.53788302, 0.53513433, 0.53235824,
            0.5295546, 0.52672327, 0.52386419, 0.52097732, 0.51806269, 0.51512038, 0.5121505, 0.50915325, 0.50612887,
            0.50307767, 0.5, 0.4968963, 0.49376705, 0.4906128, 0.48743418, 0.48423185, 0.48100655, 0.47775909,
            0.47449034, 0.4712012, 0.46789267, 0.46456578, 0.46122162, 0.45786134, 0.45448613, 0.45109726, 0.44769602,
            0.44428374, 0.44086183, 0.43743171, 0.43399486, 0.4305528, 0.42710707, 0.42365927, 0.42021102, 0.416764,
            0.41331988, 0.4098804, 0.40644733, 0.40302245, 0.3996076, 0.39620462, 0.3928154, 0.38944187, 0.38608597,
            0.38274969, 0.37943505, 0.37614408, 0.37287886, 0.36964152, 0.3664342, 0.36325908, 0.36011837, 0.35701434,
            0.35394927, 0.35092549, 0.34794536, 0.34501129, 0.34212571, 0.33929111, 0.33650999, 0.33378492, 0.33111848,
            0.32851331, 0.32597206, 0.32349743, 0.32109215, 0.31875898, 0.31650072, 0.31432016, 0.31222016, 0.31020357,
            0.30827325, 0.30643209, 0.30468296, 0.30302876, 0.30147235, 0.30001661, 0.29866436, 0.29741843, 0.2962816,
            0.29525658, 0.29434606, 0.29355265, 0.29287889, 0.29232723, 0.29190003, 0.29159955, 0.29142793, 0.29138718,
            0.2914792, 0.29170571, 0.29206829, 0.29256837, 0.29320718, 0.29398581, 0.29490512, 0.29596581, 0.29716836,
            0.29851306, 0.3, 0.30162905, 0.30339988, 0.30531196, 0.30736453, 0.30955665, 0.31188717, 0.31435474,
            0.31695784, 0.31969475, 0.32256357, 0.32556225, 0.32868857, 0.33194015, 0.33531448, 0.33880892, 0.34242071,
            0.34614696, 0.34998469, 0.35393082, 0.35798222, 0.36213562, 0.36638776, 0.37073525, 0.37517472, 0.3797027,
            0.38431572, 0.38901027, 0.39378283, 0.39862985, 0.40354777, 0.40853303, 0.41358206, 0.4186913, 0.42385719,
            0.42907617, 0.43434469, 0.43965922, 0.44501624, 0.45041222, 0.45584367, 0.4613071, 0.46679904, 0.47231604,
            0.47785464, 0.48341142, 0.48898296, 0.49456585, 0.5001567, 0.50575214, 0.51134878, 0.51694328, 0.52253228,
            0.52811245, 0.53368045, 0.53923298, 0.54476672, 0.55027838, 0.55576468, 0.56122234, 0.56664811, 0.57203875,
            0.57739104, 0.58270178, 0.58796779, 0.59318592, 0.59835305, 0.60346609, 0.60852201, 0.61351779, 0.61845048,
            0.62331718, 0.62811505, 0.63284131, 0.63749327, 0.64206831, 0.64656388, 0.65097754, 0.65530696, 0.65954989,
            0.66370421, 0.66776792, 0.67173914, 0.67561613, 0.67939727, 0.68308111, 0.68666633, 0.69015176, 0.6935364,
            0.69681937, 0.7, 0.70307774, 0.7060522, 0.70892317, 0.71169059, 0.71435453, 0.71691524, 0.7193731,
            0.72172864, 0.7239825, 0.72613549, 0.72818851, 0.73014259, 0.73199887, 0.73375858, 0.73542305, 0.7369937,
            0.73847202, 0.73985957, 0.74115796, 0.74236887, 0.74349402, 0.74453517, 0.7454941, 0.74637263, 0.74717258,
            0.7478958, 0.74854413, 0.74911943, 0.74962353, 0.75005827, 0.75042547, 0.75072693, 0.75096445, 0.75113978,
            0.75125466, 0.75131079, 0.75130986, 0.75125351, 0.75114335, 0.75098096, 0.75076789, 0.75050563, 0.75019565,
            0.74983939, 0.74943824, 0.74899356, 0.74850665, 0.74797881, 0.74741128, 0.74680526, 0.74616191, 0.74548238,
            0.74476776, 0.74401911, 0.74323746, 0.74242382, 0.74157913, 0.74070433, 0.73980031, 0.73886796, 0.7379081,
            0.73692155, 0.73590908, 0.73487145, 0.73380939, 0.7327236, 0.73161476, 0.73048351, 0.72933049, 0.7281563,
            0.72696153, 0.72574673, 0.72451244, 0.7232592, 0.72198749, 0.72069779, 0.71939058, 0.71806629, 0.71672535,
            0.71536817, 0.71399514, 0.71260665, 0.71120305, 0.70978469, 0.7083519, 0.706905, 0.7054443, 0.70397008,
            0.70248262, 0.70098218, 0.69946903, 0.69794338, 0.69640548, 0.69485553,
        ];

        for (i, e) in f_interp.iter().zip(f_expected) {
            assert_approx_eq!(f64, *i, e, F64Margin { ulps: 2, epsilon: 1e-6 });
        }
    }
}
