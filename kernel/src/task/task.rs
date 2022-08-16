use alloc::string::String;
use alloc::sync::{Arc, Weak};
use alloc::vec;
use alloc::vec::{Splice, Vec};
use core::cell::RefCell;
use core::default::default;
use core::hash::Hash;
use core::mem;
use core::mem::size_of;
use core::ops::{Add, Index};
use core::ptr::{slice_from_raw_parts, slice_from_raw_parts_mut};
use fatfs::Read;
use log::error;
use crate::asm::{disable_irq, enable_irq, r_sp, r_sstatus, r_tp, SSTATUS_SIE, SSTATUS_SPIE, SSTATUS_SPP};
use crate::consts::{BOOT_STACK_NR_PAGES, MAX_ORDER, PAGE_SIZE, STACK_MAGIC, USER_STACK_MAX_ADDR};
use crate::mm::mm::MmStruct;
use crate::mm::pagetable::PageTable;
use crate::{error_sync, info_sync, println, SpinLock, trace_sync};
use crate::task::{add_task, generate_tid};
use crate::task::stack::Stack;
use riscv::register::*;
use crate::mm::aux::*;
use xmas_elf::symbol_table::Visibility::Default;
use crate::fs::dfile::{OldDFile, get_stderr, get_stdin, get_stdout, DFile};
use crate::fs::dfile::DFILE_TYPE::DFTYPE_STDIN;
use crate::fs::fat::get_fatfs;
use crate::fs::fcntl::OpenFlags;
use crate::fs::get_dentry_from_dir;
use crate::fs::inode::{ Inode};
use crate::mm::{alloc_one_page, alloc_pages, get_kernel_pagetable};
use crate::mm::addr::{Addr, PageAlign, Vaddr};
use crate::mm::kmap::KmapToken;
use crate::mm::page::Page;
use crate::mm::vma::VMA;
use crate::pre::InnerAccess;
use crate::utils::{order2pages, pages2order};
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
    opened: Vec<Option<Arc<DFile>>>,
    pwd:String,
    pub pwd_dfile:Arc<DFile>,
}

