#![no_std]
#![no_main]
#![feature(global_asm)]
#![feature(asm)]
#![feature(panic_info_message)]
#![feature(generator_trait)]

use core::ops::Generator;
use riscv::register::{sie, stvec, sstatus};
use riscv::register::mtvec::TrapMode;
use crate::timer::set_next_trigger;

#[macro_use]

mod compile;
mod sbi;
mod console;
mod trap;
mod timer;
mod consts;


global_asm!(include_str!("entry.asm"));

#[link_section=".text.entry"]
static A:char = 'A';

fn clear_bss() {
    extern "C" {
        fn sbss_clear();
        fn ebss_clear();
    }
    unsafe {
        core::slice::from_raw_parts_mut(
            sbss_clear as usize as *mut u8,
            ebss_clear as usize - sbss_clear as usize,
        ).fill(0);
    }
}

#[no_mangle]
fn start_kernel(a0:usize,a1:usize) {
    println!("Hello World!");
    println!("{:x}",a1);
    clear_bss();
    extern "C" { fn trap_entry(); }
    unsafe {
        stvec::write(trap_entry as usize, TrapMode::Direct);
    }

    println!("{:x}",stvec::read().bits());
    //set_next_trigger();
    loop {

    }
    panic!("out");
}
