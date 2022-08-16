pub mod timer;

use core::arch::{asm, global_asm};
use core::arch::riscv64::{fence_i, sfence_vma_all, sfence_vma_vaddr};
use core::fmt::{Debug, Formatter};
use core::mem::size_of;
use log::debug;
use riscv::register::{sie, sstatus, stvec, scause};
use riscv::register::scause::{Exception, Interrupt, Scause, Trap};
use riscv::register::stvec::TrapMode;
use crate::{debug_sync, info_sync, println, warn_sync};
use crate::asm::{disable_irq, enable_irq, r_satp, r_scause, r_stval};
use crate::consts::PHY_MEM_OFFSET;
use crate::mm::{get_kernel_mm, get_kernel_pagetable};
use crate::mm::addr::{Paddr, PageAlign, Vaddr};
use crate::mm::pagetable::{PageTable, PTEFlags};
use crate::mm::vma::VMA;
use crate::pre::{InnerAccess, ReadWriteSingleNoOff};
use crate::sbi::shutdown;
use crate::syscall::syscall_entry;
use crate::task::task::{get_running, RUNNING_TASK};
use crate::trap::timer::timer_entry;
use crate::utils::{memcpy, set_usize_by_addr};
global_asm!(include_str!("trap_asm.s"));

#[repr(C)]
pub struct TrapFrame{
    pub sepc:usize,   //sepc
    pub x1:usize,   //ra
    pub x2:usize,   //sp--->this
    pub x3:usize,
    pub x4:usize,
    pub x5:usize,
    pub x6:usize,
    pub x7:usize,
    pub x8:usize,
    pub x9:usize,
    pub x10:usize,
    pub x11:usize,
    pub x12:usize,
    pub x13:usize,
    pub x14:usize,
    pub x15:usize,
    pub x16:usize,
    pub x17:usize,
    pub x18:usize,
    pub x19:usize,
    pub x20:usize,
    pub x21:usize,
    pub x22:usize,
    pub x23:usize,
    pub x24:usize,
    pub x25:usize,
    pub x26:usize,
    pub x27:usize,
    pub x28:usize,
    pub x29:usize,
    pub x30:usize,
    pub x31:usize,
    pub scause:usize,
    pub sscratch:usize,
    pub sstatus:usize,
}

impl Debug for TrapFrame {
    fn fmt(&self, f: &mut Formatter<'_>) -> core::fmt::Result {
        writeln!(f,"sepc:0x{:X}",self.sepc);
        writeln!(f,"sstatus:0x{:X}",self.sstatus);
        writeln!(f,"sscratch:0x{:X}",self.sscratch);
        writeln!(f,"ra:0x{:X}",self.x1);
        writeln!(f,"sp:0x{:X}",self.x2);
        writeln!(f,"a0:0x{:X}",self.x10);
        writeln!(f,"a1:0x{:X}",self.x11);
        writeln!(f,"a2:0x{:X}",self.x12);
        Ok(())
    }
}

impl TrapFrame {
    pub fn new_empty()->Self{
        TrapFrame{
            sepc: 0,
            x1: 0,
            x2: 0,
            x3: 0,
            x4: 0,
            x5: 0,
            x6: 0,
            x7: 0,
            x8: 0,
            x9: 0,
            x10: 0,
            x11: 0,
            x12: 0,
            x13: 0,
            x14: 0,
            x15: 0,
            x16: 0,
            x17: 0,
            x18: 0,
            x19: 0,
            x20: 0,
            x21: 0,
            x22: 0,
            x23: 0,
            x24: 0,
            x25: 0,
            x26: 0,
            x27: 0,
            x28: 0,
            x29: 0,
            x30: 0,
            x31: 0,
            scause: 0,
            sscratch: 0,
            sstatus: 0
        }
    }
    pub unsafe fn read_from(&mut self,addr:usize){
        let len = size_of::<Self>();
        memcpy(addr, self as *mut TrapFrame as usize,len);
    }
    pub unsafe fn write_to(&mut self,addr:usize){
        let len = size_of::<Self>();
        memcpy( addr,self as *mut TrapFrame as usize,len);
    }
    pub fn ok(&mut self){
        self.x10 = 0;
    }
    pub fn ret(&mut self,val:usize){
        self.x10 = val;
    }
    pub fn err(&mut self){
        self.x10 = (-1 as i64) as usize;
    }
    pub fn arg0(&self)->usize{
        self.x10
    }
    pub fn arg1(&self)->usize{
        self.x11
    }
    pub fn arg2(&self)->usize{
        self.x12
    }
    pub fn arg3(&self)->usize{
        self.x13
    }
    pub fn arg4(&self)->usize{
        self.x14
    }
    pub fn arg5(&self)->usize{
        self.x15
    }
}

#[no_mangle]
fn irq_handler(trap_frame:&mut TrapFrame){
    unsafe { RUNNING_TASK().lock_irq().unwrap().check_magic(); }
    // debug!("IRQ\n{:?}",trap_frame);
    match scause::read().cause() {
        Trap::Interrupt(irq) => {
            match irq {
                Interrupt::UserSoft => {
                    todo!()
                }
                Interrupt::SupervisorSoft => {
                    todo!()
                }
                Interrupt::UserTimer => {
                    todo!()
                }
                Interrupt::SupervisorTimer => {
                    // info_sync!("tic");
                    timer_entry(trap_frame);
                }
                Interrupt::UserExternal => {
                    todo!()
                }
                Interrupt::SupervisorExternal => {
                    todo!()
                }
                Interrupt::Unknown => {
                    panic!("unrecognized interrupt");
                }
            }
        }
        _ => panic!("irq bug")
    }
}

