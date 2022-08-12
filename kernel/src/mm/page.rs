use alloc::collections::linked_list::Iter;
use alloc::collections::LinkedList;
use alloc::rc::Rc;
use alloc::sync::{Arc, Weak};
use alloc::vec;
use alloc::vec::Vec;
use core::borrow::BorrowMut;
use core::cell::{Ref, RefCell};
use core::cmp::min;
use core::default::default;
use core::ops::{Index, IndexMut};
use core::pin::Pin;
use fatfs::{IoBase, Read, Seek, SeekFrom, Write};

use log::set_max_level;

use crate::{println, SpinLock, trace_sync};
use crate::consts::PAGE_SIZE;
use crate::mm::{_insert_area_for_page_drop, trace_global_buddy};
use crate::mm::addr::{Addr, OldAddr, Paddr, PageAlign, PFN, Vaddr};
use crate::pre::{ReadWriteSingleNoOff, InnerAccess, IOReadeWriteSeek, ReadWriteSingleOff};
use crate::utils::order2pages;
use crate::sync::SpinLockGuard;

pub struct PagesManager{
    start_vaddr: Vaddr,
    pages: Vec<Option<Weak<Page>>>,
}

impl Default for PagesManager {
    fn default() -> Self {
        PagesManager{
            start_vaddr: Vaddr(0),
            pages: vec![],
        }
    }
}

impl PagesManager {
    pub fn init(&mut self, start_addr: Vaddr, end_addr: Vaddr){
        let ns = start_addr.ceil();
        let ne = end_addr.floor();
        let pgs = (ne-ns.0).0/PAGE_SIZE;
        self.pages.resize_with(pgs, || {None});
        self.start_vaddr = ns;
    }
    pub fn new(start_addr: Vaddr, end_addr: Vaddr) ->Self{
        let mut pm =Self::default();
        pm.init(start_addr,end_addr);
        pm
    }
    pub fn cap(&self)->usize{
        self.pages.len()
    }
    fn __get_index_no_check(&self, vaddr:Vaddr) ->usize {
        (vaddr-self.start_vaddr.0).0/PAGE_SIZE
    }
    fn __get_index_check(&self,vaddr:Vaddr)->Option<usize>{
        if !vaddr.is_align(){
            return None;
        }
        if vaddr<self.start_vaddr{
            return None;
        }
        if (vaddr-self.start_vaddr.0).0/PAGE_SIZE >= self.cap(){
            return None;
        }
        Some(self.__get_index_no_check(vaddr))
    }
    pub fn get_in_memory_page_cnt(&self) ->usize{
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
    pub fn new_pages_block_in_memory(&mut self, vaddr:Vaddr, order:usize) ->Arc<Page>{
        let index = self.__get_index_no_check(vaddr);
        let pgs = order2pages(order);
        let mut ret:Arc<Page> = Default::default();
        let mut vaddr_probe = vaddr;
        for i in index..index+pgs {
            match &self.pages[i] {
                Some(weak_ptr) => {
                    match  weak_ptr.upgrade() {
                        Some(must_none_pg)=> {
                            panic!("page already in memory: {:#X}",must_none_pg.get_vaddr().0);
                        }
                        _ => {

                        }
                    }
                }
                _ => {

                }
            }
            // alloc page from mem
            let new_pg = Page::new(vaddr_probe,
                                   if i==index{ true } else { false }
            );
            if i == index {
                new_pg.set_order(order);
                ret = new_pg.clone();
            } else {
                ret.add_friend(new_pg.clone());
            }
            self.pages[i] = Some(Arc::downgrade(&new_pg));
            vaddr_probe+=PAGE_SIZE;
        }
        trace_sync!("Alloc Page Vaddr={:#X},order={}",ret.get_vaddr().0,order);
        trace_global_buddy();
        ret
    }
    pub fn get_in_memory_page(&self, vaddr:Vaddr) ->Option<Arc<Page>>{
        match self.__get_index_check(vaddr){
            None => {
                None
            }
            Some(index) => {
                match &self.pages[index] {
                    Some(i) => {
                        i.upgrade()
                    }
                    _ => {
                        None
                    }
                }
            }
        }
    }
}

pub struct Page{
    vaddr:Vaddr,
    default_flag:bool,
    is_leader_flag:bool,
    inner:SpinLock<PageMutInner>
}

pub struct PageMutInner {
    friends:Vec<Arc<Page>>,
    leader:Weak<Page>,
    order:usize,
    pos:usize
}

impl PageMutInner {
    fn change_leader(&mut self,leader:Weak<Page>){
        self.leader = leader;
    }
    pub fn get_friend(&self)->&Vec<Arc<Page>>{
        &self.friends
    }
}

impl Default for Page {
    fn default() -> Self {
        Page{
            vaddr: Vaddr(0),
            default_flag:true,
            is_leader_flag: true,
            inner: SpinLock::new(PageMutInner {
                friends: Default::default(),
                leader: Default::default(),
                order: 0,
                pos: 0
            })
        }
    }
}

struct  PageInterator<'a> {
    len : usize,
    cur : usize,
    pg: &'a Arc<Page>,
}

