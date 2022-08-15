use alloc::sync::Arc;
use core::ptr::{slice_from_raw_parts, slice_from_raw_parts_mut};
use crate::fs::inode::Inode;
use crate::mm::addr::{PageAlign, Vaddr};
use crate::mm::get_kernel_mm;
use crate::mm::vma::VMA;
use crate::pre::InnerAccess;

pub struct KmapToken{
    vaddr:Vaddr,
    is_anon:bool,
    len:usize
}

impl KmapToken {
    pub fn new_anon(len:usize)->Option<Self>{
        debug_assert!(Vaddr(len).is_align());
        let mut kernel_mm = get_kernel_mm();
        match kernel_mm.alloc_kmap_anon(len) {
            None => {
                return None;
            }
            Some(vma) => {
                let s_a = vma.get_start_vaddr();
                let e_a = vma.get_end_vaddr();
                let len = (e_a-s_a.0).0;
                // do insert vma
                kernel_mm._insert_no_check(vma);
                Some(Self{
                    vaddr: s_a,
                    is_anon: true,
                    len
                })
            }
        }
    }
    pub fn new_file(len:usize,file:Arc<Inode>,file_off:usize,file_len:usize)->Option<Self>{
        debug_assert!(Vaddr(len).is_align());
        let mut kernel_mm = get_kernel_mm();
        match kernel_mm.alloc_kmap_file(len,file,file_off,file_len) {
            None => {
                return None;
            }
            Some(vma) => {
                let s_a = vma.get_start_vaddr();
                let e_a = vma.get_end_vaddr();
                let len = (e_a-s_a.0).0;
                // do insert vma
                kernel_mm._insert_no_check(vma);
                Some(Self{
                    vaddr: s_a,
                    is_anon: false,
                    len
                })
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
            }
        }
    }
}