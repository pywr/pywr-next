use crate::network::ResolutionMaps;
use crate::parameters::errors::SimpleCalculationError;
use crate::parameters::{
    BuiltParameter, MaybeBuiltParameter, Parameter, ParameterBuildError, ParameterBuilder, ParameterMeta,
    ParameterName, ParameterState, SimpleParameter, SimpleParameterContext,
};
use crate::timestep::{TimeDomain, Timestep, TimestepIndex};
use arrow::array::{Array, ArrayRef, ArrowPrimitiveType, AsArray, PrimitiveArray};
use arrow::compute::cast;
use arrow::datatypes::{DataType, Float64Type, TimeUnit, TimestampMillisecondType, UInt64Type};
use arrow::temporal_conversions::timestamp_ms_to_datetime;
use chrono::{Datelike, Timelike};
use jiff::civil::DateTime;
use std::fmt::Debug;
use std::sync::Arc;

/// A parameter that is backed by an Arrow Float64Array.
///
/// This parameter stores its values in an Arrow `Float64Array` and allows for efficient
/// access to the values based on the model's time-steps.
#[derive(Debug)]
pub struct Array1Parameter<T: ArrowPrimitiveType> {
    meta: ParameterMeta,
    array: PrimitiveArray<T>,
    timestep_offset: Option<i32>,
}

/// Compute the time-step index to use accounting for any defined offset.
///
/// The offset is applied to the time-step index and then clamped to the bounds of the array.
/// This ensures that the time-step index is always within the bounds of the array.
#[inline]
fn timestep_index(timestep: &Timestep, offset: Option<i32>, array_len: usize) -> TimestepIndex {
    let i = match offset {
        None => timestep.index as i64,
        Some(offset) => timestep.index as i64 + offset as i64,
    };
    i.max(0).min(array_len as i64 - 1) as usize
}

impl<T> Parameter for Array1Parameter<T>
where
    T: ArrowPrimitiveType + Debug,
{
    fn meta(&self) -> &ParameterMeta {
        &self.meta
    }
}
impl SimpleParameter<f64> for Array1Parameter<Float64Type> {
    fn compute(
        &self,
        ctx: SimpleParameterContext<'_>,
        _internal_state: &mut Option<Box<dyn ParameterState>>,
    ) -> Result<f64, SimpleCalculationError> {
        let idx = timestep_index(ctx.timestep, self.timestep_offset, self.array.len());
        let value = self.array.value(idx);
        Ok(value)
    }
    fn as_parameter(&self) -> &dyn Parameter
    where
        Self: Sized,
    {
        self
    }
}

impl SimpleParameter<u64> for Array1Parameter<UInt64Type> {
    fn compute(
        &self,
        ctx: SimpleParameterContext<'_>,
        _internal_state: &mut Option<Box<dyn ParameterState>>,
    ) -> Result<u64, SimpleCalculationError> {
        let idx = timestep_index(ctx.timestep, self.timestep_offset, self.array.len());
        let value = self.array.value(idx);
        Ok(value)
    }
    fn as_parameter(&self) -> &dyn Parameter
    where
        Self: Sized,
    {
        self
    }
}

/// Builder for `Float64ArrayParameter` or `UInt64ArrayParameter`.
///
/// This builder allows for the construction of a `Float64ArrayParameter` with optional
/// time-step offset and time array. If a time array is provided, it should correspond to the
/// time-steps in the array. This array will be used to align the parameter values with the
/// model's time-steps.
///
/// The builder is generic over the type of the array, allowing for flexibility in the underlying data type.
/// During the build process, the array will be cast to the appropriate type for the parameter. If
/// the array cannot be cast to the required type, an error will be returned.
#[derive(Debug)]
pub struct Array1ParameterBuilder {
    meta: ParameterMeta,
    array: ArrayRef,
    timestep_offset: Option<i32>,
    // Optional array of DateTime values corresponding to the time-steps in the array.
    time_array: Option<ArrayRef>,
}

impl Array1ParameterBuilder {
    pub fn from_array_ref(name: ParameterName, array: &ArrayRef) -> Self {
        Self {
            meta: ParameterMeta::new(name),
            array: array.clone(),
            timestep_offset: None,
            time_array: None,
        }
    }

