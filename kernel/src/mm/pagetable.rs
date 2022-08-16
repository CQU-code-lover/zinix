use alloc::sync::Arc;
use alloc::vec;
use alloc::vec::Vec;
use core::arch::riscv64::sfence_vma_vaddr;
use core::borrow::Borrow;
use core::cmp::Ordering;
use core::sync::atomic::AtomicBool;
use fatfs::debug;
use log::log;
use log::error;
use riscv::asm::sfence_vma_all;
use riscv::register::satp::Satp;

use crate::consts::{PAGE_SIZE, PHY_MEM_OFFSET};
use crate::{debug_sync, error_sync, info_sync, println, SpinLock, trace_sync};
use crate::asm::w_satp;
use crate::mm::{alloc_one_page, alloc_pages, get_kernel_pagetable, skernel};
use crate::mm::addr::{OldAddr, Paddr, PageAlign, PFN, Vaddr};
use crate::mm::mm::{MmStruct, VmaCache};
use crate::utils::order2pages;
use crate::mm::page::Page;
use crate::pre::InnerAccess;
use crate::utils::{addr_get_ppn0, addr_get_ppn1, addr_get_ppn2, get_usize_by_addr, set_usize_by_addr};

extern "C" {
    fn boot_pagetable();
}

pub struct PageTable{
    // private pages for pagetable..
    // this pages can`t share with other address space..
    // the lock also used for protect real pagetable in memory
    // pagetable will accessed by irq handler or exc handler. use irq_lock to get lock
    private_pgs: SpinLock<Vec<Arc<Page>>>
}

const WalkRetLevelRoot:usize = 0;
const WalkRetLevelMiddle:usize = 1;
const WalkRetLevelLeaf:usize = 2;

pub struct WalkRet{
    level:usize,
    pub(crate) pte_addr:usize,
}

impl WalkRet {
    pub fn get_pte(&self)->PTE{
        let entry =unsafe {get_usize_by_addr(self.pte_addr)};
        let mut pte = PTE::from(entry);
        pte
    }
    pub fn get_level(&self)->usize{
        self.level
    }
}

