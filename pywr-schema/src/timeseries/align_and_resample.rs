use crate::timeseries::TimeseriesError;
use polars::{prelude::*, series::ops::NullBehavior};
use pywr_core::timestep::TimeDomain;
use std::cmp::Ordering;

pub fn align_and_resample(
    name: &str,
    df: DataFrame,
    time_col: &str,
    domain: &TimeDomain,
    drop_time_col: bool,
) -> Result<DataFrame, TimeseriesError> {
    // Ensure type of time column is datetime and that it is sorted
    let sort_options = SortMultipleOptions::default()
        .with_order_descending(false)
        .with_maintain_order(true);

    let df = df
        .clone()
        .lazy()
        .with_columns([col(time_col).cast(DataType::Datetime(TimeUnit::Milliseconds, None))])
        .collect()?
        .sort([time_col], sort_options)?;

    // Ensure that df start aligns with models start for any resampling
    let df = slice_start(df, time_col, domain)?;

    // Get the durations of the time column
    let durations = df
        .clone()
        .lazy()
        .select([col(time_col)
            .diff(1.into(), NullBehavior::Drop)
            .unique()
            .alias("duration")])
        .collect()?;
    let durations = durations.column("duration")?.duration()?;

    if durations.len() > 1 {
        todo!("Non-uniform timestep are not yet supported");
    }

    let timeseries_duration = match durations.physical().get(0) {
        Some(duration) => duration,
        None => return Err(TimeseriesError::TimeseriesDurationNotFound(name.to_string())),
    };

    let model_duration = domain.step_duration();
    let model_duration_string = model_duration.duration_string();

    let df = match model_duration.milliseconds().cmp(&timeseries_duration) {
        Ordering::Greater => {
            // Downsample
            df.clone()
                .lazy()
                .group_by_dynamic(
                    col(time_col),
                    [],
                    DynamicGroupOptions {
                        every: Duration::parse(model_duration_string.as_str()),
                        period: Duration::parse(model_duration_string.as_str()),
                        offset: Duration::parse("0d"),
                        start_by: StartBy::DataPoint,
                        ..Default::default()
                    },
                )
                .agg([all().exclude_cols([time_col]).as_expr().mean()])
                .collect()?
        }
        Ordering::Less => {
            // Upsample
            // TODO: this does not extend the dataframe beyond its original end date. Should it do when using a forward fill strategy?
            // The df could be extend by the length of the duration it is being resampled to.
            df.clone()
                .upsample::<[String; 0]>([], time_col, Duration::parse(model_duration_string.as_str()))?
                .fill_null(FillNullStrategy::Forward(None))?
        }
        Ordering::Equal => df,
    };

    let mut df = slice_end(df, time_col, domain)?;

    if df.height() != domain.timesteps().len() {
        return Err(TimeseriesError::DataFrameTimestepMismatch(name.to_string()));
    }

    if drop_time_col {
        let _ = df.drop_in_place(time_col)?;
    }

    Ok(df)
}

fn slice_start(df: DataFrame, time_col: &str, domain: &TimeDomain) -> Result<DataFrame, TimeseriesError> {
    let start = domain
        .first_timestep()
        .ok_or_else(|| TimeseriesError::NoTimestepsDefined)?
        .date;
    let df = df.clone().lazy().filter(col(time_col).gt_eq(lit(start))).collect()?;
    Ok(df)
}

fn slice_end(df: DataFrame, time_col: &str, domain: &TimeDomain) -> Result<DataFrame, TimeseriesError> {
    let end = domain
        .last_timestep()
        .ok_or_else(|| TimeseriesError::NoTimestepsDefined)?
        .date;
    let df = df.clone().lazy().filter(col(time_col).lt_eq(lit(end))).collect()?;
    Ok(df)
}

#[cfg(test)]
mod tests {
    use chrono::{NaiveDate, NaiveDateTime};
    use polars::prelude::*;
    use pywr_core::{
        models::ModelDomain,
        scenario::{ScenarioDomain, ScenarioDomainBuilder},
        timestep::{TimeDomain, TimestepDuration, Timestepper},
    };
    use std::num::NonZeroU64;

    use crate::timeseries::align_and_resample::align_and_resample;

