#![no_std]
#![no_main]
#![feature(panic_info_message)]
#![feature(generator_trait)]
#![feature(trace_macros)]
#![feature(alloc_error_handler)]
#![feature(linked_list_remove)]
#![feature(default_free_fn)]
#![feature(linked_list_cursors)]
#![feature(step_trait)]
#![feature(mixed_integer_ops)]
#![allow(unused_imports)]
#![allow(unused_variables)]
#![allow(unused)]
//trace_macros!(true);

extern crate alloc;
#[macro_use]
extern crate bitflags;
#[macro_use]
extern crate lazy_static;

use core::arch::global_asm;
use core::ops::Generator;

use buddy_system_allocator::LockedHeap;
use log::{debug, error, info, LevelFilter, trace, warn};
use riscv::register::{sie, sstatus, stvec};
use riscv::register::mie::read;
use riscv::register::mtvec::TrapMode;
use riscv::register::sstatus::Sstatus;
use crate::asm::r_sstatus;
use crate::fs::fat::fat_init;

use crate::logger::early_logger_init;
use crate::mm::buddy::buddy_test;
use crate::mm::mm_init;
use crate::sync::cpu_local::{get_core_id, set_core_id};
use crate::sync::SpinLock;
use crate::task::task::{Task, task_cpu_init};
use crate::task::task_test;
use crate::test::do_test;
use crate::trap::timer::timer_startup;
use crate::trap::trap_init;

#[macro_use]

mod compile;
mod sbi;
mod console;
mod trap;
mod consts;
mod logger;
mod sync;
mod mm;
mod utils;
mod task;
mod asm;
mod syscall;
mod fs;
mod test;
mod io;

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
    trap_init();
    mm_init();
    task_cpu_init();
    // task_test();
    timer_startup();
    fat_init();
    unsafe {
        do_test();
    }
    loop {
        // debug_sync!("MAIN");
    }
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