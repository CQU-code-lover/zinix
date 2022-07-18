#![no_std]
#![no_main]
#![feature(global_asm)]
#![feature(asm)]
#![feature(panic_info_message)]
#![feature(generator_trait)]
#![feature(trace_macros)]
#![feature(alloc_error_handler)]
#![feature(linked_list_remove)]
//trace_macros!(true);

#[macro_use]
extern crate lazy_static;
extern crate alloc;

#[macro_use]
extern crate bitflags;

use core::ops::Generator;
use log::{error, info, LevelFilter, warn};
use riscv::register::{sie, stvec, sstatus};
use riscv::register::mtvec::TrapMode;
use crate::logger::early_logger_init;
use crate::mm::{mm_init, MmUnitTest};
use crate::sync::SpinLock;
use crate::timer::set_next_trigger;
use buddy_system_allocator::LockedHeap;
use riscv::register::sstatus::Sstatus;
use crate::mm::buddy::buddy_test;
use crate::sync::cpu_local::{get_core_id, set_core_id};
use crate::task::task_test;

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
mod utils;
mod task;

global_asm!(include_str!("entry.asm"));

#[no_mangle]
fn start_kernel(cpu:usize,dev_tree:usize) {
    set_core_id(cpu);
    extern "C" { fn trap_entry(); }
    unsafe {
        stvec::write(trap_entry as usize, TrapMode::Direct);
        let s = sstatus::read();
        sstatus::set_sie();
        sie::set_stimer();
    }
    let lock = SpinLock::new(1);
    {
        let grd = lock.lock().unwrap();
    }
    let grd2 = lock.lock().unwrap();
    early_logger_init();
    log::set_max_level(LevelFilter::Trace);
    info!("info");
    warn!("warn");
    println!("{:x}",stvec::read().bits());
    println!("cpu {:?}",get_core_id());
    // set_next_trigger();
    mm_init();
    MmUnitTest();
    let p_fn = test as *const ();
    let p:fn(usize,usize) = unsafe { core::mem::transmute(p_fn) };
    println!("function:{:x}",p_fn as usize);
    buddy_test();
    task_test();
    println!("end task test");
    loop {
    }
    panic!("kernel exit!");
}

fn test(){
    sbi::shutdown();
}