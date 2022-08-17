use fatfs::Write;
use crate::fs::dfile::DFile;
use crate::fs::inode::Inode;
use crate::mm::addr::{Addr, Vaddr};
use crate::mm::vma::{MmapFlags, MmapProt, VMA};
use crate::pre::InnerAccess;
use crate::{SpinLock, Task};
use crate::task::info::Utsname;
use crate::trap::TrapFrame;
use super::*;

pub fn syscall_proc_entry(tf:&mut TrapFrame, syscall_id:usize) {
    let ret:isize = match syscall_id {
        // todo getppid?
        SYSCALL_UNAME=>{
            sys_unmae(tf.arg0())
        }
        SYSCALL_GETPPID=>{
            sys_getppid()
        }
        SYSCALL_GETPID=>{
            sys_getpid()
        }
        SYSCALL_BRK =>{
            sys_brk(tf.arg0())
        }
        SYSCALL_MMAP =>{
            let prot = unsafe{MmapProt::from_bits_unchecked(tf.arg2())};
            let flags = unsafe{MmapFlags::from_bits_unchecked(tf.arg3())};
            let ret = sys_mmap(tf.arg0(),tf.arg1(),prot,flags,tf.arg4(),tf.arg5());
            // trace
            trace_sync!("sys_mmap:vaddr:{:#X},len:{},prot:{:b},flags:{},fd:{},off:{},anon:{},ret:{:#X}",
            tf.arg0(),tf.arg1(),prot.bits(),flags.bits(),tf.arg4(),tf.arg5(),flags.contains(MmapFlags::MAP_ANONYMOUS)
            ,ret as usize);
            ret
        }
        SYSCALL_GETCWD =>{
            let ret = sys_getcwd(tf.arg0(),tf.arg1());
            trace_sync!("sys_getcwd:buf_addr:{:#X},len:{},ret:{}",tf.arg0(),tf.arg1(),ret);
            ret
        }
        _ => {
            panic!("fs syscall {} not impl",syscall_id);
        }
    };
    tf.ret(ret as usize);
}

fn sys_getcwd(buf:usize,len:usize)->isize{
    if buf==0{
        return -1;
    }
    let mut s = get_running().lock_irq().unwrap().pwd_ref().clone();
    s.push('\0');
    if s.len()>len{
        return -1;
    }
    assert_eq!(Vaddr(buf).write(s.as_bytes()).unwrap(),s.len());
    0
}

fn sys_unmae(buf:usize)->isize{
    let uname = Utsname::new();
    Vaddr(buf).write(uname.as_bytes()).unwrap();
    0
}

fn sys_getppid()->isize{
    let p = get_running().lock_irq().unwrap().get_parent();
    match p {
        None => {
            -1
        }
        Some(pp) => {
            pp.lock_irq().unwrap().get_tgid() as isize
        }
    }
}

fn sys_getpid()->isize{
    get_running().lock_irq().unwrap().get_tgid() as isize
}

fn sys_mmap(va:usize,len:usize,prot:MmapProt,flags:MmapFlags,fd:usize,offset:usize)->isize{
    let vaddr = if va!=0{
        Some(Vaddr(va))
    } else {
        None
    };
    let running = get_running();
    let mut tsk = running.lock_irq().unwrap();
    let fd_open_ret = tsk.get_opened(fd);
    let mm = tsk.mm.as_mut().unwrap();
    if flags.contains(MmapFlags::MAP_ANONYMOUS) {
        let anon_mmap_ret = mm.alloc_mmap_anon(vaddr,len,flags,prot);
        return match anon_mmap_ret {
            None => {
                -1
            }
            Some(v) => {
                let ret = v.get_start_vaddr().get_inner();
                // ok
                mm._insert_no_check(v);
                ret as isize
            }
        }
    } else {
        // file map
        let inode =  match fd_open_ret{
            None => {
                return -1;
            }
            Some(file) => {
                match file.clone_inode(){
                    None => {
                        return -1;
                    }
                    Some(inode) => {
                        if inode.is_dir() {
                            return -1;
                        }
                        inode
                    }
                }
            }
        };

        let file_mmap_ret = mm.alloc_mmap_file(vaddr,len,
                                               inode.clone(),
                                               offset,
                                               min(len,inode.get_dentry().len() as usize),
                                               flags,
                                               prot);
        return match file_mmap_ret {
            None => {
                -1
            }
            Some(v) => {
                let ret = v.get_start_vaddr().get_inner();
                mm._insert_no_check(v);
                ret as isize
            }
        }
    }
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
    trace_sync!("BRK arg {} ret {}",brk,ret);
    ret as isize
}