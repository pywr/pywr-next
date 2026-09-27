use pywr_schema::data_tables::{CsvDataTable, CsvDataTableLookup, DataTableValueType, PlaceholderTable};
use pywr_schema::nodes::{AggregatedNode, PlaceholderNode, VirtualStorageNode};
use pywr_schema::outputs::{ArrowStreamOutput, MemoryOutput, PlaceholderOutput};
use pywr_schema::parameters::{
    DEFAULT_DELAY, DelayParameter, HydropowerTargetParameter, MaxParameter, MinParameter, ParameterPhase,
    PlaceholderParameter,
};
use pywr_schema::time_series::{ArrowTimeSeries, PandasTimeSeries, PlaceholderTimeSeries};
use strum::IntoEnumIterator;

macro_rules! check_all_into_type_pairs {
    ($enum:ty, $kind:ty; $($source:expr),+ $(,)?) => {{
        let sources: Vec<$enum> = vec![$($source),+];
        for source in sources {
            for target in <$kind>::iter() {
                let converted = source.clone().into_type(target);
                let actual: $kind = (&converted).into();
                assert_eq!(actual, target);
            }
        }
    }};
}

#[test]
fn into_type_covers_all_non_node_pairs() {
    use pywr_schema::data_tables::{DataTable, DataTableType};
    use pywr_schema::nodes::{VirtualNode, VirtualNodeType};
    use pywr_schema::outputs::{Output, OutputType};
    use pywr_schema::parameters::{Parameter, ParameterType};
    use pywr_schema::time_series::{TimeSeries, TimeSeriesType};

    check_all_into_type_pairs!(VirtualNode, VirtualNodeType;
        VirtualNode::Aggregated(Default::default()),
        VirtualNode::AggregatedStorage(Default::default()),
        VirtualNode::VirtualStorage(Default::default()),
        VirtualNode::Placeholder(Default::default()),
    );
    check_all_into_type_pairs!(TimeSeries, TimeSeriesType;
        TimeSeries::Pandas(Default::default()),
        TimeSeries::Polars(Default::default()),
        TimeSeries::Python(Default::default()),
        TimeSeries::Arrow(Default::default()),
        TimeSeries::Parquet(Default::default()),
        TimeSeries::Placeholder(Default::default()),
    );
    check_all_into_type_pairs!(DataTable, DataTableType;
        DataTable::CSV(Default::default()),
        DataTable::Placeholder(Default::default()),
    );
    check_all_into_type_pairs!(Output, OutputType;
        Output::ArrowStream(Default::default()),
        Output::CSV(Default::default()),
        Output::HDF5(Default::default()),
        Output::Memory(Box::default()),
        Output::Placeholder(Default::default()),
    );
    check_all_into_type_pairs!(Parameter, ParameterType;
        Parameter::Aggregated(Default::default()),
        Parameter::AggregatedIndex(Default::default()),
        Parameter::AsymmetricSwitchIndex(Default::default()),
        Parameter::Constant(Default::default()),
        Parameter::ConstantScenario(Default::default()),
        Parameter::ControlCurvePiecewiseInterpolated(Default::default()),
        Parameter::ControlCurveInterpolated(Default::default()),
        Parameter::ControlCurveIndex(Default::default()),
        Parameter::ControlCurve(Default::default()),
        Parameter::DailyProfile(Default::default()),
        Parameter::IndexedArray(Default::default()),
        Parameter::MonthlyProfile(Default::default()),
        Parameter::WeeklyProfile(Default::default()),
        Parameter::UniformDrawdownProfile(Default::default()),
        Parameter::Max(Default::default()),
        Parameter::Min(Default::default()),
        Parameter::MultiThreshold(Default::default()),
        Parameter::Negative(Default::default()),
        Parameter::NegativeMax(Default::default()),
        Parameter::NegativeMin(Default::default()),
        Parameter::HydropowerTarget(Box::default()),
        Parameter::Polynomial1D(Default::default()),
        Parameter::Threshold(Default::default()),
        Parameter::TablesArray(Default::default()),
        Parameter::Python(Default::default()),
        Parameter::Delay(Default::default()),
        Parameter::DelayIndex(Default::default()),
        Parameter::Division(Default::default()),
        Parameter::Difference(Default::default()),
        Parameter::Offset(Default::default()),
        Parameter::DiscountFactor(Default::default()),
        Parameter::Interpolated(Default::default()),
        Parameter::RbfProfile(Default::default()),
        Parameter::Rolling(Default::default()),
        Parameter::RollingIndex(Default::default()),
        Parameter::Placeholder(Default::default()),
        Parameter::DiurnalProfile(Default::default()),
    );
}

#[test]
fn converts_parameters() {
    let mut source = PlaceholderParameter::default();
    source.meta.name = "parameter".into();
    let delay: DelayParameter = source.into();
    assert_eq!(delay.meta.name, "parameter");
    assert_eq!(delay.delay, DEFAULT_DELAY);

    let hydro: HydropowerTargetParameter = delay.into();
    assert_eq!(hydro.meta.name, "parameter");

    let placeholder: PlaceholderParameter = hydro.into();
    assert_eq!(placeholder.meta.name, "parameter");

    let mut maximum = MaxParameter::default();
    maximum.meta.name = "shared".into();
    maximum.phase = ParameterPhase::After;
    maximum.threshold = Some(7.0);
    let minimum: MinParameter = maximum.into();
    assert_eq!(minimum.meta.name, "shared");
    assert_eq!(minimum.phase, ParameterPhase::After);
    assert_eq!(minimum.threshold, Some(7.0));
}

#[test]
fn converts_virtual_nodes() {
    let mut source = AggregatedNode::default();
    source.meta.name = "virtual".into();
    let storage: VirtualStorageNode = source.into();
    assert_eq!(storage.meta.name, "virtual");

    let placeholder: PlaceholderNode = storage.into();
    assert_eq!(placeholder.meta.name, "virtual");
}

#[test]
fn converts_time_series() {
    let mut source = PandasTimeSeries::default();
    source.meta.name = "data".into();
    source.path = "data.csv".into();
    let arrow: ArrowTimeSeries = source.into();
    assert_eq!(arrow.meta.name, "data");
    assert_eq!(arrow.path.to_str(), Some("data.csv"));

    let placeholder: PlaceholderTimeSeries = arrow.into();
    assert_eq!(placeholder.meta.name, "data");
}

#[test]
fn converts_tables() {
    let mut source = PlaceholderTable::default();
    source.meta.name = "table".into();
    let csv: CsvDataTable = source.into();
    assert_eq!(csv.meta.name, "table");
    assert!(matches!(csv.ty, DataTableValueType::Scalar));
    assert!(matches!(csv.lookup, CsvDataTableLookup::Row { cols: 1 }));
}

#[test]
fn converts_outputs() {
    let mut source = MemoryOutput::default();
    source.meta.name = "output".into();
    source.metric_set = "metrics".into();
    let arrow: ArrowStreamOutput = source.into();
    assert_eq!(arrow.meta.name, "output");
    assert_eq!(arrow.metric_set, "metrics");
    assert_eq!(arrow.batch_size.get(), 1);

    let placeholder: PlaceholderOutput = arrow.into();
    assert_eq!(placeholder.meta.name, "output");
}
