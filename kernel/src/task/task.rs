use alloc::sync::{Arc, Weak};
use alloc::vec;
use alloc::vec::{Splice, Vec};
use core::cell::RefCell;
use core::default::default;
use core::mem::size_of;
use log::error;
use crate::asm::{r_sp, r_sstatus, r_tp, SSTATUS_SIE, SSTATUS_SPIE, SSTATUS_SPP};
use crate::consts::{BOOT_STACK_NR_PAGES, PAGE_SIZE, STACK_MAGIC};
use crate::mm::mm::MmStruct;
use crate::mm::pagetable::PageTable;
use crate::{error_sync, SpinLock};
use crate::task::{add_task, generate_tid};
use crate::task::stack::Stack;
use riscv::register::*;
use xmas_elf::symbol_table::Visibility::Default;
use crate::fs::dfile::DFile;
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

enum TaskStatus {
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
    mm: Option<MmStruct>,
    opened: Vec<Option<Arc<DFile>>>
}

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
        tsk.context.ra = p_fn as usize;
        unsafe { tsk.context.sp = tsk.kernel_stack.get_end() - size_of::<TrapFrame>(); }
        tsk.context.sstatus = r_sstatus()|SSTATUS_SPP|SSTATUS_SPIE|SSTATUS_SIE;
        let mut tf = TrapFrame::new_empty();
        tf.sstatus = r_sstatus()|SSTATUS_SPP|SSTATUS_SPIE;
        tf.sepc = p_fn as usize;
        tf.x2 = tsk.context.sp;
        unsafe { tf.write_to(tsk.context.sp); }
        tsk
    }
    pub fn create_kern_task_and_run(func:fn()){
        add_task(Arc::new(SpinLock::new(Self::create_kern_task(func))))
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
}

pub fn task_cpu_init(){
    Task::__core_init();
}