use alloc::sync::Arc;
use alloc::vec;
use alloc::vec::Vec;

use crate::mm::addr::Addr;
use crate::mm::page::Page;

bitflags! {
    pub struct PTEFlags: u8 {
        const VM_READ = 1 << 0;
        const VM_WRITE = 1 << 1;
        const VM_EXEC = 1 << 2;
        const VM_SHARD = 1 << 3;
    }
}

pub struct VMA{
    start_addr:Addr,
    end_addr:Addr,
    vm_flags:u8,
    pages: Vec<Arc<Page>>,
    phy_pgs_cnt:usize
}

impl Default for VMA {
    fn default() -> Self {
        VMA{
            start_addr:Default::default(),
            end_addr:Default::default(),
            vm_flags:0,
            pages:vec![],
            phy_pgs_cnt:0
        }
    }
}

impl VMA {
    pub fn new(start_addr:Addr,end_addr:Addr,flags:u8)->Arc<Self>{
        Arc::new(VMA{
            start_addr,
            end_addr,
            vm_flags: flags,
            pages: vec![],
            phy_pgs_cnt:0
        })
    }
    pub fn get_start_addr(&self)->Addr{
        self.start_addr
    }
    pub fn get_end_addr(&self)->Addr{
        self.end_addr
    }
    pub fn in_vma(&self, vaddr: Addr) ->bool{
        vaddr >=self.start_addr&& vaddr <self.end_addr
    }
    pub fn insert_pages(&mut self,pages:Arc<Page>){
        self.phy_pgs_cnt+= pages.get_block_size();
        self.pages.push(pages);
    }
}