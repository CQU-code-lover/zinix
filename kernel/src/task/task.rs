use alloc::string::String;
use alloc::sync::{Arc, Weak};
use alloc::vec;
use alloc::vec::{Splice, Vec};
use core::cell::RefCell;
use core::default::default;
use core::hash::Hash;
use core::mem;
use core::mem::size_of;
use core::ops::Add;
use fatfs::Read;
use log::error;
use crate::asm::{disable_irq, enable_irq, r_sp, r_sstatus, r_tp, SSTATUS_SIE, SSTATUS_SPIE, SSTATUS_SPP};
use crate::consts::{BOOT_STACK_NR_PAGES, PAGE_SIZE, STACK_MAGIC, USER_STACK_MAX_ADDR};
use crate::mm::mm::MmStruct;
use crate::mm::pagetable::PageTable;
use crate::{error_sync, println, SpinLock, trace_sync};
use crate::task::{add_task, generate_tid};
use crate::task::stack::Stack;
use riscv::register::*;
use crate::mm::aux::*;
use xmas_elf::symbol_table::Visibility::Default;
use crate::fs::dfile::{DFile, get_stdin, get_stdout};
use crate::fs::dfile::DFILE_TYPE::DFTYPE_STDIN;
use crate::fs::fat::get_fatfs;
use crate::fs::get_dentry_from_dir;
use crate::mm::{alloc_pages, get_kernel_pagetable};
use crate::mm::addr::Addr;
use crate::mm::buddy::pages2order;
use crate::sbi::shutdown;
use crate::task::task::TaskStatus::TaskRunning;
use crate::trap::TrapFrame;

const MAX_OPENED:usize = 64;

extern "C" {
    fn switch_context(cur: *const TaskContext, next: *const TaskContext);
    fn kern_trap_ret();
    fn user_trap_ret();
    fn boot_stack();
    fn boot_stack_top();
}

#[derive(Copy, Clone)]
pub enum TaskStatus {
    TaskRunning,
    TaskSleeping,
    TaskZombie,
}

struct RunningMut(Option<Arc<SpinLock<Task>>>);

impl RunningMut {
    fn new()->Self {
        RunningMut(None)
    }
    fn set(&mut self,v:Arc<SpinLock<Task>>){
        self.0 = Some(v);
    }
    fn get(&self)->Arc<SpinLock<Task>>{
        self.0.as_ref().unwrap().clone()
    }
    fn clear(&mut self)->Option<Arc<SpinLock<Task>>>{
        let ret = self.0.clone();
        self.0 = None;
        ret
    }
}

lazy_static!{
    static ref RUNNING:SpinLock<RunningMut> = SpinLock::new(RunningMut::new());
}

pub fn set_running(running:Arc<SpinLock<Task>>){
    RUNNING.lock().unwrap().set(running);
}

pub fn get_running()->Arc<SpinLock<Task>>{
    RUNNING.lock().unwrap().get()
}

pub fn RUNNING_TASK()->Arc<SpinLock<Task>>{
    get_running()
}

#[repr(C)]
pub struct TaskContext {
    ra: usize,
    //reserved by callee
    sp: usize,
    s0: usize,
    s1: usize,
    s2: usize,
    s3: usize,
    s4: usize,
    s5: usize,
    s6: usize,
    s7: usize,
    s8: usize,
    s9: usize,
    s10: usize,
    s11: usize,
    sscratch: usize,
    // point to stack
    sstatus: usize,
}

impl TaskContext {
    pub fn new()->Self {
        TaskContext {
            ra: 0,
            sp: 0,
            s0: 0,
            s1: 0,
            s2: 0,
            s3: 0,
            s4: 0,
            s5: 0,
            s6: 0,
            s7: 0,
            s8: 0,
            s9: 0,
            s10: 0,
            s11: 0,
            sscratch: 0,
            sstatus: 0
        }
    }
}

