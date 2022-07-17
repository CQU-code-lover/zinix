use alloc::sync::{Arc, Weak};
use alloc::vec::Vec;
use core::cell::{Ref, RefCell};
use crate::mm::addr::Addr;
use crate::SpinLock;

lazy_static!{
    static ref PAGES:SpinLock<Vec<Option<Weak<Page>>>> = SpinLock::new(Vec::<Option<Weak<Page>>>::new());
    static ref MMDESC:SpinLock<MmDesc> = SpinLock::new(MmDesc::default());
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

}

fn init_page(start_addr:usize,end_addr:usize){
    let sa = Addr(start_addr).ceil();
    let ea = Addr(end_addr).floor();
    MMDESC.lock().unwrap().start_addr = sa;
    MMDESC.lock().unwrap().start_addr = ea;
    MMDESC.lock().unwrap().sys_reserve_pg_cnt = (ea-sa).get_pg_cnt();
    let pg_cnt = MMDESC.lock().unwrap().sys_reserve_pg_cnt;
    for _ in 0..pg_cnt {
        PAGES.lock().unwrap().push(None);
    }
}

pub fn add_page(page:Arc<Page>){
    PAGES.lock().unwrap().push(Some(Arc::downgrade(&page)));
}

fn test(){
    let p = Page{};
    PAGES.lock().unwrap().push(None);
}