use alloc::collections::LinkedList;
use alloc::string::String;
use alloc::sync::Weak;
use alloc::vec;
use alloc::vec::Vec;
use core::fmt::{Debug, Formatter, write};
use core::ops::Add;

use log::{error, info, log_enabled, set_max_level, warn};
use riscv::interrupt::free;

use crate::{cpu_local, info_sync, println, SpinLock};
use crate::consts::{MAX_ORDER, PAGE_OFFSET, PAGE_SIZE};
use crate::mm::addr::{Addr, PFN};
use crate::mm::bitmap::{Bitmap, bitmap_test};
use crate::mm::page::Page;
use crate::sbi::shutdown;

pub struct BuddyItem {
    first_page: PFN,
    pg_cnt : usize
}

pub struct BuddyAllocator {
    free_areas:Vec<FreeArea>,
    first_pfn : PFN
}

pub fn order2pages(order:usize)->usize{
    return if order < MAX_ORDER {
        1 << order
    } else {
        0
    }
}

impl Default for BuddyAllocator {
    fn default()->Self {
        return BuddyAllocator{
            free_areas: Vec::new(),
            first_pfn : PFN::from(0)
        };
    }
}

impl BuddyAllocator{
    pub fn free_pages_cnt(&self)-> usize{
        let mut pgs_cnt = 0;
        for i in 0..MAX_ORDER {
            let pg = self.free_areas[i].free_cnt * order2pages(i);
            pgs_cnt += pg;
        }
        return pgs_cnt;
    }
    fn _init_areas(&mut self,start_pfn:PFN,pg_cnt:usize){
        self.first_pfn = start_pfn;
        let mut pfn_probe = start_pfn;
        let pfn_end = start_pfn.clone().step_n(pg_cnt);
        for _ in 0..MAX_ORDER {
            self.free_areas.push(FreeArea::default());
        }
        for i in (0..MAX_ORDER).rev() {
            let free_area = & mut self.free_areas[i];
            free_area.order = i;
            free_area.first_pfn = start_pfn;
            let pgs = order2pages(i);
            let mut bitmap_len = pg_cnt/pgs;
            if pg_cnt%pgs != 0{
                bitmap_len += 1;
            }
            free_area.bitmap = Bitmap::new(bitmap_len);
            while pfn_probe.clone().step_n(pgs) <= pfn_end {
                match free_area.insert_area(BuddyItem{
                    first_page: pfn_probe,
                    pg_cnt:pgs
                }) {
                    Some(_) => {
                        panic!("init free area fail!");
                    }
                    _ => {}
                }
                pfn_probe.step_n(pgs);
            }
        }
    }
    pub fn init(&mut self,s_addr:Addr,e_addr:Addr){
        let ns_addr = s_addr.ceil();
        let ne_addr = e_addr.floor();
        let pg_cnt = (ne_addr - ns_addr).get_pg_cnt();
        self._init_areas(PFN::from(ns_addr),pg_cnt);
    }
    pub fn new(s_addr:Addr,e_addr:Addr)->BuddyAllocator{
        let mut b = BuddyAllocator::default();
        b.init(s_addr,e_addr);
        return b;
    }
    pub fn alloc_area(&mut self, order:usize) ->Result<PFN,isize>{
        if order>=MAX_ORDER{
            return Err(-1);
        } else {
            match self.free_areas[order].alloc_area() {
                Ok(item) => {
                    // find one
                    return Ok(item.first_page);
                }
                _ => {
                    match self.alloc_area(order+1) {
                        Ok(pfn) => {
                            // alloc OK from high order
                            let pgs = order2pages(order);
                            let re_insert_pfn = pfn.clone().step_n(pgs);
                            self.free_areas[order].insert_area(BuddyItem {
                                first_page: re_insert_pfn,
                                pg_cnt: pgs,
                            });
                            return Ok(pfn);
                        }
                        _ => {
                            return Err(-1);
                        }
                    }
                }
            }
        }
    }
    pub fn free_area(&mut self, pfn:PFN, order:usize) ->Result<(),isize>{
        // check order and align
        if order>=MAX_ORDER {
            return Err(-1);
        }
        if (pfn - self.free_areas[order].first_pfn).0 % order2pages(order) !=0 {
            return Err(-1);
        }
        if (self.free_areas[order].first_pfn > pfn) {
            return Err(-1);
        }
        let mut  buddy_item = BuddyItem{
            first_page: pfn,
            pg_cnt: order2pages(order)
        };
        for i in order..MAX_ORDER {
            match self.free_areas[i].insert_area(buddy_item) {
                Some(n) => {
                    buddy_item = n;
                }
                None => {
                    return Ok(());
                }
            }
        }
        return Err(-1);
    }
    pub fn alloc_pages(order:usize)->Option<Addr>{
        None
    }
    pub fn free_pages(addr:Addr, order:usize){

    }
}

