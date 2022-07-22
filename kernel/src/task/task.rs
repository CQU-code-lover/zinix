use alloc::sync::{Arc, Weak};
use riscv::register::mtvec::read;
use crate::asm::{r_sp, r_tp};
use crate::consts::{BOOT_STACK_NR_PAGES, PAGE_SIZE, STACK_MAGIC};
use crate::mm::mm::MmStruct;
use crate::mm::pagetable::PageTable;
use crate::SpinLock;
use crate::task::{add_task, generate_tid};
use crate::task::stack::Stack;

extern "C" {
    fn switch_context(cur: *const TaskContext, next: *const TaskContext);
    fn kern_trap_ret();
    fn boot_stack();
    fn boot_stack_top();
}

enum TaskStatus {
    TaskRunning,
    TaskSleeping,
    TaskZombie,
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
    kernel_stack: Option<Stack>,
    context: TaskContext,
    parent: Option<Weak<SpinLock<Task>>>,
    status: TaskStatus,
    mm: Option<MmStruct>,
}

impl Task {
    pub fn __core_init(){
        let mut addr = boot_stack_top as usize;
        let sp = r_sp();
        while addr<(boot_stack as usize) {
            if sp>addr&&sp<=addr+(PAGE_SIZE*BOOT_STACK_NR_PAGES){
                // find
                if sp-addr<=8 {
                    panic!("can`t insert stack magic for boot thread");
                } else {
                    unsafe { (addr as *mut u64).write_volatile(STACK_MAGIC); }
                }
            }
            addr+=(PAGE_SIZE*BOOT_STACK_NR_PAGES);
        }
        let tsk = Task{
            tid: generate_tid(),
            tgid: 0,
            kernel_stack: None,
            context: TaskContext::new(),
            parent: None,
            status: TaskStatus::TaskRunning,
            mm: None
        };

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
        let task = Task {
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
                sstatus: 0,
            },
            pagetable: Arc::new(PageTable::default()),
            parent: None,
            status: TaskStatus::TaskRunning,
            is_kernel: false,
            mm: None,
        };
        task
    }
}
