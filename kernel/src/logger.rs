// 支持彩色打印以及日志存储的logger

use core::fmt;
use core::fmt::Write;
use log::{Record, Level, Metadata, LevelFilter, debug, warn, error, trace};
use log::Level::Error;
use crate::console::{print, print_to_stdout};
use crate::logger::LinuxShellColor::{Black, Blue, Green, Red, Yellow};
use crate::{println, SpinLock};
use crate::sbi::shutdown;
use crate::sync::SpinLockGuard;

lazy_static!{
    static ref log_lock:SpinLock<u8> = SpinLock::new(0);
}

pub fn log_sync_lock<'a>() ->SpinLockGuard<'a,u8>{
    log_lock.lock_irq().unwrap()
}

// Add escape sequence to print with color in Linux console
macro_rules! with_color {
    ($args: ident, $color_code: ident) => {{
        format_args!("\u{1B}[{}m{}\u{1B}[0m", $color_code as u8, $args)
    }};
}

enum LinuxShellColor {
    Red,
    Yellow,
    Blue,
    Green,
    Black
}

fn linux_shell_color_2_u8(color:LinuxShellColor)->u8{
    match color {
        LinuxShellColor::Red=>31,
        LinuxShellColor::Yellow=>93,
        LinuxShellColor::Blue=>34,
        LinuxShellColor::Green=>32,
        LinuxShellColor::Black=>90,
    }
}

fn log_level_2_linux_shell_color(level:Level)->Option<LinuxShellColor>{
    match level {
        Level::Error => Some(Red),
        Level::Warn => Some(Yellow),
        Level::Info => Some(Blue),
        Level::Debug => Some(Green),
        Level::Trace => Some(Black),
        _ => None
    }
}

//这个logger用于没有完成内存初始化时使用，此时无法使用heap内存分配
struct EarlyLogger;

fn early_logger_error_exit(){
    shutdown();
}

impl log::Log for EarlyLogger {
    fn enabled(&self, metadata: &Metadata) -> bool {
        metadata.level() <= Level::Trace
    }

    fn log(&self, record: &Record) {
        if self.enabled(record.metadata()) {
            let color= linux_shell_color_2_u8(log_level_2_linux_shell_color(record.level()).unwrap());
            print_to_stdout(format_args!("\u{1B}[{}m{}\u{1B}[0m", color as u8, format_args!(
                "[{:<5}] {}\n",
                record.level(),
                record.args()
            )));
            if record.level() == Error {
                early_logger_error_exit();
            }
        }
    }

    fn flush(&self) {}
}

    // info!("a {} event", "log")

#[macro_export]
macro_rules! info_sync {
    ($($arg:tt)+) => {
        {
            let g = crate::logger::log_sync_lock();
            log::info!($($arg)+)
        }
    }
}

#[macro_export]
macro_rules! debug_sync {
    ($($arg:tt)+) => {
        {
            let g = crate::logger::log_sync_lock();
            debug!($($arg)+)
        }
    }
}

#[macro_export]
macro_rules! warn_sync {
    ($($arg:tt)+) => {
        {
            let g = crate::logger::log_sync_lock();
            warn!($($arg)+)
        }
    }
}

#[macro_export]
macro_rules! trace_sync {
    ($($arg:tt)+) => {
        {
            let g = crate::logger::log_sync_lock();
            trace!($($arg)+)
        }
    }
}

#[macro_export]
macro_rules! error_sync {
    ($($arg:tt)+) => {
        {
            let g = crate::logger::log_sync_lock();
            error!($($arg)+)
        }
    }
}

pub fn early_logger_init(){
    static EARLY_LOGGER: EarlyLogger = EarlyLogger;
    log::set_logger(&EARLY_LOGGER).map(|()| log::set_max_level(LevelFilter::Info));
    #[cfg(not(feature = "debug"))]
    log::set_max_level(LevelFilter::Info);
    #[cfg(feature = "debug")]
    log::set_max_level(LevelFilter::Trace);
    // info_sync!("123");
    // debug_sync!("123");
    // trace_sync!("123");
    // error_sync!("123");
    // warn_sync!("123");
    // shutdown();
}
