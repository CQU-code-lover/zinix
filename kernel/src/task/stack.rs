use alloc::sync::Arc;
use core::mem::size_of;
use crate::consts::{KERNEL_STACK_SIZE_ORDER, PAGE_SIZE, STACK_MAGIC};
use crate::info_sync;
use crate::mm::alloc_pages;
use crate::mm::page::Page;
use crate::pre::{InnerAccess, ReadWriteSingleNoOff, ReadWriteSingleOff};
use crate::utils::order2pages;

pub struct Stack{
    start:usize,
    end:usize,
    pages: Option<Arc<Page>>,
}

impl Stack {
    pub fn new(is_boot_task:bool,start:usize,end:usize)->Self{
        let s = Stack{
            start,
            end,
            pages: if is_boot_task{
                None
            } else {
                let mut p = Some(alloc_pages(KERNEL_STACK_SIZE_ORDER).unwrap());
                unsafe { p.as_mut().unwrap().front().write_single_off(STACK_MAGIC as u64, 0); }
                p
            }
        };
        s
    }
    pub fn new_by_copy_from(old:&Self)->Self{
        let new = Self::new(false,0,0);
        assert!(old.pages.is_some());
        let old_pgs = old.pages.as_ref().unwrap();
        let new_pgs = new.pages.as_ref().unwrap();
        assert_eq!(old_pgs.get_order(), new_pgs.get_order());
        let len = order2pages(old_pgs.get_order())*PAGE_SIZE;
        let ll = old.get_end() - old.get_start();
        let old_start = old_pgs.get_vaddr();
        let new_start = new_pgs.get_vaddr();
        let mut i = 0;
        while i < len {
            unsafe {
                let tmp:usize = (old_start+i).read_single().unwrap();
                (new_start+i).write_single(tmp).unwrap();
                i+=size_of::<usize>();
            }
        }
        new
    }
    pub unsafe fn _check_magic(&self)->bool{
        let mut magic:u64 = 0;
        match &self.pages {
            None => {
                magic = (self.start as *const u64).read_volatile();
            }
            Some(p) => {
                magic = p.front().read_single_off(0).unwrap();
            }
        }
        magic == STACK_MAGIC
    }
    pub fn get_start(&self)->usize{
        match &self.pages {
            None => {
                self.start
            }
            Some(p) => {
                p.front().get_vaddr().get_inner()
            }
        }
    }
    pub fn get_end(&self)->usize{
        match &self.pages {
            None => {
                self.end
            }
            Some(p) => {
                p.back().get_vaddr().get_inner()+PAGE_SIZE
            }
        }
    }
}

