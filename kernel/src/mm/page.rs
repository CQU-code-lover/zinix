use alloc::collections::linked_list::Iter;
use alloc::collections::LinkedList;
use alloc::rc::Rc;
use alloc::sync::{Arc, Weak};
use alloc::vec;
use alloc::vec::Vec;
use core::borrow::BorrowMut;
use core::cell::{Ref, RefCell};
use core::default::default;
use core::pin::Pin;

use log::set_max_level;

use crate::{println, SpinLock, trace_sync};
use crate::consts::PAGE_SIZE;
use crate::mm::{_insert_area_for_page_drop, trace_global_buddy};
use crate::mm::addr::{Addr, PFN};
use crate::mm::buddy::order2pages;
use crate::sync::SpinLockGuard;

pub struct PagesManager{
    first_pfn: PFN,
    pages: Vec<Option<Weak<Page>>>,
}

impl Default for PagesManager {
    fn default() -> Self {
        PagesManager{
            first_pfn: PFN(0),
            pages: vec![],
        }
    }
}

impl PagesManager {
    pub fn init(&mut self, start_addr:Addr,end_addr:Addr){
        let ns = start_addr.ceil();
        let ne = end_addr.floor();
        let pgs = (ne-ns).get_pg_cnt();
        self.pages.resize_with(pgs, || {None});
        self.first_pfn = PFN::from(ns);
    }
    pub fn new(start_addr:Addr,end_addr:Addr)->Self{
        let mut pm =Self::default();
        pm.init(start_addr,end_addr);
        pm
    }
    pub fn cap(&self)->usize{
        self.pages.len()
    }
    pub fn in_memory_pages(&self)->usize{
        let mut cnt = 0;
        for i in &self.pages {
            match i {
                Some(inner) =>{
                    match inner.upgrade() {
                        None => {}
                        Some(_) => {
                            cnt +=1;
                        }
                    }
                }
                _ =>{

                }
            }
        }
        cnt
    }
    // this func not check pfn and order and the pfn align..
    // do check before invoke this func
    pub fn new_pages_block_in_memory(&mut self, pfn:PFN, order:usize) ->Arc<Page>{
        let index = (pfn -self.first_pfn).0;
        let pgs = order2pages(order);
        let mut ret:Arc<Page> = Default::default();
        let mut pfn_probe = pfn;
        for i in index..index+pgs {
            match &self.pages[i] {
                Some(weak_ptr) => {
                    match  weak_ptr.upgrade() {
                        Some(must_none_pg)=> {
                            panic!("page already in memory: {:#X}",must_none_pg.get_pfn().0);
                        }
                        _ => {

                        }
                    }
                }
                _ => {

                }
            }
            // alloc page from mem
            let new_pg = Page::new(pfn_probe,
                if i==index{ true } else { false }
            );
            if i == index {
                new_pg.set_order(order);
                ret = new_pg.clone();
            } else {
                ret.add_friend(new_pg.clone());
            }
            self.pages[i] = Some(Arc::downgrade(&new_pg));
            pfn_probe.step_one();
        }
        trace_sync!("Alloc Page PFN={:#X},order={}",ret.get_pfn().0,order);
        trace_global_buddy();
        ret
    }
    pub fn get_page_arc(&self,pfn:PFN)->Option<Arc<Page>>{
        if pfn>=self.first_pfn&&(pfn-self.first_pfn).0<self.pages.len(){
            match &self.pages[(pfn-self.first_pfn).0] {
                Some(i) => {
                    i.upgrade()
                }
                _ => {
                    None
                }
            }
        } else {
            None
        }
    }
}

pub struct Page{
    pfn:PFN,
    default_flag:bool,
    is_leader_flag:bool,
    inner:SpinLock<PageMutInner>
}

pub struct PageMutInner {
    friends:LinkedList<Arc<Page>>,
    leader:Weak<Page>,
    order:usize
}

pub struct PageWriter<'a>(usize, SpinLockGuard<'a,PageMutInner>);
pub struct PageReader<'a>(usize, SpinLockGuard<'a,PageMutInner>);

impl<'a> PageWriter<'a> {
    pub fn read_volatile<T:Sized>(&self,off:usize)->T{
        unsafe { ((self.0 + off) as *const T).read_volatile() }
    }
    pub fn write_volatile<T:Sized>(&self,off:usize,val:T){
        unsafe { ((self.0 + off) as *mut T).write_volatile(val) }
    }
}

impl<'a,> PageReader<'a> {
    pub fn read_volatile<T:Sized>(&self,off:usize)->T{
        unsafe { ((self.0 + off) as *const T).read_volatile() }
    }
}

impl PageMutInner {
    fn change_leader(&mut self,leader:Weak<Page>){
        self.leader = leader;
    }
    pub fn get_friend(&self)->&LinkedList<Arc<Page>>{
        &self.friends
    }
}

impl Default for Page {
    fn default() -> Self {
        Page{
            pfn: PFN(0),
            default_flag:true,
            is_leader_flag: true,
            inner: SpinLock::new(PageMutInner {
                friends: Default::default(),
                leader: Default::default(),
                order: 0
            })
        }
    }
}

