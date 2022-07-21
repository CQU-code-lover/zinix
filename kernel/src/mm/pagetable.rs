use alloc::sync::Arc;
use alloc::vec;
use alloc::vec::Vec;
use core::borrow::Borrow;
use core::cmp::Ordering;
use log::error;
use riscv::asm::sfence_vma_all;
use crate::consts::PAGE_SIZE;
use crate::mm::addr::{Addr, PFN};
use crate::mm::alloc_pages;
use crate::mm::buddy::order2pages;
use crate::mm::page::Page;
use crate::utils::{addr_get_ppn0, addr_get_ppn1, addr_get_ppn2, get_usize_by_addr, set_usize_by_addr};

extern "C" {
    fn boot_pagetable();
}

#[derive(Clone)]
pub struct PageTable{
    // private pages for pagetable..
    // this pages can`t share with other address space..
    private_pgs: Vec<Arc<Page>>
}

const WalkRetLevelRoot:usize = 0;
const WalkRetLevelMiddle:usize = 1;
const WalkRetLevelLeaf:usize = 2;

pub struct WalkRet{
    level:usize,
    pte_addr:usize,
}

impl PageTable {
    fn _insert_new_pages(&mut self,pgs : Arc<Page>) {
        self.private_pgs.push(pgs)
    }
    // alloc时默认不使用大页
    fn _walk_common(&mut self, vaddr:usize, alloc:bool)->Option<WalkRet>{
        let mut pg_addr = self._get_root_page_addr();
        let ppn_arr = [addr_get_ppn2(vaddr),addr_get_ppn1(vaddr),addr_get_ppn0(vaddr)];
        for i in 0..3{
            let index = ppn_arr[i];
            let entry_addr = pg_addr+index*8;
            let entry =unsafe {get_usize_by_addr(entry_addr)};
            let mut pte = PTE::from(entry);
            // 如果是leaf，不管pte是否valid都能返回
            if pte.is_leaf(){
                return Some(WalkRet{
                    level: i,
                    pte_addr: entry_addr
                });
            }
            if pte.vaild() {
                pg_addr = pte.get_point_paddr();
                continue;
            }
            else {
                // not get next
                if alloc {
                    let pg_arc = alloc_pages(0).unwrap();
                    let addr_usize = pg_arc.get_pfn().0;
                    self._insert_new_pages(pg_arc);
                    pte.set_ppn(addr_usize);
                    //set RWX
                    //此时一定是非叶子页表
                    pte.clear_flags(PTEFlags::R.bits|PTEFlags::W.bits|PTEFlags::X.bits);
                    let new_pte_val = pte.into();
                    // write back pte value
                    unsafe {
                        set_usize_by_addr(entry_addr,new_pte_val)
                    };
                }
                else {
                    return None;
                }
            }
        }
        // bug：三级页表项出现RWX全0情况
        error!("Walk Fault!");
        return None;
    }

    // walk pagetable but not alloc new page.
    // return leaves pagetable`s PTE addr...
    // so we can mapping it...
    pub fn walk(&mut self, vaddr:usize)->Option<WalkRet>{
        return self._walk_common(vaddr,false);
    }
    // walk pagetable and alloc new page when don`t have valid page.
    pub fn walk_alloc(&mut self, vaddr:usize)->WalkRet{
        // alloc是不会返回None，否则会panic
        let r= self._walk_common(vaddr,true);
        match r {
            None=>{
                error!("bug");
                WalkRet{
                    level: 0,
                    pte_addr: 0
                }
            },
            Some(ret)=> ret
        }
    }
    pub fn map_one_page(&mut self, vaddr:Addr, page:&Arc<Page>, flags:u8) {
        let r = self.walk_alloc(vaddr.0);
        match r.level {
            WalkRetLevelLeaf => {
                let pte_val = unsafe { get_usize_by_addr(r.pte_addr) };
                let mut pte = PTE::from(pte_val);
                pte.set_ppn(page.get_pfn().get_addr_usize());
                if pte.vaild() {
                    panic!("this page already have been mapped!");
                }
                pte.set_flags(flags);
                let new_pte_val = pte.into();
                unsafe { set_usize_by_addr(r.pte_addr, new_pte_val) };
            },
            _ => {
                panic!("big page exist in mapped space,map fail");
            }
        }
    }
    // this func can`t check mapped virtual space exist other map.
    // must check by caller
    pub fn map_pages(&mut self,vaddr:Addr, pages:Arc<Page>,flags:u8){
        self.map_one_page(vaddr,&pages,flags);
        for pg in pages.get_inner_guard().get_friend().iter() {
            self.map_one_page(vaddr+Addr(PAGE_SIZE),pg,flags);
        }
    }
    // return the unmap page`s paddr
    // this func is not pub, because unmap one map in
    // a pages block which len is not 1 is not allowed.
    fn _unmap_one_page(&mut self ,vaddr:Addr)->Result<Addr,isize>{
        let mut ret:Result<Addr,isize> = Err(-1);
        let r = self.walk_alloc(vaddr.0);
        match r.level {
            WalkRetLevelLeaf => {
                let pte_val = unsafe { get_usize_by_addr(r.pte_addr) };
                let mut pte = PTE::from(pte_val);
                ret = Ok(Addr(pte.get_point_paddr()));
                pte.set_ppn(0);
                let now_flags = pte.flags;
                let new_mask = !(PTEFlags::V.bits);
                // set invalid
                pte.set_flags(now_flags&new_mask);
                let new_pte_val = pte.into();
                unsafe { set_usize_by_addr(r.pte_addr, new_pte_val) };
            },
            _ => {
                panic!("big page exist in mapped space,unmap fail");
            }
        }
        ret
    }

