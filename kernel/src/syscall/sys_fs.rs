use alloc::string::String;
use alloc::vec::Vec;
use core::ops::Add;
use crate::error_sync;
use crate::fs::dfile::{DFile, DirEntryWrapper};
use crate::fs::fat::get_fatfs;
use crate::fs::fcntl::{AT_FDCWD, OpenFlags, OpenMode};
use crate::fs::{get_dentry_from_dir};
use crate::mm::addr::Vaddr;
use crate::trap::TrapFrame;
use crate::utils::convert_cstr_from_vaddr;
use super::*;

pub fn syscall_fs_entry(tf:&mut TrapFrame, syscall_id:usize){
    let ret = match syscall_id {
        SYSCALL_OPENAT => {
            sys_openat(tf.arg0() as isize,
                       convert_cstr_from_vaddr(Vaddr(tf.arg1())),
                       {
                           match OpenFlags::from_bits_checked(tf.arg2() as u32){
                               None => {
                                   tf.err();
                                   return;
                               }
                               Some(v) => {
                                   v
                               }
                           }
                       },
                       OpenMode::from_bits(tf.arg3() as u32).unwrap())
        }
        SYSCALL_SENDFILE => {
            sys_sendfile(tf.arg0() as isize,tf.arg1() as isize,tf.arg2() as *const usize,tf.arg3())
        }
        _ => {
            error_sync!("fs syscall {} not register",syscall_id);
            -1
        }
    };
    tf.ret(ret as usize);
}

fn sys_openat(dirfd:isize,filename:String,flags:OpenFlags,mode:OpenMode)->isize{
    let running = get_running();
    let mut tsk = running.lock_irq().unwrap();
    let pwd_dir = if dirfd==AT_FDCWD{
        match tsk.pwd_dfile.as_ref().unwrap().open_path(&filename){
            None => {
                return -1;
            }
            Some(v) => {

            }
        }
    } else {
        fs_lock.root_dir()
    };
    let wrapper = get_dentry_from_dir(pwd_dir, &filename);
    match wrapper {
        None => {
            return -1;
        }
        Some(v) => {
            if v.is_dir(){
                return -1;
            }
            let mut s = String::from(tsk.pwd_ref());
            s.push('/');
            let ss = s+&filename;
            match tsk.alloc_opend(Arc::new(OldDFile::new_file(ss))) {
                None => {
                    return -1;
                }
                Some(fd) => {
                    return fd as isize;
                }
            }
        }
    }
}

const SENDFILE_BUF_LEN:usize = 10;
fn sys_sendfile(out_fd:isize,in_fd:isize,offset:*const usize,count:usize)->isize{
    let running = get_running();
    let tsk = running.lock_irq().unwrap();
    if out_fd < 0 || in_fd < 0 {
        return -1;
    }
    let out_file = match tsk.get_opened(out_fd as usize){
        None => {
            return -1;
        }
        Some(v) => {
            v
        }
    };
    let in_file = match tsk.get_opened(in_fd as usize){
        None => {
            return -1;
        }
        Some(v) => {
            v
        }
    };
    let mut buf = [0u8;SENDFILE_BUF_LEN];
    -1
}