#[no_mangle]
fn exc_handler(trap_frame:&mut TrapFrame){
    let irq_state = disable_irq();
    debug_sync!("EXC\n{:?}",trap_frame);
    debug_sync!("sstval:{:#X}",r_stval());
    debug_sync!("scause:{:#X}",r_scause());
    unsafe { RUNNING_TASK().lock_irq().unwrap().check_magic(); }
    match scause::read().cause() {
        Trap::Exception(exc) => {
            unsafe {
                match exc {
                    Exception::InstructionMisaligned => {
                        todo!()
                    }
                    Exception::InstructionFault => {
                        todo!()
                    }
                    Exception::IllegalInstruction => {
                        todo!()
                    }
                    Exception::Breakpoint => {
                        todo!()
                    }
                    Exception::LoadFault => {
                        todo!()
                    }
                    Exception::StoreMisaligned => {
                        todo!()
                    }
                    Exception::StoreFault => {
                        todo!()
                    }
                    Exception::UserEnvCall => {
                        syscall_entry(trap_frame);
                        trap_frame.sepc+=4;
                    }
                    Exception::InstructionPageFault => {
                        let vaddr = r_stval();
                        trap_page_fault_handler(Vaddr(vaddr));
                        // unsafe {
                        //     // get_running().lock().unwrap().install_pagetable();
                        //     let v = *(trap_frame.sepc as *const usize);
                        //     println!("ins = {:#X}", v);
                        // }
                        return;
                        let mut pte = get_running().lock().unwrap().mm.as_ref().unwrap().pagetable.walk(trap_frame.sepc).unwrap().get_pte();
                        println!("{:#b}", pte.flags);
                        // pte.flags|=PTEFlags::A.bits();
                        // set_usize_by_addr(pte.get_point_paddr()+PHY_MEM_OFFSET,pte.into());

                        pte = get_running().lock().unwrap().mm.as_ref().unwrap().pagetable.walk(trap_frame.sepc).unwrap().get_pte();
                        println!("{:#b}", pte.flags);
                    }
                    Exception::LoadPageFault => {
                        let vaddr = r_stval();
                        trap_page_fault_handler(Vaddr(vaddr));
                        return;
                        let running = get_running();
                        let tsk = running.lock_irq().unwrap();
                        if tsk.is_kern(){
                            let mut mm = get_kernel_mm();
                            let va = Vaddr(r_stval()).floor();
                            let vma = mm.find_vma(va).unwrap();
                            vma._do_alloc_one_page(va);
                        } else {
                            panic!("load pg fault");
                        }
                        // let pte = get_running().lock().unwrap().mm.as_ref().unwrap().pagetable.walk(vaddr).unwrap().get_pte();
                        // println!("{:#b}", pte.flags);
                    }
                    Exception::StorePageFault => {
                        let vaddr = r_stval();
                        trap_page_fault_handler(Vaddr(vaddr));
                        return;
                        // let vaddr = r_stval();
                        // let pte  = if get_running().lock_irq().unwrap().mm.is_some() {
                        //      get_running().lock().unwrap().mm.as_ref().unwrap().pagetable.walk(vaddr).unwrap().get_pte()
                        // } else {
                        //     get_kernel_pagetable().lock_irq().as_ref().unwrap().walk(vaddr).unwrap().get_pte()
                        // };
                        // println!("{:#b}", pte.flags);
                        panic!("store pg fault");
                    }
                    Exception::Unknown => {
                        panic!("unrecognized exception");
                    }
                }
            }
        }
        _ => panic!("exc bug")
    }
    enable_irq(irq_state);
}

pub fn trap_init(){
    extern "C" { fn trap_entry(); }
    unsafe {
        stvec::write(trap_entry as usize, TrapMode::Direct);
        sstatus::set_sie();
        // timer is enable, but not set next tic
        sie::set_stimer();
    }
}

fn trap_page_fault_handler(vaddr:Vaddr) ->bool{
    let v = vaddr.floor();
    let running = get_running();
    let mut tsk = running.lock_irq().unwrap();
    let mut mm = get_kernel_mm();
    if tsk.is_kern(){
        let vma_opt = mm.find_vma(v);
        match vma_opt{
            None => {
                panic!("error address access!");
            }
            Some(vma) => {
                if vma._find_page(v).is_some(){
                    // 已经映射过 说明存在权限访问问题
                    let walk = vma.pagetable.as_ref().unwrap().walk(v.get_inner()).unwrap();
                    let pte = walk.get_pte();
                    let paddr =pte.get_point_paddr();
                    let flags =  pte.flags;
                    // unsafe { fence_i(); }
                    // warn_sync!("error address prot");
                    let satp = r_satp();
                    let pgt = (satp&0xFFFFFFFFFFFFF)<<12;
                    println!("pgt:{:#X}",pgt);
                    let paddr:Paddr =mm.pagetable._get_root_page_vaddr().into();
                    println!("pggt:{:#X}",paddr);
                    panic!("error address prot!va:{:#X},pa:{:#X},PTEflags:{:b}",v,paddr,flags);
                } else {

                    vma._do_alloc_one_page(v).unwrap();
                    return true;
                }
            }
        }
    } else {
        let mut m = tsk.mm.as_mut().unwrap();
        match m.find_vma(v){
            None => {
                let p:Paddr = m.pagetable._get_root_page_vaddr().into();
                m.pagetable.walk(0x10154);
                panic!("error address access!{:#X} {:#X}",p,r_satp()<<12);
            }
            Some(vma) => {
                if vma._find_page(v).is_some(){
                    // 已经映射过 说明存在权限访问问题
                    panic!("error address prot!");
                }
                vma._do_alloc_one_page(v).unwrap();
                debug_sync!("pgf alloc vaddr:{:#X}",v);
                return true;
            }
        }
    }
}