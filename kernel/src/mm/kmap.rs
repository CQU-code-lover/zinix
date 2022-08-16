use alloc::sync::Arc;
use core::ptr::{slice_from_raw_parts, slice_from_raw_parts_mut};
use riscv::asm::sfence_vma_all;
use crate::asm::{disable_irq, enable_irq, r_satp, w_satp};
use crate::fs::inode::Inode;
use crate::info_sync;
use crate::mm::addr::{PageAlign, Vaddr};
use crate::mm::{get_kernel_mm, get_kernel_pagetable};
use crate::mm::vma::VMA;
use crate::pre::InnerAccess;
use crate::sync::{get_irq_lock, SpinLockGuard};

pub struct KmapToken{
    vaddr:Vaddr,
    is_anon:bool,
    len:usize,
    satp_val:usize,
    irq_level:usize
}

impl KmapToken {
    pub fn new_anon(len:usize)->Option<Self>{
        let level = disable_irq();
        debug_assert!(Vaddr(len).is_align());
        let mut kernel_mm = get_kernel_mm();
        match kernel_mm.alloc_kmap_anon(len) {
            None => {
                enable_irq(level);
                return None;
            }
            Some(vma) => {
                let s_a = vma.get_start_vaddr();
                let e_a = vma.get_end_vaddr();
                let len = (e_a-s_a.0).0;
                // do insert vma
                kernel_mm._insert_no_check(vma);
                let mut r =  Self{
                    vaddr: s_a,
                    is_anon: true,
                    len,
                    satp_val: r_satp(),
                    irq_level:level
                };
                unsafe { kernel_mm.install_pagetable(); }
                Some(r)
            }
        }
    }
    pub fn new_file(len:usize,file:Arc<Inode>,file_off:usize,file_len:usize)->Option<Self>{
        // 必须在获取kernel_mm之前irq disable 否则返回时kernel mm lock失效会打开中断
        let level = disable_irq();
        debug_assert!(Vaddr(len).is_align());
        let mut kernel_mm = get_kernel_mm();
        match kernel_mm.alloc_kmap_file(len,file,file_off,file_len) {
            None => {
                enable_irq(level);
                return None;
            }
            Some(vma) => {
                let s_a = vma.get_start_vaddr();
                let e_a = vma.get_end_vaddr();
                let len = (e_a-s_a.0).0;
                // do insert vma
                kernel_mm._insert_no_check(vma);
                let mut r =  Self{
                    vaddr: s_a,
                    is_anon: false,
                    len,
                    satp_val: r_satp(),
                    irq_level:level
                };
                unsafe { kernel_mm.install_pagetable(); }
                info_sync!("New Kmap Token");
                Some(r)
            }
        }
    }
    pub fn get_buf(&self)->*const [u8]{
        let ptr = self.vaddr.get_inner() as *const u8;
        slice_from_raw_parts(ptr,self.len)
    }
    pub fn get_buf_mut(&self)->*mut [u8]{
        let ptr = self.vaddr.get_inner() as *mut u8;
        slice_from_raw_parts_mut(ptr,self.len)
    }
    pub fn get_vaddr(&self)->Vaddr{
        self.vaddr
    }
    pub fn get_len(&self)->usize{
        self.len
    }
}

impl Drop for KmapToken {
    fn drop(&mut self) {
        let mut kernel_mm = get_kernel_mm();
        match kernel_mm.drop_vma(self.vaddr) {
            None => {
                panic!("Kmap bug");
            }
            Some(s) => {
                // pass
                info_sync!("switch pagetable:{:#X}",self.satp_val<<12);
                w_satp(self.satp_val);
                unsafe { sfence_vma_all(); }
            }
        }
        //必须先drop
        drop(kernel_mm);
        info_sync!("Kmap Token Droped");
        enable_irq(self.irq_level);
    }
}