    // unmap pages.
    // Can only free all of a page block, not a portion of it.
    // otherwise a panic will be report.
    pub fn unmap_pages(&mut self,vaddr:Addr,order:usize)->Addr{
        let pgs = order2pages(order);
        let mut ret_option : Option<Addr> = None;
        let mut vaddr_probe = vaddr;
        for _ in 0..pgs{
            match self._unmap_one_page(vaddr_probe) {
                Ok(v) => {
                    match ret_option {
                        None => {
                            ret_option = Some(v);
                        }
                        Some(_) => {}
                    }
                }
                Err(_) => {
                    // err
                    panic!("pagetable unmap fail");
                }
            }
            vaddr_probe=vaddr_probe+Addr(PAGE_SIZE);
        }
        match ret_option {
            None => {
                panic!("pagetable unmap fail");
            }
            Some(v) => {
                v
            }
        }
    }

    // todo map的pages需要添加到mm空间的表中
    pub fn flush_all(){
        // unsafe {
        //     asm!("vma.flush");
        // }
        unsafe { sfence_vma_all(); }
    }
    fn _get_root_page_addr(& self) ->usize{
        self.private_pgs[0].get_pfn().0
    }
}

impl Default for PageTable {
    fn default() -> Self {
        PageTable{
            // alloc one pages for root page table
            private_pgs:vec![alloc_pages(0).unwrap()]
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

struct PTE{
    flags:u8,
    // rsw:u8, // 只能使用两位
    pnn0:u16, // 只能使用三位
    pnn1:u16, // 只能使用三位
    pnn2:u16, // 只能使用三位
}

impl Into<usize> for PTE {
    fn into(self) -> usize {
        let v:usize = 0;
        v|self.flags as usize|((self.pnn0<<10) as usize)|((self.pnn1<<19) as usize)|((self.pnn2<<28) as usize)
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
    fn _get_bits(&self, flag_mask:u8)->bool{
        return self.flags &flag_mask!=0;
    }
    fn vaild(&self)->bool{
        return self._get_bits(PTEFlags::V.bits);
    }
    fn is_leaf(&self)->bool{
        return !((self._get_bits(PTEFlags::R.bits)==false)&
            (self._get_bits(PTEFlags::W.bits)==false)&
            (self._get_bits(PTEFlags::X.bits)==false));
    }
    fn is_not_leaf(&self)->bool{
        return !self.is_leaf();
    }
    fn set_ppn(&mut self,paddr :usize){
        let np = paddr>>10;
        self.pnn0 = (np&0x1FF) as u16;
        self.pnn1 = ((np>>9)&0x1FF) as u16;
        self.pnn2 = ((np>>18)&0x1FF) as u16;
    }
    fn get_point_paddr(& self)->usize{
        return ((self.pnn0 as usize)|((self.pnn1<<9) as usize)|((self.pnn2<<18) as usize))<<12;
    }
}

pub fn create_kernel_pagetable()->PageTable{
    let kernel_pagetable_root = boot_pagetable as usize;
    // this page is not in PAGE_MANAGER.
    let mut pg = Page::default();
    pg.__set_pfn(PFN(kernel_pagetable_root));
    let kp = PageTable{
        private_pgs: vec![Arc::new(pg)]
    };
    return kp;
}