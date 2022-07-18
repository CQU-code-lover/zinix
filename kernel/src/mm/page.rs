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
use crate::mm::addr::{Addr, PFN};
use crate::mm::buddy::order2pages;
use crate::SpinLock;

lazy_static!{
    // static ref PAGES:SpinLock<Vec<Option<Weak<Page>>>> = SpinLock::new(Vec::<Option<Weak<Page>>>::new());
    // static ref MMDESC:SpinLock<MmDesc> = SpinLock::new(MmDesc::default());
}

pub struct PagesManager{
    first_pfn: PFN,
    pages: Vec<Option<Weak<Page>>>,
}

impl PagesManager {
    pub fn new(start_addr:Addr,end_addr:Addr)->Self{
        let ns = start_addr.ceil();
        let ne = end_addr.floor();
        let pgs = (ne-ns).get_pg_cnt();
        let mut pm = PagesManager{
            first_pfn: PFN::from(ns),
            pages: vec![],
        };
        pm.pages.resize_with(pgs, || {None});
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
    pub fn _new_pages_block_in_memory(&mut self, pfn:PFN, order:usize) ->Arc<Page>{
        let index = (pfn -self.first_pfn).0;
        let pgs = order2pages(order);
        let mut ret:Arc<Page> = Default::default();
        let mut pfn_probe = pfn;
        for i in index..index+pgs {
            match &self.pages[i] {
                Some(weak_ptr) => {
                    match  weak_ptr.upgrade() {
                        Some(_)=> {
                            panic!("page already in memory");
                        }
                        _ => {

                        }
                    }
                }
                _ => {

                }
            }
            // alloc page from mem
            let new_pg = Page::new(pfn_probe);
            if i == index {
                ret = new_pg.clone();
            } else {
                ret.add_friend(new_pg.clone());
            }
            self.pages[i] = Some(Arc::downgrade(&new_pg));
            pfn_probe.step_one();
        }
        ret
    }
    pub fn get_page_ref(&self,pfn:PFN)->Option<Arc<Page>>{
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

pub struct MmDesc {
    start_addr : Addr,
    end_addr :Addr,
    sys_reserve_pg_cnt: usize
}

impl MmDesc {
    pub fn default()->MmDesc{
        return MmDesc{
            start_addr: Addr(0),
            end_addr: Addr(0),
            sys_reserve_pg_cnt: 0
        }
    }
}

pub struct Page{
    pfn:PFN,
    inner:RefCell<PageMutInner>
}

struct PageMutInner {
    friends:LinkedList<Arc<Page>>,
    leader:Weak<Page>
}

impl PageMutInner {
    fn change_leader(&mut self,leader:Weak<Page>){
        self.leader = leader;
    }
}

impl Default for Page {
    fn default() -> Self {
        Page{
            pfn: PFN(0),
            inner: RefCell::new(PageMutInner {
                friends: Default::default(),
                leader: Default::default()
            })
        }
    }
}

impl Page {
    pub fn new(pfn:PFN)->Arc<Self>{
        let pg = Page{
            pfn:pfn,
            inner: RefCell::new(PageMutInner {
                friends: LinkedList::new(),
                leader: Weak::default(),
            })
        };
        let mut arc_pg = Arc::new(pg);
        arc_pg.inner.borrow_mut().change_leader(Arc::downgrade(&arc_pg));
        arc_pg
    }
    pub fn have_friends(&self)->bool{
        self.inner.borrow_mut().friends.len() != 0
    }
    pub fn add_friend(&self, page:Arc<Page>){
        page.inner.borrow_mut().change_leader(Arc::downgrade(&self.get_leader()));
        self.inner.borrow_mut().friends.push_back(page);
    }
    pub fn get_leader(&self)->Arc<Page>{
        self.inner.borrow_mut().leader.upgrade().unwrap()
    }
    // get the block size count by page counte
    pub fn get_block_size(&self)->usize {
        self.inner.borrow().friends.len()
    }
}

fn page_init(start_addr:usize, end_addr:usize){
    // let sa = Addr(start_addr).ceil();
    // let ea = Addr(end_addr).floor();
    // MMDESC.lock().unwrap().start_addr = sa;
    // MMDESC.lock().unwrap().start_addr = ea;
    // MMDESC.lock().unwrap().sys_reserve_pg_cnt = (ea-sa).get_pg_cnt();
    // let pg_cnt = MMDESC.lock().unwrap().sys_reserve_pg_cnt;
    // for _ in 0..pg_cnt {
    //     PAGES.lock().unwrap().push(None);
    // }
}

