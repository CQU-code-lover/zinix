use alloc::collections::LinkedList;
use alloc::sync::Arc;
use alloc::vec;
use core::fmt::{Debug, Formatter};

use crate::consts::PAGE_SIZE;
use crate::mm::addr::{Addr, PFN};
use crate::mm::page::Page;
use crate::mm::pagetable::PageTable;
use crate::mm::vma::VMA;

const VMA_CACHE_MAX:usize = 10;

pub struct MmStruct{
    vma_cache:VmaCache,
    pagetable:PageTable,
    vmas : LinkedList<Arc<VMA>>
}

impl Debug for MmStruct {
    fn fmt(&self, f: &mut Formatter<'_>) -> core::fmt::Result {
        writeln!(f,"[Debug MmStruct]");
        for i in self.vmas.iter() {
            writeln!(f,"0x{:x}----0x{:x}",i.get_start_addr().0,i.get_end_addr().0);
        }
        writeln!(f,"[Debug MmStruct End]");
        Ok(())
    }
}

pub struct VmaCache {
    vmas:LinkedList<Arc<VMA>>
}

impl MmStruct {
    pub fn new()->Self{
        MmStruct{
            vma_cache: VmaCache::new(),
            pagetable: PageTable::new_user(),
            vmas: Default::default()
        }
    }
    pub fn find_vma(&mut self, vaddr:Addr)->Option<Arc<VMA>>{
        let check_ret = self.vma_cache.check(vaddr);
        match  check_ret{
            None => {}
            Some(_) => {
                return check_ret;
            }
        }
        let mut ret:Option<Arc<VMA>> = None;
        for vma in &self.vmas {
            if vma.in_vma(vaddr) {
                ret = Some(vma.clone());
                break;
            }
        }
        ret
    }
    pub fn find_vma_intersection(&mut self,start_addr:Addr, end_addr:Addr)->Option<Arc<VMA>>{
        match self.find_vma(end_addr) {
            None => {
                None
            }
            Some(vma) => {
                if vma.get_start_addr()<=start_addr {
                    Some(vma)
                } else {
                    None
                }
            }
        }
    }
    pub fn merge_vmas(&mut self){

    }
    // private func for insert vma.
    // not check if the vma is valid.
    pub fn _insert_vma(&mut self,vma:Arc<VMA>){
        let mut cursor = self.vmas.cursor_front_mut();
        while match cursor.index() {
            Some(_)=>true,
            None=>false
        }{
            if cursor.current().unwrap().get_end_addr()<=vma.get_start_addr(){
                cursor.insert_after(vma);
                return ;
            }
            cursor.move_next();
        }
        panic!("_insert_vma Bug");
    }
    // must page align
    pub fn get_unmapped_area(&mut self,len:usize,flags:u8)->Option<Arc<VMA>>{
        // check len
        if len%PAGE_SIZE!=0{
            panic!("get_unmapped_area fail, len is not page align");
        }
        let mut ret :Option<Arc<VMA>> = None;
        let mut cursor = self.vmas.cursor_front_mut();
        while match cursor.index() {
            Some(_)=>true,
            None=>false
        }{
            let cur_end_addr = cursor.current().unwrap().get_end_addr();
            match cursor.peek_next() {
                None => {
                    // cur is last node
                    // check Mm range
                    // todo check Mm range
                    ret = Some(VMA::new(
                        cur_end_addr,
                        cur_end_addr+Addr(len),
                        flags
                    ));
                    break;
                }
                Some(next) => {
                    if (next.get_start_addr()-cur_end_addr).0 >= len {
                        // have found a valid hole
                        ret = Some(VMA::new(
                            cur_end_addr,
                            cur_end_addr+Addr(len),
                            flags
                        ));
                        break;
                    }
                }
            }
            cursor.move_next();
        }
        ret
    }
}

impl VmaCache {
    fn new()->Self{
        VmaCache{
            vmas: LinkedList::new()
        }
    }
    fn len(&self)->usize {
        self.vmas.len()
    }
    fn push_new_cache(&mut self, vma:Arc<VMA>){
        if self.vmas.len()==VMA_CACHE_MAX{
            self.vmas.pop_back();
        }
        self.vmas.push_front(vma);
    }
    fn check(&mut self,vaddr:Addr)->Option<Arc<VMA>>{
        let mut cursor = self.vmas.cursor_front_mut();
        while match cursor.index() {
            Some(_)=>{
                true
            }
            None=>false
        }{
            // unsafe func to bypass borrow checker?
            // the impl of current is unsafe.
            if cursor.current().unwrap().in_vma(vaddr){
                // cache hit
                let cur = cursor.remove_current().unwrap();
                self.vmas.push_front(cur.clone());
                return Some(cur);
            }
            cursor.move_next();
        }
        // cache miss
        None
    }
}