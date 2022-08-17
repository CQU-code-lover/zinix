use alloc::sync::Arc;
use crate::consts::{KERNEL_STACK_SIZE_ORDER, PAGE_SIZE, STACK_MAGIC};
use crate::info_sync;
use crate::mm::alloc_pages;
use crate::mm::page::Page;
use crate::pre::{InnerAccess, ReadWriteSingleOff};

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