    pub fn from_primitive_array<T: ArrowPrimitiveType>(name: ParameterName, array: PrimitiveArray<T>) -> Self {
        Self {
            meta: ParameterMeta::new(name),
            array: Arc::new(array),
            timestep_offset: None,
            time_array: None,
        }
    }

    pub fn timestep_offset(&mut self, offset: i32) -> &mut Self {
        self.timestep_offset = Some(offset);
        self
    }

    /// Set the time array for the parameter.
    pub fn time(&mut self, time_array: ArrayRef) -> &mut Self {
        self.time_array = Some(time_array);
        self
    }
}

impl ParameterBuilder<f64> for Array1ParameterBuilder {
    fn name(&self) -> &ParameterName {
        &self.meta.name
    }
    fn build(
        self: Box<Self>,
        resolution_maps: &ResolutionMaps,
    ) -> Result<MaybeBuiltParameter<f64>, ParameterBuildError> {
        let mut array =
            cast(&self.array, &DataType::Float64).map_err(|source| ParameterBuildError::ArrayCastError {
                from: self.array.data_type().clone(),
                to: DataType::Float64,
                source,
            })?;

        if let Some(time_array) = &self.time_array {
            // Convert to DateTime array and align with the time-steps in the model.
            array = subslice_data_for_time_domain(&array, time_array, resolution_maps.domain.time())?;
        }

        // SAFETY: We just cast the array to Float64, so it is safe to assume it is a Float64Array.
        // We clone the array to ensure we have ownership of the data.
        let array = array.as_primitive::<Float64Type>().clone();

        let p = Array1Parameter {
            meta: self.meta,
            array,
            timestep_offset: self.timestep_offset,
        };
        Ok(BuiltParameter::Simple(Box::new(p)).into())
    }
}

impl ParameterBuilder<u64> for Array1ParameterBuilder {
    fn name(&self) -> &ParameterName {
        &self.meta.name
    }
    fn build(
        self: Box<Self>,
        resolution_maps: &ResolutionMaps,
    ) -> Result<MaybeBuiltParameter<u64>, ParameterBuildError> {
        let mut array = cast(&self.array, &DataType::UInt64).map_err(|source| ParameterBuildError::ArrayCastError {
            from: self.array.data_type().clone(),
            to: DataType::UInt64,
            source,
        })?;

        if let Some(time_array) = &self.time_array {
            // Convert to DateTime array and align with the time-steps in the model.
            array = subslice_data_for_time_domain(&array, time_array, resolution_maps.domain.time())?;
        }

        // SAFETY: We just cast the array to UInt64, so it is safe to assume it is a UInt64Array.
        // We clone the array to ensure we have ownership of the data.
        let array = array.as_primitive::<UInt64Type>().clone();

        let p = Array1Parameter {
            meta: self.meta,
            array,
            timestep_offset: self.timestep_offset,
        };
        Ok(BuiltParameter::Simple(Box::new(p)).into())
    }
}

#[derive(Debug)]
pub struct Array2Parameter<T: ArrowPrimitiveType> {
    meta: ParameterMeta,
    array: Vec<PrimitiveArray<T>>,
    scenario_group_index: usize,
    timestep_offset: Option<i32>,
}

impl<T> Parameter for Array2Parameter<T>
where
    T: ArrowPrimitiveType + Debug,
{
    fn meta(&self) -> &ParameterMeta {
        &self.meta
    }
}

impl SimpleParameter<f64> for Array2Parameter<Float64Type> {
    fn compute(
        &self,
        ctx: SimpleParameterContext<'_>,
        _internal_state: &mut Option<Box<dyn ParameterState>>,
    ) -> Result<f64, SimpleCalculationError> {
        let s_idx = ctx.scenario_index.simulation_index_for_group(self.scenario_group_index);

        let array = self.array.get(s_idx).ok_or_else(|| {
            let length = self.array.len();
            SimpleCalculationError::OutOfBoundsError {
                index: s_idx,
                length,
                axis: 1,
            }
        })?;

        let t_idx = timestep_index(ctx.timestep, self.timestep_offset, array.len());

        Ok(array.value(t_idx))
    }
    fn as_parameter(&self) -> &dyn Parameter
    where
        Self: Sized,
    {
        self
    }
}

