use alloc::sync::Arc;
use crate::consts::{KERNEL_STACK_SIZE_ORDER, PAGE_SIZE, STACK_MAGIC};
use crate::mm::alloc_pages;
use crate::mm::page::Page;

pub struct Stack{
    pages: Arc<Page>,
}

impl Stack {
    pub fn new()->Self{
        let s = Stack{
            pages: alloc_pages(KERNEL_STACK_SIZE_ORDER).unwrap()
        };
        s.pages.front().get_page_writer().write_volatile(0,STACK_MAGIC  as u64);
        s
    }
    pub fn _check_magic(&self)->bool{
        let magic:u64 = self.pages.front().get_page_reader().read_volatile(0);
        magic == STACK_MAGIC
    }
}