impl PageTable {
    pub fn new_user()->Self{
        let mut p = PageTable::default();
        // get_kernel_pagetable().walk(0xFFF0000);
        let root_addr = get_kernel_pagetable()._get_root_page_vaddr().get_inner();
        for i in (0..PAGE_SIZE).filter(|x|{x%8==0}) {
            unsafe {
                ((p._get_root_page_vaddr() + i).0 as *mut u64).write_volatile(
                    ((root_addr + i) as *const u64).read_volatile()
                );
            }
        }
        p
    }
    pub unsafe fn install(&self){
        let p:Paddr = self._get_root_page_vaddr().into();
        let paddr = p.get_inner();
        info_sync!("switch pagetable:{:#X}",paddr);
        let satp_val = ((8 as usize) <<60)|(paddr>>12);
        w_satp(satp_val);
        sfence_vma_all();
    }
    fn _insert_new_pages(&self,pgs : Arc<Page>) {
        self.private_pgs.lock_irq().unwrap().push(pgs)
    }
    // alloc时默认不使用大页
    fn _walk_common(&self, vaddr:usize, alloc:bool)->Option<WalkRet>{
        // walk will access pagetable,use lock..
        let mut pg_vaddr = self._get_root_page_vaddr().get_inner();
        let mut lock = self.private_pgs.lock_irq().unwrap();
        let ppn_arr = [addr_get_ppn2(vaddr),addr_get_ppn1(vaddr),addr_get_ppn0(vaddr)];
        for i in 0..3{
            let index = ppn_arr[i];
            let entry_addr = pg_vaddr +index*8;
            let entry =unsafe {get_usize_by_addr(entry_addr)};
            let mut pte = PTE::from(entry);
            // 如果是leaf，不管pte是否valid都能返回
            trace_sync!("pte:flags={:#b},point-addr={:#X}",pte.flags,pte.get_point_paddr());
            // is_leaf可以判断大页的leaf
            if pte.is_leaf(){
                return Some(WalkRet{
                    level: i,
                    pte_addr: entry_addr
                });
            }
            if i == 2 {
                return Some(WalkRet{
                    level: 2,
                    pte_addr: entry_addr
                })
            }
            if pte.vaild() {
                // pte.clear_flags(
                //     PTEFlags::A.bits|
                //     PTEFlags::D.bits);
                let v:Vaddr = Paddr(pte.get_point_paddr()).into();
                pg_vaddr = v.get_inner();
                unsafe {
                    set_usize_by_addr(entry_addr,pte.into());
                };
                continue;
            }
            else {
                // invalid 的页表项，不是leaf
                if alloc {
                    let pg_arc = alloc_pages(0).unwrap();
                    let allocated_page_vaddr = pg_arc.get_vaddr().get_inner();
                    // 防止deadlock
                    // self._insert_new_pages(pg_arc);
                    lock.push(pg_arc);
                    pte.set_ppn_by_paddr(allocated_page_vaddr -PHY_MEM_OFFSET);
                    //set RWX
                    //此时一定是非叶子页表
                    // pte.clear_flags(PTEFlags::R.bits|
                    //     PTEFlags::W.bits|
                    //     PTEFlags::X.bits|
                    //     PTEFlags::U.bits|
                    //     PTEFlags::A.bits|
                    //     PTEFlags::D.bits);
                    pte.clear_all_flags();
                    pte.set_flags(PTEFlags::V.bits);
                    let new_pte_val = pte.into();
                    // write back pte value
                    unsafe {
                        set_usize_by_addr(entry_addr,new_pte_val)
                    };
                    pg_vaddr = allocated_page_vaddr;
                }
                else {
                    return None;
                }
            }
        }
        error_sync!("Walk Fault!");
        return None;
    }
    pub fn get_kvaddr_by_uvaddr(&self,vaddr:Vaddr)->Option<Vaddr> {
        let off = vaddr.0 % PAGE_SIZE;
        let mut vaddr_ = vaddr.clone();
        vaddr_.align();
        let r = self.walk(vaddr_.get_inner());

        r.map(|x|{
            if x.level == WalkRetLevelLeaf {
                Paddr(x.get_pte().get_point_paddr()+off).into()
            } else {
                todo!()
            }
        })
    }
    //强制映射 可能会破坏大页
    pub fn _force_map_one(&self,vaddr:usize,paddr:usize,map_flag:u8){
        let mut pg_vaddr = self._get_root_page_vaddr().get_inner();
        let mut lock = self.private_pgs.lock_irq().unwrap();
        let ppn_arr = [addr_get_ppn2(vaddr),addr_get_ppn1(vaddr),addr_get_ppn0(vaddr)];
        for i in 0..3{
            let index = ppn_arr[i];
            let entry_addr = pg_vaddr +index*8;
            let entry =unsafe {get_usize_by_addr(entry_addr)};
            let mut pte = PTE::from(entry);
            if i == 2 {
                let mut empty_pte = PTE::from(0);
                empty_pte.set_flags(map_flag);
                empty_pte.set_ppn_by_paddr(paddr);
                // set pagetable
                unsafe { set_usize_by_addr(entry_addr, empty_pte.into()) };
                return;
            }
            if pte.is_leaf()||(!pte.vaild()){
                let pg_arc = alloc_pages(0).unwrap();
                let allocated_page_vaddr = pg_arc.get_vaddr().get_inner();
                // 防止deadlock
                // self._insert_new_pages(pg_arc);
                lock.push(pg_arc);
                pte.set_ppn_by_paddr(allocated_page_vaddr - PHY_MEM_OFFSET);
                let pp = pte.get_point_paddr();
                //clear RWX
                //此时一定是非叶子页表
                pte.clear_flags(PTEFlags::R.bits | PTEFlags::W.bits | PTEFlags::X.bits);
                pte.set_flags(PTEFlags::V.bits);
                let new_pte_val = pte.into();
                // write back pte value
                unsafe {
                    set_usize_by_addr(entry_addr, new_pte_val)
                };
                pg_vaddr = allocated_page_vaddr;
                continue;
            }
            if pte.vaild() {
                pg_vaddr = OldAddr(pte.get_point_paddr()).get_vaddr();
                continue;
            }
        }
        error_sync!("Force Map Fault!");
    }
    pub fn _map_raw_no_check(&self,vaddr:usize,paddr:usize,map_flag:u8){
        assert_eq!(vaddr % PAGE_SIZE, 0);
        assert_eq!(paddr % PAGE_SIZE, 0);
        let r = self.walk_alloc(vaddr);
        match r.level {
            WalkRetLevelLeaf => {
                let pte_val = unsafe { get_usize_by_addr(r.pte_addr) };
                let mut pte = PTE::from(pte_val);
                pte.set_ppn_by_paddr(paddr);
                if pte.vaild() {
                    panic!("this page already have been mapped!");
                }
                pte.set_flags(map_flag);
                let new_pte_val = pte.into();
                unsafe { set_usize_by_addr(r.pte_addr, new_pte_val) };
            },
            _ => {
                panic!("big page exist in mapped space,map fail");
            }
        }
    }

