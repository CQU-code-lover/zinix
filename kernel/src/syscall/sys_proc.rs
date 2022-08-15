use crate::trap::TrapFrame;
use super::*;

pub fn syscall_proc_entry(tf:&mut TrapFrame, syscall_id:usize) {
    let ret:isize = match syscall_id {
        SYSCALL_BRK =>{
            -1
        }
        _ => {
            panic!("fs syscall {} not impl",syscall_id);
        }
    };
    tf.ret(ret as usize);
}

// fn sys_brk()->isize{
//
// }