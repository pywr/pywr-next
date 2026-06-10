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
    use crate::predicate::Predicate;
    use crate::recorders::aggregator::PeriodValue;
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

    #[test]
    fn test_event_aggregator_less_than() {
        let agg = EventAggregator {
            predicate: Predicate::LessThan,
            threshold: 2.0,
        };
        let mut state = EventAggregatorState::default();

        let start = NaiveDate::from_ymd_opt(2023, 3, 1)
            .unwrap()
            .and_hms_opt(0, 0, 0)
            .unwrap();
        let v1 = agg.process_value(&mut state, PeriodValue::new(start, TimeDelta::days(1).into(), 1.5));
        assert!(v1.is_none());

        let start2 = NaiveDate::from_ymd_opt(2023, 3, 2)
            .unwrap()
            .and_hms_opt(0, 0, 0)
            .unwrap();
        let v2 = agg.process_value(&mut state, PeriodValue::new(start2, TimeDelta::days(1).into(), 2.5));
        assert!(v2.is_some());
        let event = v2.unwrap();
        assert_eq!(event.start, start);
        assert_eq!(event.end, Some(start2));
    }

    #[test]
    fn test_multiple_events() {
        let agg = EventAggregator {
            predicate: Predicate::GreaterThan,
            threshold: 5.0,
        };
        let mut state = EventAggregatorState::default();

        let dates: Vec<_> = (0..6)
            .map(|i| {
                NaiveDate::from_ymd_opt(2023, 4, 1 + i)
                    .unwrap()
                    .and_hms_opt(0, 0, 0)
                    .unwrap()
            })
            .collect();

        // Start event
        assert!(
            agg.process_value(&mut state, PeriodValue::new(dates[0], TimeDelta::days(1).into(), 6.0))
                .is_none()
        );
        // Continue event
        assert!(
            agg.process_value(&mut state, PeriodValue::new(dates[1], TimeDelta::days(1).into(), 7.0))
                .is_none()
        );
        // End event
        let ev1 = agg.process_value(&mut state, PeriodValue::new(dates[2], TimeDelta::days(1).into(), 4.0));
        assert!(ev1.is_some());
        assert_eq!(ev1.unwrap().start, dates[0]);
        assert_eq!(ev1.unwrap().end, Some(dates[2]));

        // Start new event
        assert!(
            agg.process_value(&mut state, PeriodValue::new(dates[3], TimeDelta::days(1).into(), 8.0))
                .is_none()
        );
        // End new event
        let ev2 = agg.process_value(&mut state, PeriodValue::new(dates[4], TimeDelta::days(1).into(), 2.0));
        assert!(ev2.is_some());
        assert_eq!(ev2.unwrap().start, dates[3]);
        assert_eq!(ev2.unwrap().end, Some(dates[4]));

        // No event
        assert!(
            agg.process_value(&mut state, PeriodValue::new(dates[5], TimeDelta::days(1).into(), 1.0))
                .is_none()
        );
    }

    #[test]
    fn test_no_event_triggered() {
        let agg = EventAggregator {
            predicate: Predicate::GreaterThan,
            threshold: 10.0,
        };
        let mut state = EventAggregatorState::default();

        for i in 0..5 {
            let start = NaiveDate::from_ymd_opt(2023, 5, 1 + i)
                .unwrap()
                .and_hms_opt(0, 0, 0)
                .unwrap();
            let v = agg.process_value(&mut state, PeriodValue::new(start, TimeDelta::days(1).into(), 5.0));
            assert!(v.is_none());
        }
    }

    #[test]
    fn test_event_starts_but_never_ends() {
        let agg = EventAggregator {
            predicate: Predicate::GreaterThan,
            threshold: 2.0,
        };
        let mut state = EventAggregatorState::default();

        let start = NaiveDate::from_ymd_opt(2023, 6, 1)
            .unwrap()
            .and_hms_opt(0, 0, 0)
            .unwrap();
        assert!(
            agg.process_value(&mut state, PeriodValue::new(start, TimeDelta::days(1).into(), 3.0))
                .is_none()
        );

        let start2 = NaiveDate::from_ymd_opt(2023, 6, 2)
            .unwrap()
            .and_hms_opt(0, 0, 0)
            .unwrap();
        assert!(
            agg.process_value(&mut state, PeriodValue::new(start2, TimeDelta::days(1).into(), 4.0))
                .is_none()
        );
    }
}