impl SimpleParameter<u64> for Array2Parameter<UInt64Type> {
    fn compute(
        &self,
        ctx: SimpleParameterContext<'_>,
        _internal_state: &mut Option<Box<dyn ParameterState>>,
    ) -> Result<u64, SimpleCalculationError> {
        let s_idx = ctx.scenario_index.simulation_index_for_group(self.scenario_group_index);

        let array = self.array.get(s_idx).ok_or_else(|| {
            let length = self.array.len();
            SimpleCalculationError::OutOfBoundsError {
                index: s_idx,
                length,
                axis: 1,
            }
        })?;

        let t_idx = timestep_index(ctx.timestep, self.timestep_offset, array.len());

        Ok(array.value(t_idx))
    }
    fn as_parameter(&self) -> &dyn Parameter
    where
        Self: Sized,
    {
        self
    }
}

#[derive(Debug)]
pub struct Array2ParameterBuilder {
    meta: ParameterMeta,
    array: Vec<ArrayRef>,
    scenario_group: String,
    timestep_offset: Option<i32>,
    // Optional array of DateTime values corresponding to the time-steps in the array.
    time_array: Option<ArrayRef>,
}

impl Array2ParameterBuilder {
    pub fn from_array_refs(name: ParameterName, array: &[&ArrayRef], scenario_group: &str) -> Self {
        Self {
            meta: ParameterMeta::new(name),
            array: array.iter().map(|a| (*a).clone()).collect(),
            scenario_group: scenario_group.to_string(),
            timestep_offset: None,
            time_array: None,
        }
    }

    pub fn from_primitive_arrays<T: ArrowPrimitiveType>(
        name: ParameterName,
        array: &[PrimitiveArray<T>],
        scenario_group: &str,
    ) -> Self {
        let array_ref: Vec<ArrayRef> = array.iter().map(|a| Arc::new(a.clone()) as ArrayRef).collect();
        Self {
            meta: ParameterMeta::new(name),
            array: array_ref,
            scenario_group: scenario_group.to_string(),
            timestep_offset: None,
            time_array: None,
        }
    }

    pub fn timestep_offset(&mut self, offset: i32) -> &mut Self {
        self.timestep_offset = Some(offset);
        self
    }

    /// Set the time array for the parameter.
    pub fn time(&mut self, time_array: &ArrayRef) -> &mut Self {
        self.time_array = Some(time_array.clone());
        self
    }
}

impl ParameterBuilder<f64> for Array2ParameterBuilder {
    fn name(&self) -> &ParameterName {
        &self.meta.name
    }
    fn build(
        self: Box<Self>,
        resolution_maps: &ResolutionMaps,
    ) -> Result<MaybeBuiltParameter<f64>, ParameterBuildError> {
        let scenario_group_index = resolution_maps.domain.scenarios().group_index(&self.scenario_group)?;

        let mut array = self.array;

        if let Some(time_array) = &self.time_array {
            // Convert to DateTime array and align with the time-steps in the model.
            array = subslice_datas_for_time_domain(&array, time_array, resolution_maps.domain.time())?;
        }

        // If there is a scenario subset then we can reduce the data to align with the scenarios
        // that are actually used in the model.
        if let Some(subset) = resolution_maps
            .domain
            .scenarios()
            .group_scenario_subset(&self.scenario_group)?
        {
            array = array
                .into_iter()
                .enumerate()
                .filter_map(|(i, a)| subset.contains(&i).then_some(a))
                .collect();
        }

        // Now we need to cast each array in the vector to Float64Array
        let f64_array = array
            .into_iter()
            .map(|a| {
                cast(&a, &DataType::Float64)
                    .map_err(|source| ParameterBuildError::ArrayCastError {
                        from: a.data_type().clone(),
                        to: DataType::Float64,
                        source,
                    })
                    .map(|a| a.as_primitive::<Float64Type>().clone())
            })
            .collect::<Result<Vec<_>, _>>()?;

        let p = Array2Parameter {
            meta: self.meta,
            array: f64_array,
            scenario_group_index,
            timestep_offset: self.timestep_offset,
        };

        Ok(BuiltParameter::Simple(Box::new(p)).into())
    }
}

