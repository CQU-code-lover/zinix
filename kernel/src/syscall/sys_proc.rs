use alloc::string::String;
use alloc::vec::Vec;
use core::arch::riscv64::fence_i;
use fatfs::Write;
use riscv::asm::sfence_vma_all;
use crate::fs::dfile::DFile;
use crate::fs::inode::Inode;
use crate::mm::addr::{Addr, Vaddr};
use crate::mm::vma::{MmapFlags, MmapProt, VMA};
use crate::pre::{InnerAccess, ReadWriteSingleNoOff};
use crate::{SpinLock, Task};
use crate::mm::mm::MmStruct;
use crate::task::{add_task, scheduler, wait_children, wait_for};
use crate::task::info::{CloneFlags, Utsname};
use crate::task::task::do_fork;
use crate::task::task::TaskStatus::TaskSleeping;
use crate::trap::TrapFrame;
use super::*;

pub fn syscall_proc_entry(tf:&mut TrapFrame, syscall_id:usize) {
    let ret:isize = match syscall_id {
        // todo getppid?
        // execve 需要fencei以及清空target_cow_mm!!
        SYSCALL_EXECVE=>{
            sys_execve(tf.arg0(),tf.arg1(),tf.arg2(),tf)
        }
        SYSCALL_GETTID=>{
            get_running().lock_irq().unwrap().get_tid() as isize
        }
        SYSCALL_WAIT4=>{
            sys_wait4(tf.arg0() as isize, tf.arg1(), tf.arg2() as isize)
        }
        SYSCALL_CLONE=>{
            sys_clone(tf.arg0(), tf.arg1(), tf.arg2(), tf.arg3(), tf.arg4(),tf)
        }
        SYSCALL_UNAME=>{
            sys_unmae(tf.arg0())
        }
        SYSCALL_GETPPID=>{
            sys_getppid()
        }
        SYSCALL_EXIT =>{
            sys_exit(tf.arg0() as i32)
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
        SYSCALL_SET_TID_ADDRESS=>{
            sys_set_tid_address(tf.arg0())
        }
        _ => {
            panic!("fs syscall {} not impl",syscall_id);
        }
    };
    tf.ret(ret as usize);
}

fn sys_execve(path:usize, mut argv_ptr:usize, envp_ptr:usize, tf: &mut TrapFrame) ->isize{
    let running = get_running();
    let mut this_tsk = running.lock_irq().unwrap();
    let path =convert_cstr_from_vaddr(Vaddr(path));
    let path = this_tsk.pwd_ref().clone() + &path;
    let mut argv: Vec<String> = Vec::new();
    loop {
        let arg_ptr: usize = unsafe { Vaddr(argv_ptr).read_single().unwrap() };
        if arg_ptr == 0 {
            break;
        }
        argv.push(convert_cstr_from_vaddr(Vaddr(arg_ptr)));
        argv_ptr += size_of::<usize>();
    }
    println!("{:?}",argv);
    drop(this_tsk);
    let tsk_opt = unsafe { Task::create_user_task(&path, argv) };
    this_tsk = running.lock_irq().unwrap();
    let new_tsk =  match tsk_opt{
        None => {
            panic!("execve");
        }
        Some(s) => {
            s
        }
    };
    let old_mm = this_tsk.execve_from_tsk(new_tsk.clone());
    let tff = new_tsk.lock_irq().unwrap().kernel_stack.get_end() - size_of::<TrapFrame>();
    let old_sp = tf.x2;
    unsafe { *tf = (*(tff as *const TrapFrame)).clone() }
    tf.x2 = old_sp;
    tf.sepc-=4;
    // 因为syscall返回sepc会+4
    unsafe { this_tsk.install_pagetable(); }
    unsafe { fence_i(); }
    0
}

fn sys_exit(exit_code:i32)->isize{
    let t = get_running().lock_irq().unwrap().get_tid();
    info_sync!("EXIT:tid {}",t);
    exit_self(exit_code);
    assert!(false);
    0
}

fn sys_wait4(pid: isize, wstatus: usize, option: isize)->isize{
    assert_eq!(option, 0);
    let ptid = get_running().lock_irq().unwrap().get_tid();
    info_sync!("tid {} wait for tid{} SLEEPING",ptid,pid);
    let (exit_code,tid)= if pid!=-1{
        wait_children(ptid)
    } else {
        wait_for(pid as usize)
    };
    info_sync!("tid {} wait for tid{} WAKE",ptid,pid);
    unsafe { Vaddr(wstatus).write_single(exit_code).unwrap(); }
    tid as isize
}

fn sys_set_tid_address(tidptr:usize)->isize {
    let running = get_running();
    let mut tsk = running.lock_irq().unwrap();
    tsk.clear_child_tid = tidptr;
    tsk.get_tid() as isize
}

fn sys_clone(flags: usize, stack_ptr: usize, ptid: usize, ctid: usize, newtls: usize,tf:&TrapFrame)->isize{
    let clone_flags = unsafe {CloneFlags::from_bits_unchecked(flags)};
    if clone_flags.contains(CloneFlags::CLONE_VM) || clone_flags.contains(CloneFlags::CLONE_THREAD) {
        todo!()
    }
    let mut new_tf = tf.clone();
    new_tf.x10 = 0;
    // for syscall return
    new_tf.sepc +=4;
    let running = get_running();
    let mut new_task = do_fork(running.clone(),new_tf);

    if clone_flags.contains(CloneFlags::CLONE_CHILD_SETTID) && ctid != 0{
        new_task.set_child_tid = ctid;
        unsafe { Vaddr(ctid).write_single(new_task.get_tid() as i32).unwrap()};
    }
    if clone_flags.contains(CloneFlags::CLONE_CHILD_CLEARTID) && ctid != 0{
        new_task.clear_child_tid = ctid;
    }
    if !clone_flags.contains(CloneFlags::SIGCHLD){
        panic!("sys_fork: FLAG not supported!");
        return -1;
    }
    if stack_ptr!=0{
        panic!("not support");
    }
    //todo 可以省略这步吗？
    unsafe {
        sfence_vma_all();
        fence_i();
    }
    let ret = new_task.get_tid() as isize;
    info_sync!("create tid:{},user",new_task.get_tid());

    add_task(Arc::new(SpinLock::new(new_task)));
    ret
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
    let mut mm = tsk.mm.as_mut().unwrap().lock_irq().unwrap();
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
    let mut mm = tsk.mm.as_mut().unwrap().lock_irq().unwrap();
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