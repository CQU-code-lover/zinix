use alloc::sync::Arc;
use crate::consts::{KERNEL_STACK_SIZE_ORDER, PAGE_SIZE, STACK_MAGIC};
use crate::mm::alloc_pages;
use crate::mm::page::Page;

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
                p.as_ref().unwrap().front().get_page_writer().write_volatile(0,STACK_MAGIC  as u64);
                p
            }
        };
        s
    }
    pub unsafe fn _check_magic(&self)->bool{
        let mut magic:u64 = 0;
        match &self.pages {
            None => {
                magic = (self.start as *const u64).read_volatile();
            }
            Some(p) => {
                magic = p.front().get_page_reader().read_volatile(0);
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
                p.front().get_pfn().get_addr_usize()
            }
        }
    }
    pub fn get_end(&self)->usize{
        match &self.pages {
            None => {
                self.end
            }
            Some(p) => {
                p.back().get_pfn().get_addr_usize()+PAGE_SIZE
            }
        }
    }
}