impl ParameterBuilder<u64> for Array2ParameterBuilder {
    fn name(&self) -> &ParameterName {
        &self.meta.name
    }
    fn build(
        self: Box<Self>,
        resolution_maps: &ResolutionMaps,
    ) -> Result<MaybeBuiltParameter<u64>, ParameterBuildError> {
        let scenario_group_index = resolution_maps.domain.scenarios().group_index(&self.scenario_group)?;

        let mut array = self.array;

        if let Some(time_array) = &self.time_array {
            // Convert to DateTime array and align with the time-steps in the model.
            array = subslice_datas_for_time_domain(&array, time_array, resolution_maps.domain.time())?;
        }

        // If there is a scenario subset then we can reduce the data to align with the scenarios
        // that are actually used in the model.
        if let Some(subset) = resolution_maps
            .domain
            .scenarios()
            .group_scenario_subset(&self.scenario_group)?
        {
            array = array
                .into_iter()
                .enumerate()
                .filter_map(|(i, a)| subset.contains(&i).then_some(a))
                .collect();
        }

        // Now we need to cast each array in the vector to Float64Array
        let u64_array = array
            .into_iter()
            .map(|a| {
                cast(&a, &DataType::UInt64)
                    .map_err(|source| ParameterBuildError::ArrayCastError {
                        from: a.data_type().clone(),
                        to: DataType::UInt64,
                        source,
                    })
                    .map(|a| a.as_primitive::<UInt64Type>().clone())
            })
            .collect::<Result<Vec<_>, _>>()?;

        let p = Array2Parameter {
            meta: self.meta,
            array: u64_array,
            scenario_group_index,
            timestep_offset: self.timestep_offset,
        };
        Ok(BuiltParameter::Simple(Box::new(p)).into())
    }
}

fn array_to_datetime(array: &ArrayRef) -> Result<Vec<DateTime>, ParameterBuildError> {
    // Convert the array to millisecond timestamps since the Unix epoch.
    let timestamp_array = cast(array, &DataType::Timestamp(TimeUnit::Millisecond, None)).map_err(|source| {
        ParameterBuildError::DateParseError {
            message: format!(
                "Failed to cast array to Timestamp. Is the array of the correct type? {}",
                source
            ),
        }
    })?;

    // SAFETY: We just cast the array to millisecond timestamps, so this is a TimestampMillisecondArray.
    let timestamp_array_ref = timestamp_array.as_primitive::<TimestampMillisecondType>();

    let mut dt_vec = Vec::with_capacity(timestamp_array.len());
    for i in 0..timestamp_array.len() {
        if timestamp_array_ref.is_null(i) {
            return Err(ParameterBuildError::DateParseError {
                message: format!("Time array contains a null value at index {i}"),
            });
        }

        let v = timestamp_array_ref.value(i);
        // Arrow provides conversion to chrono NaiveDataTime
        let dt = timestamp_ms_to_datetime(v).ok_or(ParameterBuildError::DateParseError {
            message: format!("Failed to parse Timestamp to datetime value: {}", v),
        })?;
        // Convert chrono NaiveDateTime to jiff DateTime

        let dt = DateTime::new(
            dt.year() as i16,
            dt.month() as i8,
            dt.day() as i8,
            dt.hour() as i8,
            dt.minute() as i8,
            dt.second() as i8,
            dt.nanosecond() as i32,
        )
        .map_err(|_| ParameterBuildError::DateParseError {
            message: format!("Failed to convert chrono NaiveDateTime value {} to jiff DateTime", v),
        })?;
        dt_vec.push(dt);
    }
    Ok(dt_vec)
}

fn find_subslice<T: PartialEq>(haystack: &[T], needle: &[T]) -> Option<usize> {
    haystack.windows(needle.len()).position(|window| window == needle)
}

