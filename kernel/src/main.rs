#![no_std]
#![no_main]
#![feature(global_asm)]
#![feature(asm)]
#![feature(panic_info_message)]
#![feature(generator_trait)]
#![feature(trace_macros)]
#![feature(alloc_error_handler)]
//trace_macros!(true);

#[macro_use]
extern crate lazy_static;
extern crate alloc;


use core::ops::Generator;
use log::{error, info, LevelFilter, warn};
use riscv::register::{sie, stvec, sstatus};
use riscv::register::mtvec::TrapMode;
use crate::logger::early_logger_init;
use crate::mm::{mm_init, UnitTest};
use crate::sync::SpinLock;
use crate::timer::set_next_trigger;
use buddy_system_allocator::LockedHeap;

#[macro_use]

mod compile;
mod sbi;
mod console;
mod trap;
mod timer;
mod consts;
mod logger;
mod sync;
mod mm;

global_asm!(include_str!("entry.asm"));

#[no_mangle]
fn start_kernel(cpu:usize,devtree:usize) {
    extern "C" { fn trap_entry(); }
    unsafe {
        stvec::write(trap_entry as usize, TrapMode::Direct);
    }
    let lock = SpinLock::new(1);
    {
        let grd = lock.lock().unwrap();
    }
    let grd2 = lock.lock().unwrap();
    early_logger_init();
    log::set_max_level(LevelFilter::Trace);
    info!("Start CPU {}",cpu);
    info!("info");
    warn!("warn");
    println!("{:x}",stvec::read().bits());
    //set_next_trigger();
    mm_init();
    UnitTest();
    loop {

    }
    panic!("kernel exit!");
}
