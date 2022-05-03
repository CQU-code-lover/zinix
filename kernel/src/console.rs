use crate::sbi::console_putchar;
use core::fmt::{self, Write};
use crate::sync::SpinLock;
use lazy_static::lazy_static;
struct Stdout;

impl Write for Stdout {
    fn write_str(&mut self, s: &str) -> fmt::Result {
        for c in s.chars() {
            console_putchar(c as usize);
        }
        Ok(())
    }
}

pub fn print(args: fmt::Arguments) {

}

pub fn print_to(args: fmt::Arguments, label: &str){

}

lazy_static!{
    static ref std_spinlock:SpinLock<u8> = SpinLock::new(1);
}

pub fn print_to_stdout(args: fmt::Arguments){
    let guard = std_spinlock.lock().unwrap();
    Stdout.write_fmt(args).unwrap();
}

#[macro_export]
macro_rules! print {
    ($fmt: literal $(, $($arg: tt)+)?) => {
        $crate::console::print_to_stdout(format_args!($fmt $(, $($arg)+)?));
    }
}

#[macro_export]
macro_rules! println {
    ($fmt: literal $(, $($arg: tt)+)?) => {
        $crate::console::print_to_stdout(format_args!(concat!($fmt, "\n") $(, $($arg)+)?));
    }
}