fn has_repeated_values<T: PartialEq>(values: &[T]) -> bool {
    values
        .iter()
        .enumerate()
        .any(|(index, value)| values[..index].contains(value))
}

fn time_alignment_error(time_domain: &[DateTime], data: &[DateTime]) -> ParameterBuildError {
    ParameterBuildError::TimeAlignmentError {
        time_domain_start: time_domain[0],
        time_domain_end: *time_domain.last().unwrap(),
        data_start: data.first().copied().unwrap_or(time_domain[0]),
        data_end: data.last().copied().unwrap_or(time_domain[0]),
    }
}

fn subslice_data_for_time_domain(
    data: &ArrayRef,
    time: &ArrayRef,
    time_domain: &TimeDomain,
) -> Result<ArrayRef, ParameterBuildError> {
    // Convert to DateTime array and align with the time-steps in the model.
    let array_dt = array_to_datetime(time)?;
    let time_domain_dt = time_domain.timesteps().iter().map(|ts| ts.date).collect::<Vec<_>>();

    if data.len() != array_dt.len() {
        return Err(ParameterBuildError::TimeArrayLengthMismatch {
            time: array_dt.len(),
            data: data.len(),
        });
    }

    if has_repeated_values(&array_dt) {
        return Err(time_alignment_error(&time_domain_dt, &array_dt));
    }

    let position =
        find_subslice(&array_dt, &time_domain_dt).ok_or_else(|| time_alignment_error(&time_domain_dt, &array_dt))?;

    let array = data.slice(position, time_domain_dt.len());
    Ok(array)
}

