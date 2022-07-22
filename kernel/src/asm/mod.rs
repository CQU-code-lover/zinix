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

reg_fn!(r_ra,w_ra,"ra");
reg_fn!(r_tp,w_tp,"tp");

pub fn r_sp()->usize{
    let sp:usize = 0;
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