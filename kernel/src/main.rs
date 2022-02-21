#![no_std]
#![no_main]
#![feature(global_asm)]
#![feature(asm)]
#![feature(panic_info_message)]
#[macro_use]

mod compile;

fn start_kernel() {
    panic!("out");
}