// struct  PageInterator {
//     len : usize,
//     cur : usize,
//     pg: Arc<Page>,
//     pl_iter: Iter<Arc<Page>>
// }
//
// impl PageInterator {
//     pub fn new(pg:&Page)->Self{
//         let n = pg.get_friends();
//         let a = n.iter();
//         if pg.is_leader() {
//
//         } else {
//             // if this page is not a block,
//             // return a null interator
//             PageInterator{
//                 len: 0,
//                 cur: 0,
//                 pg: Arc::new(Default::default())
//             }
//         }
//     }
// }
//
// impl Iterator for PageInterator {
//     type Item = Arc<Page>;
//
//     fn next(&mut self) -> Option<Self::Item> {
//         let mut ret = None;
//         let iter = self.
//         if cur==0{
//             ret = Some(self.pg.clone());
//         } else if cur<len {
//
//         }
//     }
// }

impl Page {
    pub fn new(pfn:PFN,is_leader:bool)->Arc<Self>{
        let pg = Page{
            pfn:pfn,
            default_flag:false,
            is_leader_flag: is_leader,
            inner: SpinLock::new(PageMutInner {
                friends: LinkedList::new(),
                leader: Weak::default(),
                order:0,
            })
        };
        let mut arc_pg = Arc::new(pg);
        arc_pg.inner.lock().unwrap().change_leader(Arc::downgrade(&arc_pg));
        arc_pg
    }
    pub fn __set_pfn(&mut self,pfn:PFN) {
        self.pfn = pfn;
    }
    pub fn clear_one_page(&self){
        let s = self.pfn.get_addr_usize();
        for addr in (s..s+PAGE_SIZE).filter(|x| {
            (*x) %8 == 0
        }) {
            unsafe { (addr as *mut u64).write_volatile(0); }
        }
    }
    pub fn clear_pages_block(&self){
        self.clear_one_page();
        for v in self.inner.lock().unwrap().friends.iter(){
            v.clear_one_page();
        }
    }
    pub fn get_inner_guard(&self) ->SpinLockGuard<PageMutInner>{
        self.inner.lock().unwrap()
    }
    pub fn have_friends(&self)->bool{
        self.inner.lock().unwrap().friends.len() != 0
    }
    pub fn add_friend(&self, page:Arc<Page>){
        page.inner.lock().unwrap().change_leader(Arc::downgrade(&self.get_leader()));
        self.inner.lock().unwrap().friends.push_back(page);
    }
    // can`t use in page`s Drop
    pub fn get_leader(&self)->Arc<Page>{
        self.inner.lock().unwrap().leader.upgrade().unwrap()
    }
    // get the block size count by page count
    pub fn get_block_size(&self)->usize {
        self.inner.lock().unwrap().friends.len() +1
    }
    pub fn get_order(&self)->usize {
        self.inner.lock().unwrap().order
    }
    pub fn set_order(&self,order:usize){
        self.inner.lock().unwrap().order = order;
    }
    pub fn get_pfn(&self)->PFN {
        self.pfn
    }
    // can`t use in page`s Drop
    pub fn is_leader(&self)->bool {
        self.is_leader_flag
    }

    // pub fn iter(&self)->PageInterator{
    //     PageInterator::new(self)
    // }

    // the PageReader will hold PageInner lock
    pub fn get_page_writer(&self) -> PageWriter {
        PageWriter(self.pfn.get_addr_usize(), self.get_inner_guard())
    }
    pub fn get_page_reader(&self) -> PageReader {
        PageReader(self.pfn.get_addr_usize(), self.get_inner_guard())
    }
    pub fn back(&self)->Arc<Page>{
        if !self.is_leader(){
            panic!("page get back fail");
        }
        let g = self.get_inner_guard();
        if g.friends.is_empty(){
            self.get_leader().clone()
        } else {
            g.friends.back().unwrap().clone()
        }
    }
    pub fn front(&self)->Arc<Page>{
        if !self.is_leader(){
            panic!("page get front fail");
        }
        self.get_leader().clone()
    }
}

impl Drop for Page {
    fn drop(&mut self) {
        if !self.default_flag {
            trace_sync!("drop page PFN: {:#X}, leader:{},order={}", self.pfn.0,self.is_leader(),self.get_order());
            // drop for area..
            if self.is_leader() {
                // is leader, push back free area
                let order = self.get_order();
                match _insert_area_for_page_drop(self.pfn,order) {
                    Ok(_)=>{}
                    Err(_)=>{
                        // Bug Report
                        panic!("page drop fail");
                    }
                }
            }
        }
    }
}

//
// pub fn page_test() {
//     page_init(Addr(0),Addr(0x100000));
//     {
//         let ret1 = PAGES_MANAGER.lock().unwrap().new_pages_block_in_memory(PFN(0),2);
//     }
//     println!("{}",PAGES_MANAGER.lock().unwrap().in_memory_pages());
//     let ret2= PAGES_MANAGER.lock().unwrap().new_pages_block_in_memory(PFN(0),2);
//     println!("{}",PAGES_MANAGER.lock().unwrap().in_memory_pages());
// }