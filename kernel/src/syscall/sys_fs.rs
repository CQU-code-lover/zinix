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
                           match OpenFlags::from_bits(tf.arg2() as u32){
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
        SYSCALL_WRITEV => {
            sys_writev(tf.arg0() as isize,tf.arg1(),tf.arg2())
        }
        SYSCALL_WRITE=>{
            sys_write(tf.arg0() as isize,tf.arg1(),tf.arg2())
        }
        SYSCALL_DUP=>{
            do_dup(tf.arg0() as isize,None,None)
        }
        SYSCALL_DUP3=>{
            do_dup(tf.arg0() as isize,Some(tf.arg1() as isize),Some(tf.arg2()))
        }
        SYSCALL_READ=>{
            sys_read(tf.arg0() as isize,tf.arg1(),tf.arg2())
        }
        _ => {
            panic!("fs syscall {} not impl",syscall_id);
        }
    };
    tf.ret(ret as usize);
}

fn do_dup(old_fd:isize,new_fd:Option<isize>,open_flags_bits:Option<usize>)->isize{
    let mut running = get_running();
    let mut tsk = running.lock_irq().unwrap();
    match tsk.get_opened(old_fd as usize) {
        None => {
            return -1;
        }
        Some(f) => {
            match new_fd {
                None => {
                    match tsk.alloc_opend(f) {
                        None => {
                            return -1;
                        }
                        Some(fd) => {
                            return fd as isize;
                        }
                    }
                }
                Some(newfd) => {
                    if open_flags_bits.is_some() {
                        // dup3 todo
                    }
                    match tsk.set_opend(newfd as usize,Some(f)){
                        Ok(_) => {
                            return newfd;
                        }
                        Err(_) => {
                            return -1;
                        }
                    }
                }
            }
        }
    }
}

fn sys_openat(dirfd:isize,filename:String,flags:OpenFlags,mode:OpenMode)->isize{
    let running = get_running();
    let mut tsk = running.lock_irq().unwrap();
    let dir_dfile = if dirfd==AT_FDCWD {
        tsk.get_pwd_opend()
    } else {
        match tsk.get_opened(dirfd as usize){
            None => {
                return -1;
            }
            Some(f) => {f}
        }
    };
    match dir_dfile.open_path(&filename,flags){
        None => {
            return -1;
        }
        Some(new_file) => {
            match tsk.alloc_opend(Arc::new(new_file)){
                None => {
                    return -1;
                }
                Some(v) => {
                    return v as isize;
                }
            }
        }
    }
}

const SENDFILE_BUF_LEN:usize = 10;
fn sys_sendfile(out_fd:isize,in_fd:isize,offset:*const usize,count:usize)->isize{
    let running = get_running();
    let mut tsk = running.lock_irq().unwrap();
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

fn sys_write(fd:isize,ptr:usize,len:usize)->isize{
    let buf = slice_from_raw_parts(ptr as *const u8,len);
    let running = get_running();
    let mut tsk = running.lock_irq().unwrap();
    match tsk.get_opened(fd as usize){
        None => {
            return -1;
        }
        Some(f) => {
            unsafe {
                match f.write(&(*buf)[..]) {
                    Ok(l) => {
                        return l as isize;
                    }
                    Err(_) => {
                        return -1;
                    }
                }
            }
        }
    }
}

fn sys_read(fd:isize,ptr:usize,len:usize)->isize{
    let buf = slice_from_raw_parts_mut(ptr as *mut u8,len);
    let running = get_running();
    let mut tsk = running.lock_irq().unwrap();
    match tsk.get_opened(fd as usize){
        None => {
            return -1;
        }
        Some(f) => {
            unsafe {
                match f.read(&mut (*buf)[..]) {
                    Ok(l) => {
                        return l as isize;
                    }
                    Err(_) => {
                        return -1;
                    }
                }
            }
        }
    }
}

// todo使用write all
fn sys_writev(fd:isize,iov_array_base:usize,len:usize)->isize{
    #[repr(C)]
    #[derive(Copy,Clone)]
    struct IOVEC{
        iov_base:*mut u8,
        iov_len:usize
    }
    let mut len_res = len;
    let len_need_read = len_res;
    let mut file = match get_running().lock_irq().unwrap().get_opened(fd as usize){
        None => {
            return -1;
        }
        Some(f) => {f}
    };
    for i in 0..usize::MAX {
        if len_res <=0{
            break;
        }
        let iov = unsafe{*((iov_array_base + i*size_of::<IOVEC>()) as *mut IOVEC)};
        let real_write_len = min(iov.iov_len, len_res);
        let write_buf= unsafe{slice_from_raw_parts(iov.iov_base, real_write_len).as_ref().unwrap()};
        assert_eq!(match file.write(write_buf){
            Ok(l) => {l}
            Err(_) => {
                return -1;
            }
        }, real_write_len);
        len_res -= real_write_len;
    }
    len_need_read as isize
}