fn subslice_datas_for_time_domain(
    data: &[ArrayRef],
    time: &ArrayRef,
    time_domain: &TimeDomain,
) -> Result<Vec<ArrayRef>, ParameterBuildError> {
    // Convert to DateTime array and align with the time-steps in the model.
    let array_dt = array_to_datetime(time)?;
    let time_domain_dt = time_domain.timesteps().iter().map(|ts| ts.date).collect::<Vec<_>>();

    if let Some(array) = data.iter().find(|array| array.len() != array_dt.len()) {
        return Err(ParameterBuildError::TimeArrayLengthMismatch {
            time: array_dt.len(),
            data: array.len(),
        });
    }

    if has_repeated_values(&array_dt) {
        return Err(time_alignment_error(&time_domain_dt, &array_dt));
    }

    let position =
        find_subslice(&array_dt, &time_domain_dt).ok_or_else(|| time_alignment_error(&time_domain_dt, &array_dt))?;

    let arrays = data
        .iter()
        .map(|array| array.slice(position, time_domain_dt.len()))
        .collect::<Vec<_>>();
    Ok(arrays)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::state::StateBuilder;
    use crate::test_utils::default_domain;
    use crate::timestep::{TimeDomainBuilder, TimestepDuration};
    use arrow::array::{Date32Array, Float64Array, TimestampMillisecondArray};
    use float_cmp::assert_approx_eq;
    use jiff::civil::date;
    use std::num::NonZeroU64;

    fn daily_time_domain(start_day: i8, end_day: i8) -> TimeDomain {
        TimeDomainBuilder::new(
            date(1970, 1, start_day).at(0, 0, 0, 0),
            date(1970, 1, end_day).at(0, 0, 0, 0),
            TimestepDuration::Days(NonZeroU64::new(1).unwrap()),
        )
        .build()
        .unwrap()
    }

    fn hourly_time_domain(start_hour: i8, end_hour: i8) -> TimeDomain {
        TimeDomainBuilder::new(
            date(1970, 1, 1).at(start_hour, 0, 0, 0),
            date(1970, 1, 1).at(end_hour, 0, 0, 0),
            TimestepDuration::Hours(NonZeroU64::new(1).unwrap()),
        )
        .build()
        .unwrap()
    }

    fn date32_array(days: impl IntoIterator<Item = i32>) -> ArrayRef {
        Arc::new(Date32Array::from_iter_values(days))
    }

    fn timestamp_ms_array(milliseconds: impl IntoIterator<Item = i64>) -> ArrayRef {
        Arc::new(TimestampMillisecondArray::from_iter_values(milliseconds))
    }

    fn values(array: &ArrayRef) -> Vec<f64> {
        array.as_primitive::<Float64Type>().values().to_vec()
    }

    #[test]
    fn test_array1_parameter() {
        let domain = default_domain();

        let data: Float64Array = (0..366).map(|i| i as f64).collect();
        let p = Array1Parameter::<Float64Type> {
            meta: ParameterMeta::new("my-array-parameter".into()),
            array: data,
            timestep_offset: None,
        };

        let spv = StateBuilder::new(Vec::new(), 0).build();

        let mut state = p
            .setup(domain.time().timesteps(), domain.scenarios().indices().first().unwrap())
            .unwrap();

        for ts in domain.time().timesteps().iter() {
            for si in domain.scenarios().indices().iter() {
                let ctx = SimpleParameterContext {
                    timestep: ts,
                    scenario_index: si,
                    values: &spv.get_simple_parameter_values(),
                };
                assert_approx_eq!(f64, p.compute(ctx, &mut state).unwrap(), ts.index as f64);
            }
        }
    }

    #[test]
    /// Test `Array2Parameter` returns the correct value.
    fn test_array2_parameter() {
        let domain = default_domain();

        let data: Float64Array = (0..366).map(|i| i as f64).collect();
        let p = Array2Parameter {
            meta: ParameterMeta::new("my-array-parameter".into()),
            array: vec![data],
            scenario_group_index: 0,
            timestep_offset: None,
        };

        let spv = StateBuilder::new(Vec::new(), 0).build();

        let mut state = p
            .setup(domain.time().timesteps(), domain.scenarios().indices().first().unwrap())
            .unwrap();

        for ts in domain.time().timesteps().iter() {
            for si in domain.scenarios().indices().iter() {
                let ctx = SimpleParameterContext {
                    timestep: ts,
                    scenario_index: si,
                    values: &spv.get_simple_parameter_values(),
                };

                assert_approx_eq!(f64, p.compute(ctx, &mut state).unwrap(), ts.index as f64);
            }
        }
    }

    #[test]
    /// Test `Array2Parameter` returns the correct value.
    fn test_array2_parameter_not_enough_data() {
        let domain = default_domain();

        let data: Float64Array = (0..=5).map(|i| i as f64).collect();

        let p = Array2Parameter {
            meta: ParameterMeta::new("my-array-parameter".into()),
            array: vec![data],
            scenario_group_index: 0,
            timestep_offset: None,
        };

        let spv = StateBuilder::new(Vec::new(), 0).build();

        let mut state = p
            .setup(domain.time().timesteps(), domain.scenarios().indices().first().unwrap())
            .unwrap();

        for ts in domain.time().timesteps().iter() {
            for si in domain.scenarios().indices().iter() {
                let ctx = SimpleParameterContext {
                    timestep: ts,
                    scenario_index: si,
                    values: &spv.get_simple_parameter_values(),
                };

                // Parameter should return the last value in the array if the time-step index exceeds the array length.
                assert_approx_eq!(f64, p.compute(ctx, &mut state).unwrap(), ts.index.min(5) as f64);
            }
        }
    }

    #[test]
    fn test_array_to_datetime_vec() {
        let date32_values = vec![0, 1, 2, 3, 4]; // Days since epoch
        let date32_array = arrow::array::Date32Array::from(date32_values);
        let array_ref: ArrayRef = Arc::new(date32_array);

        let dt_vec = array_to_datetime(&array_ref).unwrap();

        assert_eq!(dt_vec.len(), 5);
        assert_eq!(dt_vec[0].year(), 1970);
        assert_eq!(dt_vec[0].month(), 1);
        assert_eq!(dt_vec[0].day(), 1);
        assert_eq!(dt_vec[1].day(), 2);
        assert_eq!(dt_vec[2].day(), 3);
        assert_eq!(dt_vec[3].day(), 4);
        assert_eq!(dt_vec[4].day(), 5);
    }

    #[test]
    fn test_array_to_datetime_preserves_timestamp_time_of_day_and_milliseconds() {
        let timestamps = timestamp_ms_array([5_400_123]);

        let datetimes = array_to_datetime(&timestamps).unwrap();

        assert_eq!(datetimes[0], date(1970, 1, 1).at(1, 30, 0, 123_000_000));
    }

    #[test]
    fn test_array_to_datetime_rejects_null_temporal_values() {
        let temporal_arrays: [ArrayRef; 2] = [
            Arc::new(Date32Array::from(vec![Some(0), None])),
            Arc::new(TimestampMillisecondArray::from(vec![Some(0), None])),
        ];

        for array in temporal_arrays {
            let error = array_to_datetime(&array).unwrap_err();
            assert!(matches!(error, ParameterBuildError::DateParseError { .. }));
            assert!(error.to_string().contains("null value at index 1"));
        }
    }

    #[test]
    fn test_subslice_data_for_time_domain_uses_model_dates_within_data_dates() {
        let data: ArrayRef = Arc::new(Float64Array::from_iter_values((0..7).map(|value| value as f64)));
        let time = date32_array(0..7);

        let result = subslice_data_for_time_domain(&data, &time, &daily_time_domain(3, 5)).unwrap();

        assert_eq!(values(&result), vec![2.0, 3.0, 4.0]);
    }

    #[test]
    fn test_subslice_data_for_hourly_time_domain_uses_timestamp_offset() {
        let data: ArrayRef = Arc::new(Float64Array::from_iter_values((0..6).map(|value| value as f64)));
        let time = timestamp_ms_array((0..6).map(|hour| hour * 3_600_000));

        let result = subslice_data_for_time_domain(&data, &time, &hourly_time_domain(2, 4)).unwrap();

        assert_eq!(values(&result), vec![2.0, 3.0, 4.0]);
    }

    #[test]
    fn test_subslice_datas_for_time_domain_applies_same_offset_to_each_scenario() {
        let data: Vec<ArrayRef> = vec![
            Arc::new(Float64Array::from_iter_values((0..7).map(|value| value as f64))),
            Arc::new(Float64Array::from_iter_values((100..107).map(|value| value as f64))),
        ];
        let time = date32_array(0..7);

        let result = subslice_datas_for_time_domain(&data, &time, &daily_time_domain(3, 5)).unwrap();

        assert_eq!(
            result.iter().map(values).collect::<Vec<_>>(),
            vec![vec![2.0, 3.0, 4.0], vec![102.0, 103.0, 104.0]]
        );
    }

    #[test]
    fn test_subslice_data_for_time_domain_rejects_unaligned_dates() {
        let data: ArrayRef = Arc::new(Float64Array::from_iter_values((0..3).map(|value| value as f64)));
        let time = date32_array(0..3);

        let error = subslice_data_for_time_domain(&data, &time, &daily_time_domain(3, 5)).unwrap_err();

        assert!(matches!(error, ParameterBuildError::TimeAlignmentError { .. }));
    }

    #[test]
    fn test_subslice_data_for_time_domain_rejects_repeated_timestamps_with_matching_lengths() {
        let data: ArrayRef = Arc::new(Float64Array::from_iter_values((0..5).map(|value| value as f64)));
        let time = timestamp_ms_array([0, 3_600_000, 3_600_000, 7_200_000, 10_800_000]);

        let error = subslice_data_for_time_domain(&data, &time, &hourly_time_domain(1, 3)).unwrap_err();

        assert!(matches!(error, ParameterBuildError::TimeAlignmentError { .. }));
    }

    #[test]
    fn test_subslice_datas_for_time_domain_rejects_timestamp_value_length_mismatch() {
        let data: Vec<ArrayRef> = vec![Arc::new(Float64Array::from_iter_values(
            (0..6).map(|value| value as f64),
        ))];
        let time = date32_array(0..7);

        let error = subslice_datas_for_time_domain(&data, &time, &daily_time_domain(3, 5)).unwrap_err();

        assert!(matches!(
            error,
            ParameterBuildError::TimeArrayLengthMismatch { time: 7, data: 6 }
        ));
    }
}