    #[test]
    fn test_downsample_and_slice() {
        let start = NaiveDateTime::parse_from_str("2021-01-07 00:00:00", "%Y-%m-%d %H:%M:%S").unwrap();
        let end = NaiveDateTime::parse_from_str("2021-01-20 00:00:00", "%Y-%m-%d %H:%M:%S").unwrap();
        let timestep = TimestepDuration::Days(NonZeroU64::new(7).unwrap());
        let timestepper = Timestepper::new(start, end, timestep);
        let time_domain = TimeDomain::try_from(timestepper).unwrap();

        let scenario_domain: ScenarioDomain = ScenarioDomainBuilder::default().build().unwrap();

        let domain = ModelDomain::new(time_domain, scenario_domain);

        let time = date_range(
            "time".into(),
            NaiveDate::from_ymd_opt(2021, 1, 1).unwrap().into(),
            NaiveDate::from_ymd_opt(2021, 1, 31).unwrap().into(),
            Duration::parse("1d"),
            ClosedWindow::Both,
            TimeUnit::Milliseconds,
            None,
        )
        .unwrap();

        let values: Vec<f64> = (1..32).map(|x| x as f64).collect();
        let mut df = df!(
            "time" => time,
            "values" => values
        )
        .unwrap();

        df = align_and_resample("test", df, "time", domain.time(), false).unwrap();

        let expected_dates = Column::new(
            "time".into(),
            vec![
                NaiveDateTime::parse_from_str("2021-01-07 00:00:00", "%Y-%m-%d %H:%M:%S").unwrap(),
                NaiveDateTime::parse_from_str("2021-01-14 00:00:00", "%Y-%m-%d %H:%M:%S").unwrap(),
            ],
        )
        .cast(&DataType::Datetime(TimeUnit::Milliseconds, None))
        .unwrap();
        let resampled_dates = df.column("time").unwrap();
        assert!(resampled_dates.equals(&expected_dates));

        let expected_values = Column::new(
            "values".into(),
            vec![
                10.0, // mean of 7, 8, 9, 10, 11, 12, 13
                17.0, // mean of 14, 15, 16, 17, 18, 19, 20
            ],
        );
        let resampled_values = df.column("values").unwrap();
        assert!(resampled_values.equals(&expected_values));
    }

    #[test]
    fn test_upsample_and_slice() {
        let start = NaiveDateTime::parse_from_str("2021-01-01 00:00:00", "%Y-%m-%d %H:%M:%S").unwrap();
        let end = NaiveDateTime::parse_from_str("2021-01-14 00:00:00", "%Y-%m-%d %H:%M:%S").unwrap();
        let timestep = TimestepDuration::Days(NonZeroU64::new(1).unwrap());
        let timestepper = Timestepper::new(start, end, timestep);
        let time_domain = TimeDomain::try_from(timestepper).unwrap();

        let scenario_domain: ScenarioDomain = ScenarioDomainBuilder::default().build().unwrap();
        let domain = ModelDomain::new(time_domain, scenario_domain);

        let time = date_range(
            "time".into(),
            NaiveDate::from_ymd_opt(2021, 1, 1).unwrap().into(),
            NaiveDate::from_ymd_opt(2021, 1, 15).unwrap().into(),
            Duration::parse("7d"),
            ClosedWindow::Both,
            TimeUnit::Milliseconds,
            None,
        )
        .unwrap();

        let values: Vec<f64> = vec![1.0, 2.0, 3.0];
        let mut df = df!(
            "time" => time,
            "values" => values
        )
        .unwrap();

        df = align_and_resample("test", df, "time", domain.time(), false).unwrap();

        let expected_values = Column::new(
            "values".into(),
            vec![1.0, 1.0, 1.0, 1.0, 1.0, 1.0, 1.0, 2.0, 2.0, 2.0, 2.0, 2.0, 2.0, 2.0],
        );
        let resampled_values = df.column("values").unwrap();
        assert!(resampled_values.equals(&expected_values));
    }

    #[test]
    #[cfg(feature = "core")]
    fn test_no_resample_slice() {
        let start = NaiveDateTime::parse_from_str("2021-01-01 00:00:00", "%Y-%m-%d %H:%M:%S").unwrap();
        let end = NaiveDateTime::parse_from_str("2021-01-03 00:00:00", "%Y-%m-%d %H:%M:%S").unwrap();
        let timestep = TimestepDuration::Days(NonZeroU64::new(1).unwrap());
        let timestepper = Timestepper::new(start, end, timestep);
        let time_domain = TimeDomain::try_from(timestepper).unwrap();

        let scenario_domain: ScenarioDomain = ScenarioDomainBuilder::default().build().unwrap();
        let domain = ModelDomain::new(time_domain, scenario_domain);

        let time = date_range(
            "time".into(),
            NaiveDate::from_ymd_opt(2021, 1, 1).unwrap().into(),
            NaiveDate::from_ymd_opt(2021, 1, 3).unwrap().into(),
            Duration::parse("1d"),
            ClosedWindow::Both,
            TimeUnit::Milliseconds,
            None,
        )
        .unwrap();

        let values: Vec<f64> = vec![1.0, 2.0, 3.0];
        let mut df = df!(
            "time" => time.clone(),
            "values" => values.clone()
        )
        .unwrap();

        df = align_and_resample("test", df, "time", domain.time(), false).unwrap();

        let expected_values = Column::new("values".into(), values);
        let resampled_values = df.column("values").unwrap();
        assert!(resampled_values.equals(&expected_values));

        let expected_dates = Column::new("time".into(), time)
            .cast(&DataType::Datetime(TimeUnit::Milliseconds, None))
            .unwrap();

        let resampled_dates = df.column("time").unwrap();
        assert!(resampled_dates.equals(&expected_dates));
    }
}
