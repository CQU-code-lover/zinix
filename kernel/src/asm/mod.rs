use alloc::string::ToString;
use crate::println;

global_asm!(include_str!("riscv.asm"));

macro_rules! read_reg_fn{
    ($asm_fn: ident, $s: tt) => {
        pub fn $asm_fn()->usize{
            let mut val:usize = 0;
            unsafe{asm!("",out($s) val);}
            val
        }
    };
}

macro_rules! write_reg_fn{
    ($asm_fn: ident, $s: tt) => {
        pub fn $asm_fn(val:usize){
            unsafe{asm!("",in($s) val);}
        }
    };
}

macro_rules! reg_fn{
    ($asm_fn_read: ident,$asm_fn_write: ident,$s: tt) => {
        read_reg_fn!($asm_fn_read,$s);
        write_reg_fn!($asm_fn_write,$s);
    };
}

macro_rules! read_csr {
    ($fn: ident, $s: tt) => {
        pub fn $fn()->usize{
            unsafe{asm!("",in($s) val);}
        }
    };
}

reg_fn!(r_ra,w_ra,"ra");

pub const SSTATUS_SPP:usize= (1 << 8);  // Previous mode, 1=Supervisor, 0=User
pub const SSTATUS_SPIE:usize= (1 << 5); // Supervisor Previous Interrupt Enable
pub const SSTATUS_UPIE:usize= (1 << 4); // User Previous Interrupt Enable
pub const SSTATUS_SIE:usize= (1 << 1);  // Supervisor Interrupt Enable
pub const SSTATUS_UIE:usize= (1 << 0);  // User Interrupt Enable

pub fn r_sstatus()->usize {
    let mut val:usize = 0;
    unsafe {
        asm!("csrr {},sstatus", out(reg) val);
    }
    val
}

pub fn r_tp()->usize{
    let mut tp:usize = 0;
    unsafe {
        asm!("mv {}, tp",out(reg) tp);
    }
    tp
}

pub fn w_tp(tp:usize){
    unsafe {
        asm!("mv tp, {}",in(reg) tp);
    }
}

pub fn r_sp()->usize{
    let mut sp:usize = 0;
    unsafe {
        asm!("mv {}, sp",out(reg) sp);
    }
    sp
}

pub fn w_sp(sp:usize){
    unsafe {
        asm!("mv sp, {}",in(reg) sp);
    }
}

extern "C" {
    fn intr_disable()->usize;
    fn intr_enable(s:usize)->usize;
}

pub fn enable_irq(v:usize){
    unsafe { intr_enable(v); }
}

pub fn disable_irq()->usize{
    unsafe { intr_disable() }
}