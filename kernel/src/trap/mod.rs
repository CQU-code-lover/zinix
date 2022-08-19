pub mod timer;

use alloc::collections::BTreeMap;
use alloc::sync::Arc;
use core::arch::{asm, global_asm};
use core::arch::riscv64::{fence_i, sfence_vma_all, sfence_vma_vaddr};
use core::fmt::{Debug, Formatter};
use core::mem::size_of;
use fatfs::Write;
use log::debug;
use riscv::register::{sie, sstatus, stvec, scause};
use riscv::register::scause::{Exception, Interrupt, Scause, Trap};
use riscv::register::sstatus::Sstatus;
use riscv::register::stvec::TrapMode;
use crate::{debug_sync, info_sync, println, r_sstatus, trace_sync, warn_sync};
use crate::asm::{disable_irq, enable_irq, r_satp, r_scause, r_stval, SSTATUS_SPP};
use crate::consts::PHY_MEM_OFFSET;
use crate::mm::{alloc_one_page, get_kernel_mm, get_kernel_pagetable};
use crate::mm::addr::{Paddr, PageAlign, Vaddr};
use crate::mm::page::Page;
use crate::mm::pagetable::{PageTable, PTEFlags};
use crate::mm::vma::{_vma_flags_2_pte_flags, MmapProt, VMA, VmFlags};
use crate::pre::{InnerAccess, ReadWriteSingleNoOff, ShowRdWrEx};
use crate::sbi::shutdown;
use crate::syscall::syscall_entry;
use crate::task::task::{get_running, RUNNING_TASK};
use crate::trap::timer::timer_entry;
use crate::utils::{memcpy, set_usize_by_addr};
global_asm!(include_str!("trap_asm.s"));

