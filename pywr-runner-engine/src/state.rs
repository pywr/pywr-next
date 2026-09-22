use crate::command::InitialiseRequest;
use crate::event::{EngineStatus, RunSummary};
use jiff::civil::DateTime;
use pywr_core::recorders::ArrowStreamCommit;
use std::sync::mpsc::Receiver;
use strum_macros::EnumDiscriminants;

#[derive(EnumDiscriminants)]
#[strum_discriminants(name(RunnerStateKind))]
pub enum RunnerState<R> {
    Initialising(InitialiseRequest),
    Ready {
        runtime: R,
        arrow_stream_commits: Option<Receiver<ArrowStreamCommit>>,
        reason: ReadyReason,
    },
    Running {
        runtime: R,
        arrow_stream_commits: Option<Receiver<ArrowStreamCommit>>,
        target: RunTarget,
    },
    Finalising {
        runtime: R,
        arrow_stream_commits: Option<Receiver<ArrowStreamCommit>>,
    },
    Pausing {
        runtime: R,
        arrow_stream_commits: Option<Receiver<ArrowStreamCommit>>,
    },
    Cancelling {
        runtime: R,
        arrow_stream_commits: Option<Receiver<ArrowStreamCommit>>,
    },
    Completed(RunSummary),
    Cancelled(RunSummary),
    Failed {
        error: String,
    },
}

impl<R> RunnerState<R> {
    pub fn status(&self) -> EngineStatus {
        match self {
            RunnerState::Initialising(_) => EngineStatus::Initialising,
            RunnerState::Ready { .. } => EngineStatus::Ready,
            RunnerState::Running { .. } => EngineStatus::Running,
            RunnerState::Pausing { .. } => EngineStatus::Pausing,
            RunnerState::Finalising { .. } => EngineStatus::Finalising,
            RunnerState::Cancelling { .. } => EngineStatus::Cancelling,
            RunnerState::Completed(_) => EngineStatus::Completed,
            RunnerState::Cancelled(_) => EngineStatus::Cancelled,
            RunnerState::Failed { .. } => EngineStatus::Failed,
        }
    }

    pub fn kind(&self) -> RunnerStateKind {
        self.into()
    }

    pub fn is_terminal(&self) -> bool {
        matches!(
            self,
            RunnerState::Completed(_) | RunnerState::Cancelled(_) | RunnerState::Failed { .. }
        )
    }

    pub fn needs_tick(&self) -> bool {
        matches!(
            self,
            RunnerState::Initialising { .. }
                | RunnerState::Running { .. }
                | RunnerState::Pausing { .. }
                | RunnerState::Cancelling { .. }
                | RunnerState::Finalising { .. }
        )
    }
}

pub enum ReadyReason {
    Initialised,
    TargetReached,
    Paused,
}

pub enum RunTarget {
    Step,
    ToEnd,
    ToDatetime(DateTime),
}
