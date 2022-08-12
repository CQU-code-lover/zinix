use crate::trap::TrapFrame;

pub fn syscall_proc_entry(tf:&mut TrapFrame, syscall_id:usize) {
    let ret:isize = match syscall_id {
        _ => {
            panic!("fs syscall {} not impl",syscall_id);
        }
    };
    tf.ret(ret as usize);
}