#![no_std]
#![no_main]
#![feature(global_asm)]
#![feature(asm)]
#![feature(panic_info_message)]
#![feature(generator_trait)]
#![feature(trace_macros)]
#![feature(alloc_error_handler)]
#![feature(linked_list_remove)]
#![feature(default_free_fn)]
#![feature(linked_list_cursors)]
#![feature(asm)]
#![allow(unused_imports)]
//trace_macros!(true);

extern crate alloc;
#[macro_use]
extern crate bitflags;
#[macro_use]
extern crate lazy_static;

use core::ops::Generator;

use buddy_system_allocator::LockedHeap;
use log::{error, info, LevelFilter, warn};
use riscv::register::{sie, sstatus, stvec};
use riscv::register::mie::read;
use riscv::register::mtvec::TrapMode;
use riscv::register::sstatus::Sstatus;

use crate::logger::early_logger_init;
use crate::mm::buddy::buddy_test;
use crate::mm::mm_init;
use crate::sync::cpu_local::{get_core_id, set_core_id};
use crate::sync::SpinLock;
use crate::timer::set_next_trigger;

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
mod asm;

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
    let p_fn = test as *const ();
    let p:fn(usize,usize) = unsafe { core::mem::transmute(p_fn) };
    println!("function:{:x}",p_fn as usize);
    buddy_test();
    // task_test();
    println!("end task test");
    loop {
    }
    panic!("kernel exit!");
}

fn test(){
    sbi::shutdown();
}