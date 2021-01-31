use log::*;
use std::io;
use std::io::Write;

struct Logger;

fn loglevel_ansi_color(level: Level) -> &'static str {
    match level {
        Level::Error => "\x1B[1;31m",
        Level::Warn => "\x1B[1;33m",
        Level::Info => "\x1B[1;34m",
        Level::Debug => "\x1B[1;35m",
        Level::Trace => "\x1B[1;36m",
    }
}

#[cfg(not(debug_assertions))]
const LEVEL_FILTER: LevelFilter = LevelFilter::Info;
#[cfg(debug_assertions)]
const LEVEL_FILTER: LevelFilter = LevelFilter::Debug;

impl log::Log for Logger {
    fn enabled(&self, _metadata: &Metadata) -> bool {
        true
    }

    #[cfg(not(debug_assertions))]
    fn log(&self, record: &Record) {
        if !self.enabled(record.metadata()) {
            return;
        }

        let (mut stdin_read, mut stderr_read);

        let color = loglevel_ansi_color(record.level());
        // let mut writer = File::create("tmp").unwrap();
        let writer: &mut dyn Write = if record.level() >= Level::Warn {
            stderr_read = io::stderr();
            &mut stderr_read
        } else {
            stdin_read = io::stdout();
            &mut stdin_read
        };

        writeln!(
            writer,
            "{color}{}\x1B[0;0m - {}",
            record.level(),
            record.args(),
            color = color
        )
        .expect("Failed to write log message to stream");
    }

    #[cfg(debug_assertions)]
    fn log(&self, record: &Record) {
        if !self.enabled(record.metadata()) {
            return;
        }

        let (mut stdin_read, mut stderr_read);

        let color = loglevel_ansi_color(record.level());
        // let mut writer = File::create("tmp").unwrap();
        let writer: &mut dyn Write = if record.level() >= Level::Warn {
            stderr_read = io::stderr();
            &mut stderr_read
        } else {
            stdin_read = io::stdout();
            &mut stdin_read
        };

        writeln!(
            writer,
            "{color}{}\x1B[0;0m {}:{} - {}",
            record.level(),
            record.file().unwrap_or("unknown"),
            record.line().unwrap_or(0),
            record.args(),
            color = color
        )
        .expect("Failed to write log message to stream");
    }

    fn flush(&self) {}
}

static LOGGER: Logger = Logger;

pub fn init() {
    log::set_logger(&LOGGER)
        .map(|()| log::set_max_level(LEVEL_FILTER))
        .expect("Failed to init logger");
}