    // walk pagetable but not alloc new page.
    // return leaves pagetable`s PTE addr...
    // so we can mapping it...
    pub fn walk(&self, vaddr:usize)->Option<WalkRet>{
        return self._walk_common(vaddr,false);
    }
    // walk pagetable and alloc new page when don`t have valid page.
    pub fn walk_alloc(&self, vaddr:usize)->WalkRet{
        // alloc是不会返回None，否则会panic
        let r= self._walk_common(vaddr,true);
        match r {
            None=>{
                error_sync!("bug");
                WalkRet{
                    level: 0,
                    pte_addr: 0
                }
            },
            Some(ret)=> ret
        }
    }

    //检查是否未映射最小页面
    pub fn is_not_mapped(&self, vaddr: Vaddr)->bool{
        let r = self.walk_alloc(vaddr.0);

        let lock = self.private_pgs.lock_irq().unwrap();
        match r.level {
            WalkRetLevelLeaf => {
                let pte_val = unsafe { get_usize_by_addr(r.pte_addr) };
                let mut pte = PTE::from(pte_val);
                if pte.vaild() {
                    false
                } else {
                    true
                }
            },
            _ => {
                // todo
                panic!("big page exist in mapped space,can`t check if is mapped");
            }
        }
    }

    pub fn is_not_mapped_order(&self, vaddr: Vaddr,order:usize)->bool {
        for i in vaddr.page_addr_iter(order2pages(order)*PAGE_SIZE) {
            if !self.is_not_mapped(i) {
                return false;
            }
        }
        true
    }

    // 不支持force map，force map可以使用unmap组合实现
    pub fn map_one_page(&self, vaddr: Vaddr, paddr:Paddr, flags:u8)->Result<(),isize> {
        trace_sync!("map one page {:#X}=>{:#X}",vaddr.0,paddr.0);
        let r = self.walk_alloc(vaddr.0);
        let lock = self.private_pgs.lock_irq().unwrap();
        match r.level {
            WalkRetLevelLeaf => {
                let pte_val = unsafe { get_usize_by_addr(r.pte_addr) };
                let mut pte = PTE::from(pte_val);
                let mut new_pte = PTE::default();
                if pte.vaild() {
                    return Err(-1);
                }
                new_pte.set_ppn_by_paddr(paddr.get_inner());
                new_pte.set_flags(flags);
                let new_pte_val = new_pte.into();
                unsafe { set_usize_by_addr(r.pte_addr, new_pte_val) };
            },
            _ => {
                panic!("big page exist in mapped space,map fail");
            }
        }
        // clear tlb entry
        unsafe { sfence_vma_vaddr(vaddr.get_inner()); }
        Ok(())
    }

    // if map exist , error return.
    pub fn map_pages(&self, vaddr: Vaddr, paddr: Paddr, order:usize, flags:u8)->Result<(),isize>{
        let pgs = order2pages(order);
        for i in vaddr.page_addr_iter(pgs*PAGE_SIZE) {
            if !self.is_not_mapped(i){
                return Err(-1);
            }
        }
        for i in 0..pgs {
            let append = i*PAGE_SIZE;
            self.map_one_page(vaddr+append, paddr+append, flags).unwrap();
        }
        Ok(())
    }

    // return the unmap page`s paddr
    // this func is not pub, because unmap one map in
    // a pages block which len is not 1 is not allowed.
    pub fn _unmap_one_page(&self, vaddr: Vaddr) ->Result<Paddr,isize>{
        let mut ret:Result<Paddr,isize> = Err(-1);
        let r = self.walk_alloc(vaddr.0);

        let lock = self.private_pgs.lock_irq().unwrap();
        match r.level {
            WalkRetLevelLeaf => {
                let pte_val = unsafe { get_usize_by_addr(r.pte_addr) };
                let mut pte = PTE::from(pte_val);
                ret = Ok(Paddr(pte.get_point_paddr()));
                pte.set_ppn_by_paddr(0);
                // set invalid
                pte.clear_flags(PTEFlags::V.bits);
                let new_pte_val = pte.into();
                unsafe { set_usize_by_addr(r.pte_addr, new_pte_val) };
            },
            _ => {
                panic!("big page exist in mapped space,unmap fail");
            }
        }
        unsafe { sfence_vma_vaddr(vaddr.get_inner()); }
        ret
    }