#[derive(Clone)]
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
    pub unsafe fn write_to(&self,addr:usize){
        let len = size_of::<Self>();
        memcpy( addr,self as *const TrapFrame as usize,len);
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
    // trace_sync!("IRQ\n{:?}",trap_frame);
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
    let spp = r_sstatus()&SSTATUS_SPP;

    debug_sync!("spp:{}",spp);
    unsafe { RUNNING_TASK().lock_irq().unwrap().check_magic(); }
    match scause::read().cause() {
        Trap::Exception(exc) => {
            unsafe {
                match exc {
                    Exception::InstructionMisaligned => {
                        todo!()
                    }
                    Exception::IllegalInstruction => {
                        todo!()
                    }
                    Exception::Breakpoint => {
                        todo!()
                    }
                    Exception::StoreMisaligned => {
                        todo!()
                    }
                    Exception::UserEnvCall => {
                        syscall_entry(trap_frame);
                        trap_frame.sepc+=4;
                        info_sync!("syscall");
                    }
                    Exception::InstructionPageFault|Exception::InstructionFault => {
                        if r_sstatus()&SSTATUS_SPP !=0{
                            trap_frame.sstatus&(!SSTATUS_SPP);
                            info_sync!("change spp");
                        }
                        let vaddr = r_stval();
                        trap_page_fault_handler(Vaddr(vaddr),PgFaultProt::EXEC);
                        // unsafe {
                        //     // get_running().lock().unwrap().install_pagetable();
                        //     let v = *(trap_frame.sepc as *const usize);
                        //     println!("ins = {:#X}", v);
                        // }
                        // let mut pte = get_running().lock().unwrap().mm.as_ref().unwrap().pagetable.walk(trap_frame.sepc).unwrap().get_pte();
                        // println!("{:#b}", pte.flags);
                        // pte.flags|=PTEFlags::A.bits();
                        // set_usize_by_addr(pte.get_point_paddr()+PHY_MEM_OFFSET,pte.into());

                        // pte = get_running().lock().unwrap().mm.as_ref().unwrap().pagetable.walk(trap_frame.sepc).unwrap().get_pte();
                        // println!("{:#b}", pte.flags);
                    }
                    Exception::LoadPageFault|Exception::LoadFault=> {
                        let vaddr = r_stval();
                        trap_page_fault_handler(Vaddr(vaddr),PgFaultProt::Read);
                    }
                    Exception::StorePageFault|Exception::StoreFault =>{
                        let vaddr = r_stval();
                        trap_page_fault_handler(Vaddr(vaddr),PgFaultProt::Write);
                    }
                    Exception::Unknown => {
                        panic!("unrecognized exception");
                    }
                }
            }
        }
        _ => panic!("exc bug")
    }
    trace_sync!("exc handler ret user space");
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

#[derive(Clone,PartialOrd, PartialEq)]
enum PgFaultProt{
    Read,
    Write,
    EXEC
}

#[cfg(not(feature = "copy_on_write"))]
fn trap_page_fault_handler(vaddr:Vaddr,prot:PgFaultProt) ->bool {
    let v = vaddr.floor();
    let running = get_running();
    let mut tsk = running.lock_irq().unwrap();
    let mut mm = get_kernel_mm();
    let is_kern = r_sstatus()&SSTATUS_SPP!=0;
    if is_kern{
        let vma_opt = mm.find_vma(v);
        match vma_opt{
            None => {
                panic!("error address access!");
            }
            Some(vma) => {
                if vma._find_page(v).is_some(){
                    // 已经映射过 说明存在权限访问问题
                    panic!("prot fault");
                } else {
                    vma._do_alloc_one_page(v).unwrap();
                    return true;
                }
            }
        }
    } else {
        let mut mm_locked = tsk.mm.as_mut().unwrap().lock_irq().unwrap();
        let mut vma_opt = mm_locked.find_vma(v);
        match vma_opt{
            None => {
                panic!("error address access!");
            }
            Some(vma) => {
                if vma._find_page(v).is_some(){
                    // 已经映射过 说明存在权限访问问题
                    panic!("prot fault");
                } else {
                    vma._do_alloc_one_page(v).unwrap();
                    return true;
                }
            }
        }
    }
}


#[cfg(feature = "copy_on_write")]
fn trap_page_fault_handler(vaddr:Vaddr,prot:PgFaultProt) ->bool{
    match prot.clone() {
        PgFaultProt::Read => {
            debug_sync!("PGF:{:#X},R",vaddr);
        }
        PgFaultProt::Write => {
            debug_sync!("PGF:{:#X},W",vaddr);
        }
        PgFaultProt::EXEC => {
            debug_sync!("PGF:{:#X},X",vaddr);
        }
    }
    let v = vaddr.floor();
    let running = get_running();
    let mut tsk = running.lock_irq().unwrap();
    let mut mm = get_kernel_mm();
    let is_kern = r_sstatus()&SSTATUS_SPP!=0;
    if is_kern{
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
        let mut tsk_mm = tsk.mm.as_mut().unwrap().lock_irq().unwrap();
        let is_cow = tsk_mm.cow_target.is_some();
        if is_cow {
            let mut cow_mm_nolock = tsk_mm.cow_target.as_ref().unwrap().clone();
            let mut cow_mm = cow_mm_nolock.lock_irq().unwrap();
            // todo 这有个bug 当新建的cow进程使用非法权限访问时无法被捕捉到
            match cow_mm.find_vma(v) {
                None => {
                    panic!("cow fail");
                }
                Some(vma) => {
                    // 检查是否remap过，如果remap过那么存在权限问题
                    if tsk_mm.find_vma(v).unwrap()._vaddr_have_map(v){
                        panic!("prot fault");
                    }

                    let pg = match vma._find_page(v) {
                        None => {
                            //原来的vma未map
                            vma._do_alloc_one_page(v).unwrap()
                        }
                        Some(s) => { s }
                    };
                    // 注意此时存在的两种情况
                    // case1：pagetable没有映射所以导致进入trap
                    // case2：pagetable已经映射了，但是prot fault
                    match prot {
                        PgFaultProt::Read => {
                            // 如果为读或者执行那么会映射到cow_mm的物理页
                            let r_vma = tsk_mm.find_vma(v).unwrap();
                            if !r_vma.readable(){
                                panic!("prot fault");
                            }
                            let map_flags = vma.get_flags() & (!VmFlags::VM_WRITE);
                            r_vma.pagetable.as_ref().unwrap().map_one_page(
                                v,
                                pg.get_paddr(),
                                _vma_flags_2_pte_flags(map_flags),
                            );
                        }
                        PgFaultProt::Write => {
                            // 判断vma是否支持write
                            let w_vma = tsk_mm.find_vma(v).unwrap();
                            if !w_vma.writeable(){
                                panic!("prot fault");
                            }
                            // 注意cow的页可能已经被写过了 检查Vma的write tree
                            match &vma.cow_write_reserve_pgs{
                                None => {
                                    //原cow mm未写 那么直接复制page tree上的数据即可
                                    //强制进行数据map
                                    w_vma._cow_remap_one_page(v,pg.clone()).unwrap();
                                }
                                Some(cwrpgs) => {
                                    match cwrpgs.get(&v){
                                        None => {
                                            w_vma._cow_remap_one_page(v,pg.clone()).unwrap();
                                        }
                                        Some(arcpg) => {
                                            w_vma._cow_remap_one_page(v,arcpg.clone()).unwrap();
                                        }
                                    }
                                }
                            }
                        }
                        PgFaultProt::EXEC => {
                            // 如果为读或者执行那么会映射到cow_mm的物理页
                            let e_vma = tsk_mm.find_vma(v).unwrap();
                            if !e_vma.execable(){
                                panic!("prot fault");
                            }
                            let map_flags = vma.get_flags() & (!VmFlags::VM_WRITE);
                            e_vma.pagetable.as_ref().unwrap().map_one_page(
                                v,
                                pg.get_paddr(),
                                _vma_flags_2_pte_flags(map_flags),
                            );
                        }
                    }
                }
            }
            unsafe { sfence_vma_all(); }
        } else {
            match tsk_mm.find_vma(v){
                None => {
                    let p:Paddr = tsk_mm.pagetable._get_root_page_vaddr().into();
                    panic!("error address access!{:#X} {:#X}",p,r_satp()<<12);
                }
                Some(vma) => {
                    match vma._find_page(v){
                        Some(pg) => {
                            // 已经映射过 说明存在权限访问问题或者是作为被cow的vma
                            if prot==PgFaultProt::Write&&vma.cow_write_reserve_pgs.is_some(){
                                let new_flags = _vma_flags_2_pte_flags(vma.vm_flags);
                                vma.pagetable.as_ref().unwrap().change_map_flags(v,new_flags).unwrap();
                                let fixed_pg = alloc_one_page().unwrap();
                                unsafe { fixed_pg.copy_one_page_data_from(pg) };
                                vma.cow_write_reserve_pgs.as_mut().unwrap().insert(v,fixed_pg);
                            } else {
                                panic!("error address prot!");
                            }
                        }
                        None => {
                            vma._do_alloc_one_page(v).unwrap();
                            debug_sync!("pgf alloc vaddr:{:#X}",v);
                            return true;
                        }
                    }
                }
            }
        }
        true
    }
}