fn get_init_pwd()->String {
    String::from("/")
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
            opened: vec![None;MAX_OPENED],
            pwd:get_init_pwd(),
            pwd_dfile:DFile::get_root()
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
    pub fn get_tgid(&self)->usize{
        self.tgid
    }
    pub fn get_parent(&self)->Option<Arc<SpinLock<Task>>>{
        if self.parent.is_none() {
            return None;
        }
        self.parent.as_ref().unwrap().upgrade()
    }
    pub fn get_status(&self)->TaskStatus{
        self.status
    }
    pub fn set_status(&mut self,status :TaskStatus) {
        self.status = status;
    }
    pub fn get_opened(&mut self, fd:usize) ->Option<Arc<DFile>>{
        if fd < self.opened.len() {
            self.opened[fd].as_mut().map(|x|{
                x.clone()
            })
        } else {
            None
        }
    }
    pub fn set_opened(&mut self, fd:usize, file:Option<Arc<DFile>>)->Result<Option<Arc<DFile>>,()>{
        if fd < self.opened.len() {
            let ret = self.opened[fd].as_ref().map(|x|{
                x.clone()
            });
            self.opened[fd] = file;
            Ok(ret)
        } else {
            Err(())
        }
    }
    pub fn clear_opened(&mut self, fd:usize)->Result<Option<Arc<DFile>>,()>{
        self.set_opened(fd,None)
    }
    pub fn alloc_opened(&mut self, file:Arc<DFile>) ->Option<usize>{
        for i in 0..self.opened.len(){
            match self.opened[i].as_ref(){
                None => {
                    // find empty
                    self.opened[i] = Some(file);
                    return Some(i);
                }
                Some(_) => {}
            }
        }
        None
    }
    pub fn get_pwd_opened(&mut self) ->Arc<DFile>{
        self.pwd_dfile.clone()
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
            opened:vec![None;MAX_OPENED],
            pwd:get_init_pwd(),
            pwd_dfile: DFile::get_root()
        };
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
    pub unsafe fn create_user_task(path:&str,args:Vec<String>)->Option<Arc<SpinLock<Task>>>{
        let node = match Inode::get_root().get_node_by_path(path) {
            Some(node)=> {
                node
            }
            None=> {
                return None;
            }
        };
        let file_len = node.get_dentry().len() as usize;
        let kmap_len = Vaddr(file_len).ceil().get_inner();
        let kmap_token = KmapToken::new_file(kmap_len,
                                             node.clone(),0, file_len).unwrap();
        let read_buf = kmap_token.get_buf();
        let cnt = kmap_token.get_len();
        let (mm_struct, mut auxv, entry_point) = MmStruct::new_from_elf(&(*read_buf)[..cnt],node.clone());
        // kamp 会占用kernel pagetable 使用期间不能够切换页表
        drop(kmap_token);

        trace_sync!("New User Task: entry point={:#X}",entry_point);
        let mut tsk = Task {
            tid: generate_tid(),
            tgid: 0,
            kernel_stack: Stack::new(false,0,0),
            context: TaskContext::new(),
            parent: None,
            status: TaskStatus::TaskRunning,
            mm: Some(mm_struct),
            opened:vec![None;MAX_OPENED],
            pwd:get_init_pwd(),
            pwd_dfile: DFile::get_root()
        };
        tsk.opened[0] = Some(Arc::new(DFile::new_stdin()));
        tsk.opened[1] = Some(Arc::new(DFile::new_stdout()));
        tsk.opened[2] = Some(Arc::new(DFile::new_stderr()));
        tsk.context.ra = user_trap_ret as usize;
        unsafe { tsk.context.sp = tsk.kernel_stack.get_end() - size_of::<TrapFrame>(); }
        tsk.context.sstatus = r_sstatus()|SSTATUS_SPIE|SSTATUS_SIE&(!SSTATUS_SPP);
        let mut tf = TrapFrame::new_empty();
        tf.sstatus = r_sstatus()|SSTATUS_SPIE&(!SSTATUS_SPP);
        tf.sepc = entry_point;
        tf.x2 = tsk.context.sp;

        // install user task pgt to access user stack
        // tsk.mm.as_ref().unwrap().install_pagetable();
        // let walk_ret = tsk.mm.as_ref().unwrap().pagetable.walk(0xFFFFFEE);
        let stack_vma = match tsk.mm.as_mut().unwrap().find_vma(Vaddr(USER_STACK_MAX_ADDR-PAGE_SIZE)) {
            None => {
                return None;
            }
            Some(t) => {
                t
            }
        };
        let stack_args_pg= stack_vma.__fast_alloc_one_page_and_get(Vaddr(USER_STACK_MAX_ADDR-PAGE_SIZE));
        // let mut user_sp = USER_STACK_MAX_ADDR;
        let user_sp_start = (stack_args_pg.get_vaddr()+PAGE_SIZE).get_inner();
        let mut user_sp = user_sp_start;
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
            // envp[i] = user_sp;
            envp[i] = user_sp+USER_STACK_MAX_ADDR-user_sp_start;
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
            // argv[i] = user_sp;
            argv[i] = user_sp+USER_STACK_MAX_ADDR-user_sp_start;
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
        auxv.push(AuxHeader{aux_type: AT_RANDOM, value: user_sp+USER_STACK_MAX_ADDR-user_sp_start});
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

        let auxv_base = user_sp+USER_STACK_MAX_ADDR-user_sp_start;
        // println!("[auxv]: base 0x{:X}", auxv_base);
        for i in 0..auxv.len() {
            // println!("[auxv]: {:?}", auxv[i]);
            let addr = user_sp + core::mem::size_of::<AuxHeader>() * i;
            *( addr as *mut usize) = auxv[i].aux_type ;
            *((addr + core::mem::size_of::<usize>()) as *mut usize) = auxv[i].value ;
        }

        ////////////// *envp [] //////////////////////
        user_sp -= (env.len() + 1) * core::mem::size_of::<usize>();
        let envp_base = user_sp+USER_STACK_MAX_ADDR-user_sp_start;
        *((user_sp + core::mem::size_of::<usize>() * (env.len())) as *mut usize) = 0;
        for i in 0..env.len() {
            *((user_sp + core::mem::size_of::<usize>() * i) as *mut usize) = envp[i] ;
        }

        ////////////// *argv [] //////////////////////
        user_sp -= (args.len() + 1) * core::mem::size_of::<usize>();
        let argv_base = user_sp+USER_STACK_MAX_ADDR-user_sp_start;
        *((user_sp + core::mem::size_of::<usize>() * (args.len())) as *mut usize) = 0;
        for i in 0..args.len() {
            *((user_sp + core::mem::size_of::<usize>() * i) as *mut usize) = argv[i] ;
        }

        ////////////// argc //////////////////////
        user_sp -= core::mem::size_of::<usize>();
        *(user_sp as *mut usize) = args.len();

        let have_used = user_sp_start - user_sp;
        assert!(have_used<PAGE_SIZE);

        // tf.sscratch = user_sp;
        tf.sscratch = USER_STACK_MAX_ADDR-have_used;
        tf.x10 = args.len();
        tf.x11 = argv_base;
        tf.x12 = envp_base;
        tf.x13 = auxv_base;

        unsafe { tf.write_to(tsk.context.sp); }


        // get_kernel_pagetable().lock_irq().unwrap().install();
        // let v:u32 =
        //     unsafe {
        //         tsk.mm.as_ref().unwrap()._read_single_by_vaddr(Vaddr(0x11980))
        //     };
        // println!("{:#X}",v);
        // shutdown();
        info_sync!("add user task OK");
        Some(Arc::new(SpinLock::new(tsk)))
    }
    pub unsafe fn create_user_task_and_run(path:&str,args:Vec<String>)->Result<(),()>{
        add_task(
            match Self::create_user_task(path,args) {
                None => {
                    return Err(());
                }
                Some(tsk) => {
                    tsk
                }
            }
        );
        Ok(())
    }
    pub fn pwd_mut_ref(&mut self)->&mut String{
        &mut self.pwd
    }
    pub fn pwd_ref(& self)->&String{
        &self.pwd
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