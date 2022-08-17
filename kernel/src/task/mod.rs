use alloc::collections::LinkedList;
use alloc::string::String;
use alloc::sync::Arc;
use alloc::vec;
use alloc::vec::Vec;
use core::arch::global_asm;
use core::arch::riscv64::fence_i;
use core::cell::RefCell;
use core::hint::spin_loop;
use core::sync::atomic::{AtomicU64, AtomicUsize, Ordering};

use log::error;

use crate::{error_sync, info_sync, println, SpinLock};
use crate::mm::mm::MmStruct;
use crate::mm::pagetable::PageTable;
use crate::sbi::shutdown;
use crate::task::task::{get_running, set_running, Task, TaskContext, TaskStatus};
use crate::task::task::TaskStatus::TaskZombie;
use crate::trap::TrapFrame;

pub(crate) mod task;
pub(crate) mod stack;
pub(crate) mod info;

extern "C" {
    fn switch_context(cur: *const TaskContext, next: *const TaskContext);
    fn kern_trap_ret();
}

global_asm!(include_str!("switch_context.s"));

// tid必须从1开始，避免错误初始化为0的情况
lazy_static! {
    static ref g_tid:AtomicUsize = AtomicUsize::new(1);
    static ref running_list : SpinLock<LinkedList<Arc<SpinLock<Task>>>> = SpinLock::new(LinkedList::new());
    static ref sleep_list : SpinLock<LinkedList<Arc<SpinLock<Task>>>> = SpinLock::new(LinkedList::new());
    static ref exit_list : SpinLock<LinkedList<Arc<SpinLock<Task>>>> = SpinLock::new(LinkedList::new());
}

fn generate_tid() -> usize {
    g_tid.fetch_add(1, Ordering::SeqCst)
}

fn current() -> Option<Arc<SpinLock<Task>>> {
    running_list.lock().unwrap().front().map(|arc_task| arc_task.clone())
}

pub fn add_task(task: Arc<SpinLock<Task>>) {
    running_list.lock().unwrap().push_back(task);
}

pub fn exit_self(){
    let this_task = get_running();
    this_task.lock_irq().unwrap().set_status(TaskZombie);
    scheduler(None);
}

pub fn get_task() -> Arc<SpinLock<Task>> {
    running_list.lock().unwrap().pop_front().unwrap()
}

pub fn scheduler(sleep_list_assign:Option<&SpinLock<LinkedList<Arc<SpinLock<Task>>>>>) {
    // info_sync!("schedule");
    let mut rs = running_list.lock().unwrap();
    // bug raise
    assert_ne!(rs.len(), 0);
    let current = get_running();
    // todo 实现idle
    if rs.len() == 1 {
        match current.lock_irq().unwrap().get_status() {
            TaskStatus::TaskRunning => {
                return;
            }
            _ => {
                // idle
                todo!()
            }
        }
    }
    // pop running task
    rs.pop_front();
    match current.lock_irq().unwrap().get_status() {
        TaskStatus::TaskRunning => {
            rs.push_back(current.clone());
        }
        TaskStatus::TaskSleeping => {
            if sleep_list_assign.is_some(){
                sleep_list_assign.unwrap().lock_irq().unwrap().push_back(current.clone());
            } else {
                sleep_list.lock_irq().unwrap().push_back(current.clone());
            }
        }
        TaskStatus::TaskZombie => {
            exit_list.lock_irq().unwrap().push_back(current.clone());
        }
    }
    let next_running = rs.front().unwrap();
    let ctx_cur = current.lock().unwrap().get_ctx_mut_ref() as *const TaskContext;
    let ctx_next = next_running.lock().unwrap().get_ctx_mut_ref() as *const TaskContext;
    set_running(next_running.clone());
    let next_locked = next_running.lock_irq().unwrap();
    if next_locked.is_user() {
        // install pagetable
        // next_locked.context.sp
        // let ttff = unsafe {&*(next_locked.context.sp as *const TrapFrame)};
        // println!("{:?}",ttff);
        // shutdown();
        unsafe { next_locked.install_pagetable(); }
    }
    drop(next_locked);
    drop(rs);
    unsafe {
        switch_context(ctx_cur, ctx_next);
    }
}

pub fn task_init() {

}

pub fn task_test() {
    Task::create_kern_task_and_run(test_switch_func);
    println!("start schedule");
}

fn test_switch_func() {
    loop {
        println!("This is Test Function");
    }
}
