use alloc::sync::Arc;
use alloc::vec;
use alloc::vec::Vec;
use core::cell::RefCell;
use crate::consts::PAGE_SIZE;

use crate::mm::addr::Addr;
use crate::mm::buddy::order2pages;
use crate::mm::mm::MmStruct;
use crate::mm::page::Page;
use crate::mm::pagetable::PageTable;
use crate::SpinLock;

bitflags! {
    pub struct VMAFlags: u8 {
        const VM_READ = 1 << 0;
        const VM_WRITE = 1 << 1;
        const VM_EXEC = 1 << 2;
        const VM_SHARD = 1 << 3;
        const VM_USER = 1<<4;
    }
}

pub struct VMA{
    start_addr:Addr,
    end_addr:Addr,
    vm_flags:u8,
    inner:SpinLock<VMAMutInner>
}

struct VMAMutInner{
    pages:Vec<Arc<Page>>,
    phy_pgs_cnt:usize,
    pagetable:Option<Arc<PageTable>>
}

impl Default for VMAMutInner {
    fn default() -> Self {
        Self{
            pages: vec![],
            phy_pgs_cnt: 0,
            pagetable:None
        }
    }
}

impl Default for VMA {
    fn default() -> Self {
        VMA{
            start_addr:Default::default(),
            end_addr:Default::default(),
            vm_flags:0,
            inner :SpinLock::new(VMAMutInner::default())
        }
    }
}

fn _vma_flags_2_pte_flags(f:u8)->u8{0}

impl VMA {
    pub fn new(start_addr:Addr,end_addr:Addr,pagetable:Arc<PageTable>,flags:u8)->Arc<Self>{
        Arc::new(VMA{
            start_addr,
            end_addr,
            vm_flags: flags,
            inner :SpinLock::new(VMAMutInner{
                pages: vec![],
                phy_pgs_cnt: 0,
                pagetable: Some(pagetable)
            })
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
    pub fn map_pages(&self,pages:Arc<Page>,vaddr: Addr)->Result<(),isize>{
        // check arg
        if !vaddr.is_align() {
            return Err(-1);
        }
        if !self.in_vma(vaddr) {
            return Err(-1);
        }
        let mut inner = self.inner.lock_irq().unwrap();
        let pgs_cnt = order2pages(pages.get_order());
        if (self.get_end_addr()-vaddr).0 < pgs_cnt*PAGE_SIZE {
            return Err(-1);
        }
        // do map in pagetable
        // vma flags to pte flags
        let pte_flags = _vma_flags_2_pte_flags(self.get_flags());
        inner.pagetable.as_ref().unwrap().map_pages(vaddr,pages.clone(),pte_flags);
        inner.phy_pgs_cnt += pgs_cnt;
        inner.pages.push(pages);
        Ok(())
    }
    pub fn get_pagetable(&self)->Arc<PageTable>{
        self.inner.lock_irq().unwrap().pagetable.as_ref().unwrap().clone()
    }
    pub fn get_flags(&self)->u8{
        self.vm_flags
    }
}