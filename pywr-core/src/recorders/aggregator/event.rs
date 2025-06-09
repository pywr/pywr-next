use crate::predicate::Predicate;
use crate::recorders::aggregator::PeriodValue;
use crate::timestep::PywrDuration;
use chrono::NaiveDateTime;

#[derive(Default, Clone, Debug)]
enum EventState {
    #[default]
    Ended,
    Started(NaiveDateTime),
}

#[derive(Debug, Clone, Copy)]
pub struct Event {
    pub start: NaiveDateTime,
    pub end: Option<NaiveDateTime>,
}

impl Event {
    pub fn duration(&self) -> Option<PywrDuration> {
        self.end.map(|end| (end - self.start).into())
    }
}

#[derive(Default, Debug, Clone)]
pub struct EventAggregatorState {
    current: EventState,
}

#[derive(Debug, Clone)]
pub struct EventAggregator {
    predicate: Predicate,
    threshold: f64,
}

impl EventAggregator {
    pub fn new(predicate: Predicate, threshold: f64) -> Self {
        Self { predicate, threshold }
    }

    pub fn setup(&self) -> EventAggregatorState {
        EventAggregatorState::default()
    }

    /// Process a new value and return an event if one has completed.
    pub fn process_value(&self, current_state: &mut EventAggregatorState, value: PeriodValue<f64>) -> Option<Event> {
        let active_now = self.predicate.apply(value.value, self.threshold);

        let (new_current, event) = match (&current_state.current, active_now) {
            (EventState::Ended, true) => {
                // Start a new event
                (EventState::Started(value.start), None)
            }
            (EventState::Started(started), false) => {
                // End the current event
                let event = Event {
                    start: *started,
                    end: Some(value.start),
                };

                (EventState::Ended, Some(event))
            }
            (EventState::Started(started), true) => {
                // Continue the current event
                (EventState::Started(*started), None)
            }
            (EventState::Ended, false) => {
                // No event to continue
                (EventState::Ended, None)
            }
        };

        current_state.current = new_current;

        event
    }
}

#[cfg(test)]
mod tests {
    use super::{EventAggregator, EventAggregatorState};
    use crate::recorders::aggregator::PeriodValue;
    use crate::Predicate;
    use chrono::{NaiveDate, TimeDelta};

    #[test]
    fn test_event_aggregator() {
        let agg = EventAggregator {
            predicate: Predicate::GreaterThan,
            threshold: 1.0,
        };

        let mut state = EventAggregatorState::default();

        let start = NaiveDate::from_ymd_opt(2023, 1, 30)
            .unwrap()
            .and_hms_opt(0, 0, 0)
            .unwrap();
        let agg_value = agg.process_value(&mut state, PeriodValue::new(start, TimeDelta::days(1).into(), 3.0));
        assert!(agg_value.is_none());

        let start = NaiveDate::from_ymd_opt(2023, 1, 31)
            .unwrap()
            .and_hms_opt(0, 0, 0)
            .unwrap();
        let agg_value = agg.process_value(&mut state, PeriodValue::new(start, TimeDelta::days(1).into(), 3.0));
        assert!(agg_value.is_none());

        let start = NaiveDate::from_ymd_opt(2023, 2, 1)
            .unwrap()
            .and_hms_opt(0, 0, 0)
            .unwrap();
        let agg_value = agg.process_value(&mut state, PeriodValue::new(start, TimeDelta::days(1).into(), 1.0));
        assert!(agg_value.is_some());

        let start = NaiveDate::from_ymd_opt(2023, 2, 2)
            .unwrap()
            .and_hms_opt(0, 0, 0)
            .unwrap();
        let agg_value = agg.process_value(&mut state, PeriodValue::new(start, TimeDelta::days(1).into(), 1.0));
        assert!(agg_value.is_none());
    }
}
