use crate::command::InitialiseRequest;
use crate::event::{EngineStatus, RunSummary};
use jiff::civil::DateTime;
use pywr_core::recorders::SnapshotBuffer;
use strum_macros::EnumDiscriminants;

#[derive(EnumDiscriminants)]
#[strum_discriminants(name(RunnerStateKind))]
pub enum RunnerState<R> {
    Initialising(InitialiseRequest),
    Ready {
        runtime: R,
        snapshots: Option<SnapshotBuffer>,
        reason: ReadyReason,
    },
    Running {
        runtime: R,
        snapshots: Option<SnapshotBuffer>,
        target: RunTarget,
    },
    Finalising {
        runtime: R,
        snapshots: Option<SnapshotBuffer>,
    },
    Pausing {
        runtime: R,
        snapshots: Option<SnapshotBuffer>,
    },
    Cancelling {
        runtime: R,
    },
    Completed(RunSummary),
    Cancelled(RunSummary),
    Failed {
        error: String,
        summary: RunSummary,
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