    // unmap pages.
    // Can only free all of a page block, not a portion of it.
    // otherwise a panic will be report.
    // todo 检查unmap的paddr是否连续
    pub fn unmap_pages(&self, vaddr: Vaddr, order:usize)->Result<Paddr,isize>{
        let pgs = order2pages(order);
        let mut ret_option : Option<Paddr> = None;
        let mut vaddr_probe = vaddr;
        for _ in 0..pgs{
            match self._unmap_one_page(vaddr_probe) {
                Ok(v) => {
                    if ret_option.is_none(){
                        ret_option = Some(v);
                    }
                }
                Err(e) => {
                    // err
                    return Err(e);
                }
            }
            vaddr_probe += PAGE_SIZE;
        }
        Ok(ret_option.unwrap())
    }

    // todo map的pages需要添加到mm空间的表中
    pub unsafe fn flush_self(&self){
        sfence_vma_all();
    }
    pub fn _get_root_page_vaddr(& self) ->Vaddr{
        self.private_pgs.lock_irq().unwrap()[0].get_vaddr()
    }
}

impl Default for PageTable {
    fn default() -> Self {
        PageTable{
            // alloc one pages for root page table
            private_pgs:SpinLock::new(vec![alloc_pages(0).unwrap()])
        }
    }
}

impl Drop for PageTable {
    fn drop(&mut self) {
        todo!()
    }
}

bitflags! {
    pub struct PTEFlags: u8 {
        const V = 1 << 0;
        const R = 1 << 1;
        const W = 1 << 2;
        const X = 1 << 3;
        const U = 1 << 4;
        const G = 1 << 5;
        const A = 1 << 6;
        const D = 1 << 7;
    }
}

pub struct PTE{
    pub flags:u8,
    // rsw:u8, // 只能使用两位
    pnn0:u16, // 只能使用三位
    pnn1:u16, // 只能使用三位
    pnn2:u16, // 只能使用三位
}

impl Into<usize> for PTE {
    fn into(self) -> usize {
        let v:usize = 0;
        v|self.flags as usize|((self.pnn0 as usize)<<10)|((self.pnn1 as usize)<<19)|((self.pnn2 as usize)<<28)
    }
}

impl From<usize> for PTE {
    fn from(v: usize) -> Self {
        PTE{
            flags:v as u8,
            pnn0:((v>>10)&0x1FF) as u16,
            pnn1:((v>>19)&0x1FF) as u16,
            pnn2:((v>>28)&0x1FF) as u16,
        }
    }
}

impl Default for PTE {
    fn default() -> Self {
        PTE{
            flags:0,
            pnn0:0,
            pnn1:0,
            pnn2:0
        }
    }
}

impl PTE {
    fn set_flags(&mut self,flag_mask:u8){
        self.flags |=flag_mask;
    }
    fn clear_flags(&mut self,flag_mask:u8){
        let n_mask = flag_mask^0xFF;
        self.flags &=n_mask;
    }
    fn clear_all_flags(&mut self){
        self.flags = 0;
    }
    fn _get_bits(&self, flag_mask:u8)->bool{
        return self.flags &flag_mask!=0;
    }
    fn vaild(&self)->bool{
        return self._get_bits(PTEFlags::V.bits);
    }
    fn is_leaf(&self)->bool{
        return !((self._get_bits(PTEFlags::R.bits)==false)&&
            (self._get_bits(PTEFlags::W.bits)==false)&&
            (self._get_bits(PTEFlags::X.bits)==false));
    }
    fn is_not_leaf(&self)->bool{
        return !self.is_leaf();
    }
    fn set_ppn_by_paddr(&mut self, paddr :usize){
        let np = paddr>>12;
        self.pnn0 = (np&0x1FF) as u16;
        self.pnn1 = ((np>>9)&0x1FF) as u16;
        self.pnn2 = ((np>>18)&0x1FF) as u16;
    }
    pub fn get_point_paddr(& self)->usize{
        return ((self.pnn0 as usize)|((self.pnn1 as usize) <<9)|((self.pnn2 as usize)<<18))<<12;
    }
}

pub fn create_kernel_pagetable()->PageTable{
    let kernel_pagetable_root = boot_pagetable as usize;
    // this page is not in PAGE_MANAGER.
    let mut pg = Page::default();
    pg.__set_vaddr(Vaddr(kernel_pagetable_root));
    let kp = PageTable{
        private_pgs: SpinLock::new(vec![Arc::new(pg)])
    };
    return kp;
}

pub fn create_kernel_mm()->MmStruct {
    MmStruct::new_kern_mm_by_pagetable(create_kernel_pagetable())
}