impl<'a> PageInterator<'a> {
    pub fn new(pg:&'a Arc<Page>)->Self{
        let lock = pg.inner.lock().unwrap();
        let a = lock.friends.iter();
        if pg.is_leader() {
            PageInterator{
                len: lock.friends.len()+1,
                cur: 0,
                pg,
            }
        } else {
            // if this page is not a block,
            // return a null interator
            PageInterator{
                len: 0,
                cur: 0,
                pg,
            }
        }
    }
}

impl<'a> Iterator for PageInterator<'a> {
    type Item = Arc<Page>;

    fn next(&mut self) -> Option<Self::Item> {
        if self.cur<self.len {
            if self.cur==0{
                self.cur+=1;
                Some(self.pg.clone())
            } else {
                let r = Some(self.pg.inner.lock_irq().unwrap().friends[self.cur-1].clone());
                self.cur+=1;
                r
            }
        } else {
            None
        }
    }
}


// todo support multi page rw
impl Page {
    pub fn read(&self, buf: &mut [u8]) -> Result<usize, ()> {
        let pos_now = self.seek(SeekFrom::Current(0)).unwrap();
        let pos_end = (self.get_block_page_cnt() *PAGE_SIZE) as u64;
        let pos_left = (pos_end-pos_now) as usize;
        let buf_len = buf.len();
        let mut inner = self.inner.lock().unwrap();
        let mut real_read_len = min(buf_len, pos_left);
        let read_ret = (self.vaddr + pos_now as usize).read(&mut buf[..real_read_len]);
        match read_ret {
            Ok(v) => {
                inner.pos += v;
                Ok(v)
            }
            Err(e) => {
                Err(e)
            }
        }
    }
}

impl Page {
    pub fn write(&self, buf: &[u8]) -> Result<usize, ()> {
        let pos_now = self.seek(SeekFrom::Current(0)).unwrap();
        let pos_end = (self.get_block_page_cnt() *PAGE_SIZE) as u64;
        let pos_left = (pos_end-pos_now) as usize;
        let buf_len = buf.len();
        let mut inner = self.inner.lock().unwrap();
        let mut real_write_len = min(buf_len,pos_left);
        let write_ret = (self.vaddr + pos_now as usize).write(&buf[..real_write_len]);
        match write_ret {
            Ok(v) => {
                inner.pos += v;
                Ok(v)
            }
            Err(e) => {
                Err(e)
            }
        }
    }

    fn flush(&mut self) -> Result<(), ()> {
        (self.vaddr+self.inner.lock().unwrap().pos).flush()
    }
}

