use alloc::collections::LinkedList;
use alloc::sync::Arc;
use alloc::vec;
use crate::mm::addr::Addr;
use crate::mm::pagetable::PageTable;
use crate::mm::vma::VMA;

const VMA_CACHE_MAX:usize = 10;

pub struct MmStruct{
    vma_cache:VmaCache,
    pagetable:PageTable,
    vmas : LinkedList<Arc<VMA>>
}

pub struct VmaCache {
    vmas:LinkedList<Arc<VMA>>
}

impl MmStruct {
    pub fn find_vma(addr:Addr){

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