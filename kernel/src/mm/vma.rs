use alloc::collections::LinkedList;
use alloc::sync::Arc;
use alloc::vec;
use alloc::vec::Vec;
use core::cell::RefCell;
use core::default::Default;
use crate::consts::PAGE_SIZE;

use crate::mm::addr::{OldAddr, Paddr, PageAlign, Vaddr};
use crate::utils::order2pages;
use crate::mm::mm::MmStruct;
use crate::mm::page::Page;
use crate::mm::pagetable::{PageTable, PTEFlags};
use crate::SpinLock;

bitflags! {
    pub struct VMAFlags: u8 {
        const VM_READ = 1 << 0;
        const VM_WRITE = 1 << 1;
        const VM_EXEC = 1 << 2;
        const VM_USER = 1<<3;
        const VM_SHARD = 1 << 4;
    }
}

pub struct VMA{
    start_vaddr: Vaddr,
    end_vaddr: Vaddr,
    vm_flags:u8,
    inner:SpinLock<VMAMutInner>
}

struct VMAMutInner{
    // 使用link list有更好的增删性能
    pages:LinkedList<Arc<Page>>,
    phy_pgs_cnt:usize,
    pagetable:Option<Arc<PageTable>>
}

impl Default for VMAMutInner {
    fn default() -> Self {
        Self{
            pages: Default::default(),
            phy_pgs_cnt: 0,
            pagetable:None
        }
    }
}

impl Default for VMA {
    fn default() -> Self {
        VMA{
            start_vaddr:Default::default(),
            end_vaddr:Default::default(),
            vm_flags:0,
            inner :SpinLock::new(VMAMutInner::default())
        }
    }
}

fn _vma_flags_2_pte_flags(f:u8)->u8{
    (f<<1)|PTEFlags::V.bits()
}

impl VMA {
    pub fn new(start_addr: Vaddr, end_addr: Vaddr, pagetable:Arc<PageTable>, flags:u8) ->Arc<Self>{
        Arc::new(VMA{
            start_vaddr: start_addr,
            end_vaddr: end_addr,
            vm_flags: flags,
            inner :SpinLock::new(VMAMutInner{
                pages: Default::default(),
                phy_pgs_cnt: 0,
                pagetable: Some(pagetable)
            })
        })
    }
    pub fn get_start_vaddr(&self) -> Vaddr {
        self.start_vaddr
    }
    pub fn get_end_vaddr(&self) -> Vaddr {
        self.end_vaddr
    }
    pub fn in_vma(&self, vaddr: Vaddr) ->bool{
        vaddr >=self.start_vaddr && vaddr <self.end_vaddr
    }

    // 为什么要返回错误值？ 可能出现映射区域超出vma范围情况
    // 输入参数要求，vaddr align && vaddr in vma
    // 可能的错误： -1 映射物理页超出vma范围
    pub fn map_pages(&self, pages:Arc<Page>, vaddr: Vaddr)->Result<(),isize>{
        debug_assert!(vaddr.is_align());
        debug_assert!(self.in_vma(vaddr));
        let mut inner = self.inner.lock_irq().unwrap();
        let pgs_cnt = order2pages(pages.get_order());
        if (self.get_end_vaddr()-vaddr.0).0 < pgs_cnt*PAGE_SIZE {
            return Err(-1);
        }
        // do map in pagetable
        // vma flags to pte flags
        let pte_flags = _vma_flags_2_pte_flags(self.get_flags());
        inner.pagetable.as_ref().unwrap().map_pages(vaddr,pages.get_vaddr().into(),pages.get_order(),pte_flags);
        inner.phy_pgs_cnt += pgs_cnt;
        inner.pages.push_back(pages);
        Ok(())
    }
    // 只能按照page block的方式unmap
    pub fn unmap_pages(&self, vaddr:Vaddr)->Option<Arc<Page>>{
        let mut inner = self.inner.lock_irq().unwrap();
        // find pgs from link list
        let mut cursor = inner.pages.cursor_front_mut();
        let mut removed:Option<Arc<Page>> = None;
        while cursor.current().is_some() {
            if cursor.current().unwrap().get_vaddr() == vaddr {
                removed = cursor.remove_current();
            }
            cursor.move_next();
        }
        // unmap pagetable
        if removed.is_some() {
            let order = removed.as_ref().unwrap().get_order();
            // bug if unmap pagetable fail
            assert!(self.get_pagetable().unmap_pages(vaddr,order).is_ok());
        }
        removed
    }
    pub fn get_pagetable(&self)->Arc<PageTable>{
        self.inner.lock_irq().unwrap().pagetable.as_ref().unwrap().clone()
    }
    pub fn get_flags(&self)->u8{
        self.vm_flags
    }
}