use core::sync::atomic::{AtomicUsize, Ordering};
use log::info;
use riscv::register::time;
use crate::info_sync;
use crate::sbi::set_timer;
use crate::task::scheduler;
use crate::trap::TrapFrame;

const TICKS_PER_SEC: usize = 100;
const MSEC_PER_SEC: usize = 1000;
const CLOCK_FREQ: usize = 12500000;
const TIC_MAX: usize = 10;

lazy_static!{
    static ref tic_counter:AtomicUsize = AtomicUsize::new(0);
}

fn tic()->bool{
    let cnt = tic_counter.fetch_add(1, Ordering::SeqCst);
    if cnt==TIC_MAX{
        tic_counter.store(0,Ordering::SeqCst);
        true
    } else{
        false
    }
}

fn get_time() -> usize {
    time::read()
}

fn get_time_ms() -> usize {
    time::read() / (CLOCK_FREQ / MSEC_PER_SEC)
}

fn set_next_trigger() {
    set_timer(get_time() + CLOCK_FREQ / TICKS_PER_SEC);
}

pub fn timer_startup(){
    set_next_trigger()
}

pub fn timer_entry(trap_frame:&mut TrapFrame){
    set_next_trigger();
    if tic() {
        scheduler(None);
    }
}