impl Debug for BuddyAllocator {
    fn fmt(&self, f: &mut Formatter<'_>) -> core::fmt::Result {
        writeln!(f,"[BuddyAllocator Debug]");
        writeln!(f,"Start Addr: {}",self.first_pfn.0 << PAGE_OFFSET);
        let mut pgs_cnt = 0;
        for i in 0..MAX_ORDER {
            let pg = self.free_areas[i].free_cnt * order2pages(i);
            pgs_cnt += pg;
        }
        writeln!(f,"Freed Pages: {}", pgs_cnt);
        for i in 0..MAX_ORDER {
            write!(f,"Order{}: ",i);
            for j in &self.free_areas[i].list {
                write!(f,"{:X}\t",j.first_page.0);
            }
            writeln!(f,"");
        }
        writeln!(f,"[BuddyAllocator Debug End]");
        core::fmt::Result::Ok(())
    }
}

pub struct FreeArea {
    list : LinkedList<BuddyItem>,
    bitmap: Bitmap,
    first_pfn : PFN,
    free_cnt: usize,
    order: usize
}

impl FreeArea {
    fn default() ->FreeArea{
        FreeArea{
            list: LinkedList::new(),
            bitmap: Bitmap::default(),
            first_pfn: PFN(0),
            free_cnt: 0,
            order: 0
        }
    }
    fn len(&self)->usize{
        return self.list.len();
    }
    fn alloc_area(&mut self)->Result<BuddyItem,isize>{
        if self.free_cnt == 0 {
            // no free area..
            Err(-1)
        } else {
            self.free_cnt -= 1;
            let ret = self.list.pop_back().unwrap();
            match  self._get_bitmap_index(ret.first_page) {
                Ok(index) => {
                    self.bitmap.turn_over(index);
                }
                _ => {
                    panic!("get buddy item bitmap index fail..");
                }
            }
            Ok(ret)
        }
    }
    // this function can`t check the align of area,
    // so check args before invoke this func.
    fn insert_area(&mut self,item:BuddyItem)->Option<BuddyItem>{
        self.free_cnt += 1;
        match self._get_bitmap_index(item.first_page) {
            Ok(index) => {
                // check bitmap
                let is_set = self.bitmap.get(index);
                let is_max = (self.order == (MAX_ORDER-1));
                if is_set && (!is_max){
                    // combime..
                    let target_pfn = self._get_buddy_PFN(item.first_page);
                    let mut removed_index = self.list.len();
                    let mut removed_acc:usize = 0;
                    for area in self.list.iter() {
                        if target_pfn == area.first_page {
                            removed_index = removed_acc;
                            break;
                        }
                        removed_acc+=1;
                    }
                    // if not find target buddy item, the removed index eq to len of list
                    // which will lead panic...
                    let removed_item = self.list.remove(removed_index);
                    self.free_cnt -= 2;
                    self.bitmap.clear(index);
                    // self.bitmap.turn_over(index);
                    let new_item = BuddyItem{
                        first_page: PFN(
                            if removed_item.first_page<item.first_page {
                                removed_item.first_page.0
                            } else {
                                item.first_page.0
                            }
                        ),
                        pg_cnt: item.pg_cnt*2
                    };
                    return Some(new_item);
                } else {
                    self.list.push_back(item);
                    self.bitmap.turn_over(index);
                }
            }
            _ => {
                panic!("insert buddy item fail..");
            }
        }
        return None;
    }
    fn is_empty(&self)->bool{
        self.free_cnt == 0
    }
    fn _get_bitmap_index(&self,pfn:PFN)->Result<usize,isize>{
        let pgs = order2pages(self.order);
        if pfn< self.first_pfn {
            return Err(-1);
        }
        let pg = (pfn - self.first_pfn).0;
        let mut index = pg/pgs;
        if pg%pgs!=0{
            Err(-1)
        } else {
            index = index/2;
            Ok(index)
        }
    }
    fn _get_buddy_PFN(&self,pfn :PFN)->PFN{
        let pgs= order2pages(self.order);
        let index = (pfn - self.first_pfn).0 / pgs;
        let interval = PFN(pgs);
        if index%2 == 0 {
            pfn + interval
        } else {
            pfn - interval
        }
    }
}

pub fn buddy_test(){
    bitmap_test();
    let mut b = BuddyAllocator::new(Addr(0x0),Addr(0x1000000));
    let m = b.alloc_area(0);
    b.free_area(m.unwrap(), 0);
    info_sync!("\n{:?}",b);
    // page_test();
    shutdown();
}