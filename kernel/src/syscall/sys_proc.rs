use crate::mm::addr::{Addr, Vaddr};
use crate::pre::InnerAccess;
use crate::trap::TrapFrame;
use super::*;

pub fn syscall_proc_entry(tf:&mut TrapFrame, syscall_id:usize) {
    let ret:isize = match syscall_id {
        SYSCALL_BRK =>{
            sys_brk(tf.arg0())
        }
        _ => {
            panic!("fs syscall {} not impl",syscall_id);
        }
    };
    tf.ret(ret as usize);
}

fn sys_brk(brk:usize)->isize{
    let running = get_running();
    let mut tsk = running.lock_irq().unwrap();
    let mm = tsk.mm.as_mut().unwrap();
    let mut ret:isize = 0;
    if brk==0{
        ret = mm.get_brk().get_inner() as isize;
    } else {
        let brk_now = mm.get_brk().get_inner();
        if brk<mm.get_start_brk().get_inner(){
            ret = -1;
        } else {
            if brk_now==brk{
                ret = brk as isize;
            } else if brk<brk_now{
                // shrink
                match mm._shrink_brk(Vaddr(brk)){
                    Ok(_) => {
                        ret = brk as isize;
                    }
                    Err(_) => {
                        ret = -1;
                    }
                }
            } else {
                //expand
                match mm._expand_brk(Vaddr(brk)) {
                    Ok(_) => {
                        ret = brk as isize;
                    }
                    Err(_) => {
                        ret = -1;
                    }
                }
            }
        }
    }
    ret as isize
}