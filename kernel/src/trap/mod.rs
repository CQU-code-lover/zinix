use crate::println;
global_asm!(include_str!("trap_asm.s"));

struct TrapFrame{
    sepc:usize,   //sepc
    x1:usize,   //ra
    x2:usize,   //sp--->this
    x3:usize,
    x4:usize,
    x5:usize,
    x6:usize,
    x7:usize,
    x8:usize,
    x9:usize,
    x10:usize,
    x11:usize,
    x12:usize,
    x13:usize,
    x14:usize,
    x15:usize,
    x16:usize,
    x17:usize,
    x18:usize,
    x19:usize,
    x20:usize,
    x21:usize,
    x22:usize,
    x23:usize,
    x24:usize,
    x25:usize,
    x26:usize,
    x27:usize,
    x28:usize,
    x29:usize,
    x30:usize,
    x31:usize,
    scause:usize,
    sscratch:usize,
    sstatus:usize,
}

#[no_mangle]
fn irq_handler(trap_frame:&mut TrapFrame){
    println!("{:X}",trap_frame.scause);
}

#[no_mangle]
fn exc_handler(trap_frame:&mut TrapFrame){
    println!("{:X}",trap_frame.scause);
}
