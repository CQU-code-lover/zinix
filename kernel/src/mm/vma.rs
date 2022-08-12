use alloc::collections::LinkedList;
use alloc::sync::Arc;
use alloc::vec;
use alloc::vec::Vec;
use core::cell::RefCell;
use core::default::Default;
use core::fmt::{Debug, Formatter};
use core::mem::size_of;
use fatfs::{Read, Seek, SeekFrom, Write};
use crate::consts::PAGE_SIZE;

use crate::mm::addr::{OldAddr, Paddr, PageAlign, Vaddr};
use crate::mm::alloc_one_page;
use crate::utils::order2pages;
use crate::mm::mm::MmStruct;
use crate::mm::page::Page;
use crate::mm::pagetable::{PageTable, PTEFlags};
use crate::pre::{ReadWriteOffUnsafe, ReadWriteSingleNoOff, ReadWriteSingleOff};
use crate::{println, SpinLock};
use crate::fs::inode::Inode;

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

impl Debug for VMA {
    fn fmt(&self, f: &mut Formatter<'_>) -> core::fmt::Result {
        writeln!(f,"range:{:#X}=>{:#X}",self.start_vaddr.0,self.end_vaddr.0);
        writeln!(f,"flags:{:b}",self.vm_flags);
        Ok(())
    }
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
    //           -1 虚拟页存在映射
    // todo 支持force map
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
        if !inner.pagetable.as_ref().unwrap().is_not_mapped_order(vaddr,pages.get_order()){
            return Err(-1);
        }
        inner.pagetable.as_ref().unwrap().map_pages(vaddr,pages.get_vaddr().into(),pages.get_order(),pte_flags).unwrap();
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
    // 相对map pages来说在存在映射时可以不分配物理页并且跳过，这样速度更快
    pub fn fast_alloc_one_page(&self,vaddr:Vaddr){
        assert!(self.in_vma(vaddr));
        let mut inner = self.inner.lock_irq().unwrap();
        if inner.pagetable.as_ref().unwrap().is_not_mapped(vaddr) {
            let pages = alloc_one_page().unwrap();
            inner.pagetable.as_ref().unwrap().map_one_page(vaddr, pages.get_paddr(), _vma_flags_2_pte_flags(self.get_flags()));
            inner.pages.push_back(pages);
        }
    }
    // 注意这个分配物理页不一定是连续的
    pub fn fast_alloc_pages(&self,vaddr:Vaddr,order:usize){
        for i in vaddr.page_addr_iter(order2pages(order)*PAGE_SIZE){
            self.fast_alloc_one_page(i);
        }
    }
    pub fn find_page(&self,vaddr:Vaddr)->Option<Arc<Page>> {
        for i in self.inner.lock_irq().unwrap().pages.iter(){
            if i.get_vaddr() == vaddr {
                return Some(i.clone());
            }
        }
        return None;
    }
    pub fn fast_alloc_one_page_and_get(&self,vaddr:Vaddr)->Arc<Page>{
        assert!(self.in_vma(vaddr));
        let mut inner = self.inner.lock_irq().unwrap();
        if inner.pagetable.as_ref().unwrap().is_not_mapped(vaddr) {
            let pages = alloc_one_page().unwrap();
            inner.pagetable.as_ref().unwrap().map_one_page(vaddr, pages.get_paddr(), self.get_flags());
            inner.pages.push_back(pages.clone());
            return pages;
        } else {
            let kavddr = inner.pagetable.as_ref().unwrap().get_kvaddr_by_uvaddr(vaddr).unwrap();
            for i in inner.pages.iter() {
                if i.get_vaddr() == kavddr {
                    return i.clone();
                }
            }
            panic!("can`t find mapped page");
        }
    }
}

// todo 安全性 是否需要加锁才能访问page
impl ReadWriteOffUnsafe<u8> for VMA {
    unsafe fn read_off(&self, buf: &mut [u8], off: usize) -> usize {
        let size = 1;
        let buf_size = buf.len() * size;
        assert!(Vaddr(off).is_align_n(size));
        assert!(self.start_vaddr+buf_size+off < self.end_vaddr);
        assert!(self.start_vaddr+off < self.end_vaddr);
        let start = self.start_vaddr;
        let mut page_now = self.fast_alloc_one_page_and_get(start);
        page_now.seek(SeekFrom::Start(off as u64));
        let mut buf_index:usize = 0;
        while buf_index < buf.len() {
            let read_len = page_now.read(&mut buf[buf_index..]).unwrap();
            if read_len == 0 {
                // change pages
                let vaddr_now = start+off + buf_index*size;
                if buf_index!=buf.len(){
                    assert!(vaddr_now.is_align());
                }
                page_now = self.fast_alloc_one_page_and_get(vaddr_now);
                page_now.seek(SeekFrom::Start(0));
            } else {
                buf_index+=read_len;
            }
        }
        buf_size
    }

    unsafe fn write_off(&self, buf: &[u8], off: usize) -> usize {
        let size = 1;
        let buf_size = buf.len() * size;
        assert!(Vaddr(off).is_align_n(size));
        assert!(self.start_vaddr+buf_size+off < self.end_vaddr);
        assert!(self.start_vaddr+off < self.end_vaddr);
        let start = self.start_vaddr;
        let mut page_now = self.fast_alloc_one_page_and_get(start);
        page_now.seek(SeekFrom::Start(off as u64));
        let mut buf_index:usize = 0;
        while buf_index < buf.len() {
            let write_len = page_now.write(&buf[buf_index..]).unwrap();
            if write_len == 0 {
                // change pages
                let vaddr_now = start+off + buf_index*size;
                if buf_index!=buf.len(){
                    assert!(vaddr_now.is_align());
                }
                page_now = self.fast_alloc_one_page_and_get(vaddr_now);
                page_now.seek(SeekFrom::Start(0));
            } else {
                buf_index+= write_len;
            }
        }
        buf_size
    }
}

pub struct VmaInode{
    inode:Arc<Inode>,
    off:usize,

}