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

        let _ = captured;
    }

    fn flush(&self) {}
}

static LOG_ROUTER: OnceLock<MultiLogger> = OnceLock::new();

/// A logger that forwards each record to every enabled child logger.
pub struct MultiLogger {
    loggers: Vec<Box<dyn Log>>,
}

impl MultiLogger {
    pub fn new(loggers: Vec<Box<dyn Log>>) -> Self {
        Self { loggers }
    }
}

impl Log for MultiLogger {
    fn enabled(&self, metadata: &Metadata<'_>) -> bool {
        self.loggers.iter().any(|logger| logger.enabled(metadata))
    }

    fn log(&self, record: &Record<'_>) {
        for logger in &self.loggers {
            if logger.enabled(record.metadata()) {
                logger.log(record);
            }
        }
    }

    fn flush(&self) {
        for logger in &self.loggers {
            logger.flush();
        }
    }
}

/// Install an application logger together with the runner's scoped capture sink.
///
/// This must be called before any other global logger is installed.
pub fn install_log_router_with(logger: Box<dyn Log>) -> Result<(), log::SetLoggerError> {
    let router = LOG_ROUTER.get_or_init(|| MultiLogger::new(vec![logger, Box::new(RunnerLogger)]));
    log::set_logger(router)?;
    // Runner requests may ask for debug records even when the application logger filters them.
    log::set_max_level(LevelFilter::Debug);
    Ok(())
}

pub(crate) fn capture_logs<T>(level: LogLevel, sender: Sender<LogRecord>, action: impl FnOnce() -> T) -> T {
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
