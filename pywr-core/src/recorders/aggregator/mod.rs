mod event;
mod periodic;

use event::{Event, EventAggregator, EventAggregatorState};
pub use periodic::{AggregationFrequency, AggregationFunction, PeriodValue};
use periodic::{PeriodicAggregator, PeriodicAggregatorState};

#[derive(Debug, Clone)]
pub enum AggregatorState {
    Periodic(PeriodicAggregatorState),
    Event(EventAggregatorState),
}

impl AggregatorState {
    fn as_periodic(&self) -> Option<&PeriodicAggregatorState> {
        match self {
            AggregatorState::Periodic(state) => Some(state),
            _ => None,
        }
    }

    fn as_periodic_mut(&mut self) -> Option<&mut PeriodicAggregatorState> {
        match self {
            AggregatorState::Periodic(state) => Some(state),
            _ => None,
        }
    }

    fn as_event(&self) -> Option<&EventAggregatorState> {
        match self {
            AggregatorState::Event(state) => Some(state),
            _ => None,
        }
    }

    fn as_event_mut(&mut self) -> Option<&mut EventAggregatorState> {
        match self {
            AggregatorState::Event(state) => Some(state),
            _ => None,
        }
    }
}

#[derive(Debug, Clone)]
pub struct NestedAggregatorState {
    state: AggregatorState,
    child: Option<Box<NestedAggregatorState>>,
}

pub enum AggregatorValue {
    Periodic(PeriodValue<f64>),
    Event(Event),
}

impl From<Event> for AggregatorValue {
    fn from(event: Event) -> Self {
        AggregatorValue::Event(event)
    }
}

impl From<PeriodValue<f64>> for AggregatorValue {
    fn from(value: PeriodValue<f64>) -> Self {
        AggregatorValue::Periodic(value)
    }
}

#[derive(Debug, Clone)]
pub enum Aggregator {
    Periodic(PeriodicAggregator),
    Event(EventAggregator),
}

impl From<PeriodicAggregatorState> for AggregatorState {
    fn from(state: PeriodicAggregatorState) -> Self {
        AggregatorState::Periodic(state)
    }
}

impl From<EventAggregatorState> for AggregatorState {
    fn from(state: EventAggregatorState) -> Self {
        AggregatorState::Event(state)
    }
}

impl Aggregator {
    fn setup(&self) -> AggregatorState {
        match self {
            Aggregator::Periodic(_) => PeriodicAggregatorState::default().into(),
            Aggregator::Event(_) => EventAggregatorState::default().into(),
        }
    }

    fn process_value(&self, state: &mut AggregatorState, value: PeriodValue<f64>) -> Option<AggregatorValue> {
        match self {
            Aggregator::Periodic(agg) => agg.process_value(state.as_periodic_mut().unwrap(), value).into(),
            Aggregator::Event(agg) => agg.process_value(state.as_event_mut().unwrap(), value).into(),
        }
    }

    fn calc_aggregation(&self, state: &AggregatorState) -> Option<PeriodValue<f64>> {
        match self {
            Aggregator::Periodic(agg) => agg.calc_aggregation(state.as_periodic().unwrap()),
            Aggregator::Event(_) => None,
        }
    }
}

#[derive(Clone, Debug)]
pub struct NestedAggregator {
    aggregator: Aggregator,
    child: Option<Box<NestedAggregator>>,
}

impl NestedAggregator {
    pub fn new(aggregator: Aggregator, child: Option<NestedAggregator>) -> Self {
        Self {
            aggregator,
            child: child.map(Box::new),
        }
    }

    /// Create the initial default state for the aggregator.
    pub fn setup(&self) -> NestedAggregatorState {
        NestedAggregatorState {
            state: self.aggregator.setup(),
            child: self.child.as_ref().map(|c| Box::new(c.setup())),
        }
    }

    /// Append a new value to the aggregator.
    pub fn append_value(&self, state: &mut NestedAggregatorState, value: AggregatorValue) -> Option<PeriodValue<f64>> {
        let agg_value = match (&self.child, state.child.as_mut()) {
            (Some(child), Some(child_state)) => child.append_value(child_state, value),
            (None, None) => Some(value),
            (None, Some(_)) => panic!("Aggregator state contains a child state when none is expected."),
            (Some(_), None) => panic!("Aggregator state does not contain a child state when one is expected."),
        };

        if let Some(agg_value) = agg_value {
            self.aggregator.process_value(&mut state.state, agg_value)
        } else {
            None
        }
    }

    /// Compute the final aggregation value from the current state.
    ///
    /// This will also compute the final aggregation value from the child aggregators if any exists.
    /// This includes aggregation calculations over partial or unfinished periods.
    pub fn finalise(&self, state: &mut NestedAggregatorState) -> Option<PeriodValue<f64>> {
        let final_child_value = match (&self.child, state.child.as_mut()) {
            (Some(child), Some(child_state)) => child.finalise(child_state),
            (None, None) => None,
            (None, Some(_)) => panic!("Aggregator state contains a child state when none is expected."),
            (Some(_), None) => panic!("Aggregator state does not contain a child state when one is expected."),
        };

        // If there is a final value from the child aggregator then process it
        if let Some(final_child_value) = final_child_value {
            let _ = self.aggregator.process_value(&mut state.state, final_child_value);
        }

        // Finally, compute the aggregation of the current state
        self.aggregator.calc_aggregation(&state.state)
    }
}

#[cfg(test)]
mod tests {
    use super::{AggregationFrequency, AggregationFunction, Aggregator, NestedAggregator, PeriodicAggregator};
    use crate::recorders::aggregator::PeriodValue;
    use chrono::{Datelike, NaiveDate, TimeDelta};
    use float_cmp::assert_approx_eq;

    #[test]
    fn test_nested_aggregator() {
        let model_agg = PeriodicAggregator::new(None, AggregationFunction::Max);

        let annual_agg = PeriodicAggregator::new(Some(AggregationFrequency::Annual), AggregationFunction::Min);

        // Setup an aggregator to calculate the max of the annual minimum values
        let max_annual_min = NestedAggregator {
            aggregator: Aggregator::Periodic(model_agg),
            child: Some(Box::new(NestedAggregator {
                aggregator: Aggregator::Periodic(annual_agg),
                child: None,
            })),
        };

        let mut state = max_annual_min.setup();

        let mut date = NaiveDate::from_ymd_opt(2023, 1, 1)
            .unwrap()
            .and_hms_opt(0, 0, 0)
            .unwrap();
        for _i in 0..365 * 3 {
            let value = PeriodValue::new(date, TimeDelta::days(1).into(), date.year() as f64);
            let _agg_value = max_annual_min.append_value(&mut state, value);
            date += TimeDelta::days(1);
        }

        let final_value = max_annual_min.finalise(&mut state);

        if let Some(final_value) = final_value {
            assert_approx_eq!(f64, final_value.value, 2025.0);
        } else {
            panic!("Final value is None!")
        }
    }
}