impl Page {
    pub fn seek(&self, pos: SeekFrom) -> Result<u64, ()> {
        let mut inner = self.inner.lock().unwrap();
        let max_pos = (inner.friends.len() +1)*PAGE_SIZE ;
        match pos {
            SeekFrom::Start(v) => {
                if v<=(max_pos as u64){
                    inner.pos = v as usize;
                    Ok(inner.pos as u64)
                }else{
                    Err(())
                }
            }
            SeekFrom::End(v) => {
                match max_pos.checked_add_signed(v as isize){
                    None => {
                        // overflow
                        Err(())
                    }
                    Some(s) => {
                        if s>max_pos || s<0 {
                            Err(())
                        } else {
                            inner.pos = s;
                            Ok(s as u64)
                        }
                    }
                }
            }
            SeekFrom::Current(v) => {
                match inner.pos.checked_add_signed(v as isize){
                    None => {
                        // overflow
                        Err(())
                    }
                    Some(s) => {
                        if s>max_pos{
                            Err(())
                        } else {
                            inner.pos = s;
                            Ok(s as u64)
                        }
                    }
                }
            }
        }
    }
}

impl<T:Copy> ReadWriteSingleOff<T> for Page {
    unsafe fn write_single_off(&self, val: T, off: usize) -> Option<()> {
        if off < self.__get_max_pos() {
            // 注意deadlock
            let lock = self.inner.lock().unwrap();
            (self.get_vaddr()+off).write_single(val)
        } else {
            None
        }
    }

    unsafe fn read_single_off(&self, off: usize) -> Option<T> {
        if off < self.__get_max_pos() {
            let lock = self.inner.lock().unwrap();
            (self.get_vaddr()+off).read_single()
        } else {
            None
        }
    }
}

impl Page {
    pub fn new(vaddr:Vaddr,is_leader:bool)->Arc<Self>{
        let pg = Page{
            vaddr,
            default_flag:false,
            is_leader_flag: is_leader,
            inner: SpinLock::new(PageMutInner {
                friends: Vec::new(),
                leader: Weak::default(),
                order:0,
                pos: 0
            })
        };
        let mut arc_pg = Arc::new(pg);
        arc_pg.inner.lock().unwrap().change_leader(Arc::downgrade(&arc_pg));
        arc_pg
    }
    pub fn __set_vaddr(&mut self, vaddr:Vaddr) {
        self.vaddr = vaddr;
    }
    pub fn __get_max_pos(&self)->usize {
        self.get_block_page_cnt()*PAGE_SIZE
    }
    pub fn clear_one_page(&self){
        let s = self.vaddr.get_inner();
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
        self.inner.lock().unwrap().friends.push(page);
    }
    // can`t use in page`s Drop
    pub fn get_leader(&self)->Arc<Page>{
        self.inner.lock().unwrap().leader.upgrade().unwrap()
    }
    // get the block size count by page count
    pub fn get_block_page_cnt(&self) ->usize {
        let ret = self.inner.lock().unwrap().friends.len() +1;
        debug_assert!(
            if !self.is_leader_flag {
                ret == 1
            } else {
                true
            });
        ret
    }
    pub fn get_order(&self)->usize {
        self.inner.lock().unwrap().order
    }
    pub fn set_order(&self,order:usize){
        self.inner.lock().unwrap().order = order;
    }
    pub fn get_vaddr(&self) ->Vaddr {
        self.vaddr
    }
    pub fn get_paddr(&self) -> Paddr{
        self.vaddr.into()
    }
    // can`t use in page`s Drop
    pub fn is_leader(&self)->bool {
        self.is_leader_flag
    }

    pub fn back(&self)->Arc<Page>{
        if !self.is_leader(){
            panic!("page get back fail");
        }
        let g = self.get_inner_guard();
        if g.friends.is_empty(){
            self.get_leader().clone()
        } else {
            g.friends[g.friends.len()-1].clone()
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
            trace_sync!("drop page vaddr: {:#X}, leader:{},order={}", self.vaddr.0,self.is_leader(),self.get_order());
            // drop for area..
            if self.is_leader() {
                // is leader, push back free area
                let order = self.get_order();
                match _insert_area_for_page_drop(self.vaddr, order) {
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