// task没有设置mut inner，为了达到可变性以及线程安全，需要以SpinLock为单位进行访问
// todo 栈溢出检测
pub struct Task {
    tid: usize,
    tgid: usize,
    kernel_stack: Stack,
    context: TaskContext,
    parent: Option<Weak<SpinLock<Task>>>,
    status: TaskStatus,
    pub mm: Option<MmStruct>,
    opened: Vec<Option<Arc<DFile>>>
}

// impl Add<usize> for Addr {
//     type Output = usize;
//
//     fn add(self, rhs: usize) -> Self::Output {
//         rhs
//     }
// }

impl Task {
    pub fn __core_init(){
        let mut addr = boot_stack as usize;
        let sp = r_sp();
        while addr<(boot_stack_top as usize) {
            if sp>addr&&sp<=addr+(PAGE_SIZE*BOOT_STACK_NR_PAGES){
                // find
                if sp-addr<=8 {
                    panic!("can`t insert stack magic for boot thread");
                } else {
                    unsafe { (addr as *mut u64).write_volatile(STACK_MAGIC); }
                    break;
                }
            }
            addr+=(PAGE_SIZE*BOOT_STACK_NR_PAGES);
        }
        let tsk = Task{
            tid: generate_tid(),
            tgid: 0,
            kernel_stack: Stack::new(true,addr,addr+(PAGE_SIZE*BOOT_STACK_NR_PAGES)),
            context: TaskContext::new(),
            parent: None,
            status: TaskStatus::TaskRunning,
            mm: None,
            opened: vec![None;MAX_OPENED]
        };
        sscratch::write(0);
        unsafe {
            #[cfg(feature = "qemu")]
            sstatus::set_sum();
        }
        let t = Arc::new(SpinLock::new(tsk));
        set_running(t.clone());
        add_task(t);
    }
    pub fn get_status(&self)->TaskStatus{
        self.status
    }
    pub fn set_status(&mut self,status :TaskStatus) {
        self.status = status;
    }
    pub fn get_opened(&self,index:usize)->Arc<DFile>{
        self.opened[index].as_ref().unwrap().clone()
    }
    pub fn is_kern(&self)->bool {
        match self.mm {
            None => true,
            Some(_)=>false
        }
    }
    pub fn is_user(&self)->bool{
        !self.is_kern()
    }
    pub fn create_kern_task(func: fn())->Self {
        let p_fn = func as *const ();
        let mut tsk = Task {
            tid: generate_tid(),
            tgid: 0,
            kernel_stack: Stack::new(false,0,0),
            context: TaskContext::new(),
            parent: None,
            status: TaskStatus::TaskRunning,
            mm: None,
            opened:vec![None;MAX_OPENED]
        };
        tsk.opened[0] = Some(get_stdin());
        tsk.opened[1] = Some(get_stdout());
        tsk.context.ra = kern_trap_ret as usize;
        unsafe { tsk.context.sp = tsk.kernel_stack.get_end() - size_of::<TrapFrame>(); }
        tsk.context.sstatus = r_sstatus()|SSTATUS_SPP|SSTATUS_SPIE|SSTATUS_SIE;
        let mut tf = TrapFrame::new_empty();
        tf.sstatus = r_sstatus()|SSTATUS_SPP|SSTATUS_SPIE;
        tf.sepc = p_fn as usize;
        tf.x2 = tsk.context.sp- size_of::<TrapFrame>();
        unsafe { tf.write_to(tsk.context.sp); }
        tsk
    }
    pub fn create_kern_task_and_run(func:fn()){
        add_task(Arc::new(SpinLock::new(Self::create_kern_task(func))))
    }
    pub unsafe fn create_user_task_and_run(path:&str,args:Vec<String>){
        let level = disable_irq();
        let fs_g = get_fatfs();
        let fs= fs_g.lock().unwrap();
        let wrapper = get_dentry_from_dir(fs.root_dir(), path).unwrap();
        let file_len = wrapper.len;
        assert!(file_len<0x400000);
        let mut f = wrapper.to_file();
        let pages = alloc_pages(pages2order((file_len/PAGE_SIZE)+1)).unwrap();
        let ptr = pages.get_pfn().get_addr_usize() as *mut [u8;0x400000];
        let read_buf = &mut *ptr;
        let mut cnt: usize = 0;
        loop {
            let read = f.read(&mut read_buf[cnt..]).unwrap();
            cnt += read;
            if read == 0 {
                break;
            }
        }
        println!("{}",cnt);
        let (mm_struct, mut auxv, entry_point) = MmStruct::new_from_elf(&read_buf[..cnt]);

        trace_sync!("New User Task: entry point={:#X}",entry_point);
        let mut tsk = Task {
            tid: generate_tid(),
            tgid: 0,
            kernel_stack: Stack::new(false,0,0),
            context: TaskContext::new(),
            parent: None,
            status: TaskStatus::TaskRunning,
            mm: Some(mm_struct),
            opened:vec![None;MAX_OPENED]
        };
        tsk.opened[0] = Some(get_stdin());
        tsk.opened[1] = Some(get_stdout());
        tsk.context.ra = user_trap_ret as usize;
        unsafe { tsk.context.sp = tsk.kernel_stack.get_end() - size_of::<TrapFrame>(); }
        tsk.context.sstatus = r_sstatus()|SSTATUS_SPIE|SSTATUS_SIE&(!SSTATUS_SPP);
        let mut tf = TrapFrame::new_empty();
        tf.sstatus = r_sstatus()|SSTATUS_SPIE&(!SSTATUS_SPP);
        tf.sepc = entry_point;
        tf.x2 = tsk.context.sp;

        // install user task pgt to access user stack
        tsk.mm.as_ref().unwrap().install_pagetable();
        let walk_ret = tsk.mm.as_ref().unwrap().pagetable.walk(0xFFFFFEE);
        let mut user_sp = USER_STACK_MAX_ADDR;
        ////////////// envp[] ///////////////////
        let mut env: Vec<String> = Vec::new();
        env.push(String::from("SHELL=/user_shell"));
        env.push(String::from("PWD=/"));
        env.push(String::from("USER=root"));
        env.push(String::from("MOTD_SHOWN=pam"));
        env.push(String::from("LANG=C.UTF-8"));
        env.push(String::from("INVOCATION_ID=e9500a871cf044d9886a157f53826684"));
        env.push(String::from("TERM=vt220"));
        env.push(String::from("SHLVL=2"));
        env.push(String::from("JOURNAL_STREAM=8:9265"));
        env.push(String::from("OLDPWD=/root"));
        env.push(String::from("_=busybox"));
        env.push(String::from("LOGNAME=root"));
        env.push(String::from("HOME=/"));
        env.push(String::from("PATH=/"));
        let mut envp: Vec<usize> = (0..=env.len()).collect();
        envp[env.len()] = 0;
        for i in 0..env.len() {
            user_sp -= env[i].len() + 1;
            envp[i] = user_sp;
            let mut p = user_sp;
            // write chars to [user_sp, user_sp + len]
            for c in env[i].as_bytes() {
                *( p as *mut u8) = *c;
                p += 1;
            }
            // str end with \0
            *(p as *mut u8) = 0;
        }
        // make the user_sp aligned to 8B for k210 platform
        user_sp -= user_sp % core::mem::size_of::<usize>();

        ////////////// argv[] ///////////////////
        let mut argv: Vec<usize> = (0..=args.len()).collect();
        argv[args.len()] = 0;
        for i in 0..args.len() {
            user_sp -= args[i].len() + 1;
            // println!("user_sp {:X}", user_sp);
            argv[i] = user_sp;
            let mut p = user_sp;
            // write chars to [user_sp, user_sp + len]
            for c in args[i].as_bytes() {
                *( p as *mut u8) = *c;
                // print!("({})",*c as char);
                p += 1;
            }
            *(p as *mut u8) = 0;
        }
        // make the user_sp aligned to 8B for k210 platform
        user_sp -= user_sp % core::mem::size_of::<usize>();

        ////////////// platform String ///////////////////
        let platform = "RISC-V64";
        user_sp -= platform.len() + 1;
        user_sp -= user_sp % core::mem::size_of::<usize>();
        let mut p = user_sp;
        for c in platform.as_bytes() {
            *( p as *mut u8) = *c;
            p += 1;
        }
        *(p as *mut u8) = 0;

        ////////////// rand bytes ///////////////////
        user_sp -= 16;
        p = user_sp;
        auxv.push(AuxHeader{aux_type: AT_RANDOM, value: user_sp});
        for i in 0..0xf {
            *( p as *mut u8) = i as u8;
            p += 1;
        }

        ////////////// padding //////////////////////
        user_sp -= user_sp % 16;

        ////////////// auxv[] //////////////////////
        auxv.push(AuxHeader{aux_type: AT_EXECFN, value: argv[0]});// file name
        // todo check auxv len
        if auxv.len()<38{
            auxv.push(AuxHeader{aux_type: AT_NULL, value:0})
        }
        // auxv.push(AuxHeader{aux_type: AT_NULL, value:0});// end
        user_sp -= auxv.len() * core::mem::size_of::<AuxHeader>();

        let auxv_base = user_sp;
        // println!("[auxv]: base 0x{:X}", auxv_base);
        for i in 0..auxv.len() {
            // println!("[auxv]: {:?}", auxv[i]);
            let addr = user_sp + core::mem::size_of::<AuxHeader>() * i;
            *( addr as *mut usize) = auxv[i].aux_type ;
            *((addr + core::mem::size_of::<usize>()) as *mut usize) = auxv[i].value ;
        }

        ////////////// *envp [] //////////////////////
        user_sp -= (env.len() + 1) * core::mem::size_of::<usize>();
        let envp_base = user_sp;
        *((user_sp + core::mem::size_of::<usize>() * (env.len())) as *mut usize) = 0;
        for i in 0..env.len() {
            *((user_sp + core::mem::size_of::<usize>() * i) as *mut usize) = envp[i] ;
        }

        ////////////// *argv [] //////////////////////
        user_sp -= (args.len() + 1) * core::mem::size_of::<usize>();
        let argv_base = user_sp;
        *((user_sp + core::mem::size_of::<usize>() * (args.len())) as *mut usize) = 0;
        for i in 0..args.len() {
            *((user_sp + core::mem::size_of::<usize>() * i) as *mut usize) = argv[i] ;
        }

        ////////////// argc //////////////////////
        user_sp -= core::mem::size_of::<usize>();
        *(user_sp as *mut usize) = args.len();

        tf.sscratch = user_sp;
        tf.x10 = args.len();
        tf.x11 = argv_base;
        tf.x12 = envp_base;
        tf.x13 = auxv_base;

        unsafe { tf.write_to(tsk.context.sp); }

        let kkk = *(0x16ff0 as *const usize);

        enable_irq(level);
        get_kernel_pagetable().lock_irq().unwrap().install();
        add_task(Arc::new(SpinLock::new(tsk)));
        trace_sync!("add user task OK");
        // shutdown();
    }
    pub fn get_ctx_mut_ref(&mut self)->&mut TaskContext{
        &mut self.context
    }
    pub unsafe fn check_magic(&self){
        if !self.kernel_stack._check_magic(){
            error_sync!("stack overflow");
            shutdown();
        }
    }
    pub unsafe fn install_pagetable(&self) {
        self.mm.as_ref().unwrap().install_pagetable();
    }
}

pub fn task_cpu_init(){
    Task::__core_init();
}