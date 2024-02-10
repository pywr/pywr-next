use std::{cmp::Ordering, ops::Deref};

use chrono::{format, NaiveDateTime};

use polars::{prelude::*, series::ops::NullBehavior};
use pywr_core::models::ModelDomain;
use pywr_v1_schema::model;

use crate::SchemaError;

pub fn align_and_resample(
    name: &str,
    df: DataFrame,
    time_col: &str,
    domain: &ModelDomain,
) -> Result<DataFrame, SchemaError> {
    // Ensure type of time column is datetime and that it is sorted
    let df = df
        .clone()
        .lazy()
        .with_columns([col(time_col).cast(DataType::Datetime(TimeUnit::Nanoseconds, None))])
        .collect()?
        .sort([time_col], false, true)?;

    // Ensure that df start aligns with models start for any resampling
    let df = slice_start(df, time_col, domain)?;

    // Get the durations of the time column
    let durations = df
        .clone()
        .lazy()
        .select([col(time_col).diff(1, NullBehavior::Drop).unique().alias("duration")])
        .collect()?;
    let durations = durations.column("duration")?.duration()?.deref();

    if durations.len() > 1 {
        // Non-uniform timestep are not yet supported
        todo!();
    }

    let timeseries_duration = match durations.get(0) {
        Some(duration) => duration,
        None => return Err(SchemaError::TimeseriesDurationNotFound(name.to_string())),
    };

    let model_duration = domain.time().step_duration().whole_nanoseconds() as i64;

    let df = match model_duration.cmp(&timeseries_duration) {
        Ordering::Greater => {
            // Downsample
            df.clone()
                .lazy()
                .group_by_dynamic(
                    col(time_col),
                    [],
                    DynamicGroupOptions {
                        every: Duration::new(model_duration),
                        period: Duration::new(model_duration),
                        offset: Duration::new(0),
                        start_by: StartBy::DataPoint,
                        ..Default::default()
                    },
                )
                .agg([col("*").exclude([time_col]).mean()])
                .collect()?
        }
        Ordering::Less => {
            // Upsample
            // TODO: this does not extend the dataframe beyond its original end date. Should it do when using a forward fill strategy?
            // The df could be extend by the length of the duration it is being resampled to.
            df.clone()
                .upsample::<[String; 0]>([], "time", Duration::new(model_duration), Duration::new(0))?
                .fill_null(FillNullStrategy::Forward(None))?
        }
        Ordering::Equal => df,
    };

    let df = slice_end(df, time_col, domain)?;

    // TODO check df length equals number of model timesteps
    Ok(df)
}

fn slice_start(df: DataFrame, time_col: &str, domain: &ModelDomain) -> Result<DataFrame, SchemaError> {
    let start = domain.time().first_timestep().date.midnight().assume_utc();
    let start = NaiveDateTime::from_timestamp_opt(start.unix_timestamp(), 0).unwrap();
    let df = df.clone().lazy().filter(col(time_col).gt_eq(lit(start))).collect()?;
    Ok(df)
}

fn slice_end(df: DataFrame, time_col: &str, domain: &ModelDomain) -> Result<DataFrame, SchemaError> {
    let end = domain.time().last_timestep().date.midnight().assume_utc();
    let end = NaiveDateTime::from_timestamp_opt(end.unix_timestamp(), 0).unwrap();
    let df = df.clone().lazy().filter(col(time_col).lt_eq(lit(end))).collect()?;
    Ok(df)
}

#[cfg(test)]
mod tests {
    //use polars::{datatypes::TimeUnit, time::{ClosedWindow, Duration}};
    use chrono::{NaiveDate, NaiveDateTime};
    use polars::prelude::*;
    use pywr_core::{
        models::ModelDomain,
        scenario::{ScenarioDomain, ScenarioGroupCollection},
        timestep::{TimeDomain, Timestepper},
    };
    use time::{Date, Month};

    use crate::timeseries::{align_and_resample::align_and_resample, tests};

    #[test]
    fn test_downsample_and_slice() {
        let time_domain: TimeDomain = Timestepper::new(
            Date::from_calendar_date(2021, Month::January, 7).unwrap(),
            Date::from_calendar_date(2021, Month::January, 20).unwrap(),
            7,
        )
        .into();

        let scenario_domain: ScenarioDomain = ScenarioGroupCollection::new(vec![]).into();

        let domain = ModelDomain::new(time_domain, scenario_domain);

        let time = polars::time::date_range(
            "time",
            NaiveDate::from_ymd_opt(2021, 1, 1).unwrap().into(),
            NaiveDate::from_ymd_opt(2021, 1, 31).unwrap().into(),
            Duration::parse("1d"),
            ClosedWindow::Both,
            TimeUnit::Milliseconds,
            None,
        )
        .unwrap();
        //let values: Vec<f64> = vec![1.0; 31];
        let values: Vec<f64> = (1..32).map(|x| x as f64).collect();
        let mut df = df!(
            "time" => time,
            "values" => values
        )
        .unwrap();

        df = align_and_resample("test", df, "time", &domain).unwrap();

        let expected_dates = Series::new(
            "time",
            vec![
                NaiveDateTime::parse_from_str("2021-01-07 00:00:00", "%Y-%m-%d %H:%M:%S").unwrap(),
                NaiveDateTime::parse_from_str("2021-01-14 00:00:00", "%Y-%m-%d %H:%M:%S").unwrap(),
            ],
        )
        .cast(&DataType::Datetime(TimeUnit::Nanoseconds, None))
        .unwrap();
        let resampled_dates = df.column("time").unwrap();
        assert!(resampled_dates.equals(&expected_dates));

        let expected_values = Series::new(
            "values",
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
        let time_domain: TimeDomain = Timestepper::new(
            Date::from_calendar_date(2021, Month::January, 1).unwrap(),
            Date::from_calendar_date(2021, Month::January, 14).unwrap(),
            1,
        )
        .into();
        let scenario_domain: ScenarioDomain = ScenarioGroupCollection::new(vec![]).into();
        let domain = ModelDomain::new(time_domain, scenario_domain);

        let time = polars::time::date_range(
            "time",
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

        df = align_and_resample("test", df, "time", &domain).unwrap();

        let expected_values = Series::new(
            "values",
            vec![1.0, 1.0, 1.0, 1.0, 1.0, 1.0, 1.0, 2.0, 2.0, 2.0, 2.0, 2.0, 2.0, 2.0],
        );
        let resampled_values = df.column("values").unwrap();
        assert!(resampled_values.equals(&expected_values));
    }

    #[test]
    fn test_no_resample_slice() {
        let time_domain: TimeDomain = Timestepper::new(
            Date::from_calendar_date(2021, Month::January, 1).unwrap(),
            Date::from_calendar_date(2021, Month::January, 3).unwrap(),
            1,
        )
        .into();
        let scenario_domain: ScenarioDomain = ScenarioGroupCollection::new(vec![]).into();
        let domain = ModelDomain::new(time_domain, scenario_domain);

        let time = polars::time::date_range(
            "time",
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

        df = align_and_resample("test", df, "time", &domain).unwrap();

        let expected_values = Series::new("values", values);
        let resampled_values = df.column("values").unwrap();
        assert!(resampled_values.equals(&expected_values));

        let expected_dates = Series::new("time", time)
            .cast(&DataType::Datetime(TimeUnit::Nanoseconds, None))
            .unwrap();
        let resampled_dates = df.column("time").unwrap();
        assert!(resampled_dates.equals(&expected_dates));
    }
}
