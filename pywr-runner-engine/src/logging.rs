use crate::event::{LogLevel, LogRecord};
use log::{Level, LevelFilter, Log, Metadata, Record};
use std::cell::RefCell;
use std::sync::{OnceLock, mpsc::Sender};

thread_local! {
    static CAPTURE: RefCell<Option<(LogLevel, Sender<LogRecord>)>> = const { RefCell::new(None) };
}

struct RunnerLogger;

impl Log for RunnerLogger {
    fn enabled(&self, metadata: &Metadata<'_>) -> bool {
        metadata.level() <= Level::Debug
    }

    fn log(&self, record: &Record<'_>) {
        if !self.enabled(record.metadata()) {
            return;
        }

        let captured = CAPTURE.with(|capture| {
            let capture = capture.borrow();
            let Some((minimum_level, sender)) = capture.as_ref() else {
                return false;
            };
            let level = match record.level() {
                Level::Error => LogLevel::Error,
                Level::Warn => LogLevel::Warn,
                Level::Info => LogLevel::Info,
                Level::Debug | Level::Trace => LogLevel::Debug,
            };
            if level_enabled(level, *minimum_level) {
                let _ = sender.send(LogRecord {
                    level,
                    message: record.args().to_string(),
                });
            }
            true
        });

        if !captured {
            eprintln!("{} {}", record.level(), record.args());
        }
    }

    fn flush(&self) {}
}

static LOGGER: RunnerLogger = RunnerLogger;
static LOGGER_INSTALLED: OnceLock<bool> = OnceLock::new();

/// Install the log facade bridge used to forward model and engine diagnostics
/// to the active runner request. Returns false when an application installed a
/// logger before the runner, in which case the existing logger is preserved.
pub fn install_log_router() -> bool {
    *LOGGER_INSTALLED.get_or_init(|| {
        if log::set_logger(&LOGGER).is_ok() {
            log::set_max_level(LevelFilter::Debug);
            true
        } else {
            false
        }
    })
}

pub(crate) fn capture_logs<T>(level: LogLevel, sender: Sender<LogRecord>, action: impl FnOnce() -> T) -> T {
    if !install_log_router() {
        return action();
    }

    CAPTURE.with(|capture| {
        let previous = capture.replace(Some((level, sender)));
        let result = action();
        capture.replace(previous);
        result
    })
}

fn level_enabled(level: LogLevel, minimum: LogLevel) -> bool {
    fn priority(level: LogLevel) -> u8 {
        match level {
            LogLevel::Debug => 0,
            LogLevel::Info => 1,
            LogLevel::Warn => 2,
            LogLevel::Error => 3,
        }
    }
    priority(level) >= priority(minimum)
}
