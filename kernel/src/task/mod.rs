use alloc::collections::LinkedList;
use alloc::string::String;
use alloc::sync::Arc;
use alloc::vec;
use alloc::vec::Vec;
use core::cell::RefCell;
use core::hint::spin_loop;
use core::sync::atomic::{AtomicU64, AtomicUsize, Ordering};
use log::error;
use crate::mm::{PageTable, PF_Allocator};
use crate::{println, SpinLock};
use crate::sbi::shutdown;

global_asm!(include_str!("switch_context.s"));

// tid必须从1开始，避免错误初始化为0的情况
lazy_static!{
    static ref g_tid:AtomicUsize = AtomicUsize::new(1);
    static ref running_list : SpinLock<LinkedList<Arc<SpinLock<Task>>>> = SpinLock::new(LinkedList::new());
    static ref sleep_list : SpinLock<LinkedList<Arc<SpinLock<Task>>>> = SpinLock::new(LinkedList::new());
}

fn generate_tid()->usize{
    g_tid.fetch_add(1,Ordering::SeqCst)
}

fn current()->Option<Arc<SpinLock<Task>>>{
    running_list.lock().unwrap().front().map(|arc_task| arc_task.clone())
}

pub fn add_task(task:Arc<SpinLock<Task>>){
    running_list.lock().unwrap().push_back(task);
}

pub fn get_task()->Arc<SpinLock<Task>>{
    running_list.lock().unwrap().pop_front().unwrap()
}

fn scheduler(){
    let mut rs = running_list.lock().unwrap();
    if rs.len() == 0 {
        error!("Bug");
    }
    if rs.len()==1{
        return;
    }
    let current = rs.pop_front().unwrap();
    rs.push_back(current.clone());
    let next_running = rs.front().unwrap();
    let ctx_cur= &(current.lock().unwrap().context) as *const TaskContext;
    let ctx_next= &(next_running.lock().unwrap().context) as *const TaskContext;
    unsafe {
        switch_context(ctx_cur ,ctx_next );
    }
}

extern "C" {
    fn switch_context(cur: *const TaskContext,next: *const TaskContext);
    fn kern_trap_ret();
}

enum TaskStatus{
    TaskRunning,
    TaskSleeping,
    TaskZombie,
}

#[repr(C)]
struct TaskContext{
    ra:usize,
    //reserved by callee
    sp:usize,
    s0:usize,
    s1:usize,
    s2:usize,
    s3:usize,
    s4:usize,
    s5:usize,
    s6:usize,
    s7:usize,
    s8:usize,
    s9:usize,
    s10:usize,
    s11:usize,
    sscratch:usize,     // point to stack
    sstatus:usize,
}

pub struct Task{
    tid:usize,
    tgid:usize,
    kernel_stack : usize,
    context : TaskContext,
    pagetable: Arc<SpinLock<PageTable>>,
    parent:Option<Arc<SpinLock<Task>>>,
    status:TaskStatus,
    is_kernel:bool,
}

impl Task {
    fn _core_new_kernel_task<F>(f: F)
        where
            F: FnOnce(),
            F: Send + 'static,
    {
        f();
    }
}

pub fn create_kern_task(func:fn()){
    let p_fn = func as *const ();
    let task = Task{
        tid: generate_tid(),
        tgid: 0,
        kernel_stack: 0,
        context: TaskContext {
            ra: p_fn as usize,
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
        },
        pagetable: Arc::new(SpinLock::new(PageTable::default())),
        parent: None,
        status: TaskStatus::TaskRunning,
        is_kernel: false
    };
    add_task(Arc::new(SpinLock::new(task)));
}

pub fn task_test(){
    let mut task1 = Task{
        tid: 0,
        tgid: 0,
        kernel_stack: 0,
        context: TaskContext {
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
        },
        pagetable: Arc::new(SpinLock::new(PageTable::default())),
        parent: None,
        status: TaskStatus::TaskRunning,
        is_kernel: false
    };
    let p_fn = test_switch_func as *const ();
    task1.context.ra = p_fn as usize;
    add_task(Arc::new(SpinLock::new(task1)));

    let mut task2 = Task{
        tid: 0,
        tgid: 0,
        kernel_stack: 0,
        context: TaskContext {
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
        },
        pagetable: Arc::new(SpinLock::new(PageTable::default())),
        parent: None,
        status: TaskStatus::TaskRunning,
        is_kernel: false
    };
    let p_fn = test_switch_func as *const ();
    task2.context.ra = p_fn as usize;
    add_task(Arc::new(SpinLock::new(task2)));

    println!("start schedule");
    scheduler();
}

fn test_switch_func(){
    println!("This is Test Function");
    shutdown();
}
