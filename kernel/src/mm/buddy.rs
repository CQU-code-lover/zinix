use alloc::collections::LinkedList;
use alloc::string::String;
use alloc::sync::Weak;
use alloc::vec;
use alloc::vec::Vec;
use core::fmt::{Debug, Formatter, write};
use core::ops::Add;

use log::{error, info, log_enabled, set_max_level, warn};
use riscv::interrupt::free;

use crate::{cpu_local, info_sync, println, SpinLock, trace_sync, utils};
use crate::consts::{MAX_ORDER, PAGE_OFFSET, PAGE_SIZE};
use crate::mm::addr::{OldAddr, PageAlign, PFN, Vaddr};
use crate::mm::bitmap::{Bitmap, bitmap_test};
use crate::mm::page::Page;
use crate::pre::InnerAccess;
use crate::sbi::shutdown;

pub struct BuddyItem {
    vaddr: Vaddr,
}

pub struct BuddyAllocator {
    free_areas:Vec<FreeArea>,
    start_vaddr: Vaddr
}

impl Default for BuddyAllocator {
    fn default()->Self {
        return BuddyAllocator{
            free_areas: Vec::new(),
            start_vaddr: Vaddr(0)
        };
    }
}

impl BuddyAllocator{
    pub fn get_free_pages_cnt(&self) -> usize{
        let mut pgs_cnt = 0;
        for i in 0..MAX_ORDER {
            let pg = self.free_areas[i].free_cnt * utils::order2pages(i);
            pgs_cnt += pg;
        }
        return pgs_cnt;
    }
    fn _init_areas(&mut self, start_vaddr:Vaddr, pg_cnt:usize){
        self.start_vaddr = start_vaddr;
        let mut vaddr_probe = start_vaddr;
        let vaddr_end = start_vaddr + pg_cnt*PAGE_SIZE;
        for _ in 0..MAX_ORDER {
            self.free_areas.push(FreeArea::default());
        }
        for i in (0..MAX_ORDER).rev() {
            let free_area = &mut self.free_areas[i];
            free_area.order = i;
            free_area.start_vaddr = start_vaddr;
            let pgs = utils::order2pages(i);
            let mut bitmap_len = pg_cnt/pgs;
            if pg_cnt%pgs != 0{
                bitmap_len += 1;
            }
            free_area.bitmap = Bitmap::new(bitmap_len);
            let mut cnt = 0;
            for j in vaddr_probe.addr_iter((vaddr_end-vaddr_probe.get_inner()).get_inner(),pgs*PAGE_SIZE){
                // 必须检查间隔距离大于pgs*PAGE_SIZE
                if (vaddr_end - j.0).0 < pgs*PAGE_SIZE {
                    break;
                }
                free_area.insert_area(BuddyItem{
                    vaddr: j,
                }).map(|_|{
                    panic!("buddy init fail!");
                });
                cnt+=1;
            }
            vaddr_probe += (pgs*PAGE_SIZE*cnt);
        }
    }
    pub fn init(&mut self, s_vaddr: Vaddr, e_vaddr: Vaddr){
        let ns_addr = s_vaddr.ceil();
        let ne_addr = e_vaddr.floor();
        let pg_cnt = ((ne_addr - ns_addr.get_inner())/PAGE_SIZE).get_inner();
        self._init_areas(ns_addr,pg_cnt);
    }
    pub fn new(s_vaddr: Vaddr, e_vaddr: Vaddr) ->BuddyAllocator{
        let mut b = BuddyAllocator::default();
        b.init(s_vaddr,e_vaddr);
        return b;
    }
    pub fn alloc_area(&mut self, order:usize) ->Result<Vaddr,isize>{
        if order >= MAX_ORDER {
            Err(-1)
        } else {
            match self.free_areas[order].alloc_area() {
                Ok(item) => {
                    // find one
                    Ok(item.vaddr)
                }
                _ => {
                    match self.alloc_area(order + 1) {
                        Ok(vaddr) => {
                            // alloc OK from high order
                            let pgs = utils::order2pages(order);
                            let re_insert_vaddr = vaddr.clone() + PAGE_SIZE*pgs;
                            // trace_sync!("reinsert buddy: (PFN,order) = ({:?}, {})",re_insert_pfn,order);
                            let insert_ret = self.free_areas[order].insert_area(BuddyItem {
                                vaddr: re_insert_vaddr,
                            });
                            assert!(insert_ret.is_none());
                            Ok(vaddr)
                        }
                        _ => {
                            Err(-1)
                        }
                    }
                }
            }
        }
    }
    pub fn free_area(&mut self, vaddr:Vaddr, order:usize) ->Result<(),isize>{
        // check order and align
        if order>=MAX_ORDER {
            return Err(-1);
        }
        if self.free_areas[order].start_vaddr > vaddr {
            return Err(-1);
        }
        if ((vaddr - self.free_areas[order].start_vaddr.0) % (PAGE_SIZE*utils::order2pages(order))).0 !=0 {
            return Err(-1);
        }
        let mut  buddy_item = BuddyItem{
            vaddr,
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
}

impl Debug for BuddyAllocator {
    fn fmt(&self, f: &mut Formatter<'_>) -> core::fmt::Result {
        writeln!(f,"[BuddyAllocator Debug]");
        writeln!(f,"Start Vaddr: {:#X}",self.start_vaddr.0);
        let mut pgs_cnt = 0;
        for i in 0..MAX_ORDER {
            let pg = self.free_areas[i].free_cnt * utils::order2pages(i);
            pgs_cnt += pg;
        }
        writeln!(f,"Freed Pages: {}", pgs_cnt);
        for i in 0..MAX_ORDER {
            write!(f,"Order{}: ",i);
            for j in &self.free_areas[i].list {
                write!(f,"{:X}\t",j.vaddr.0);
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
    start_vaddr: Vaddr,
    free_cnt: usize,
    order: usize
}

impl FreeArea {
    fn default() ->FreeArea{
        FreeArea{
            list: LinkedList::new(),
            bitmap: Bitmap::default(),
            start_vaddr: Vaddr(0),
            free_cnt: 0,
            order: 0
        }
    }
    fn len(&self)->usize{
        self.free_cnt
    }
    fn alloc_area(&mut self)->Result<BuddyItem,isize>{
        if self.free_cnt == 0 {
            // no free area..
            Err(-1)
        } else {
            self.free_cnt -= 1;
            let ret = self.list.pop_back().unwrap();
            match  self._get_bitmap_index(ret.vaddr) {
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
        match self._get_bitmap_index(item.vaddr) {
            Ok(index) => {
                // check bitmap
                let is_set = self.bitmap.get(index);
                let is_max = (self.order == (MAX_ORDER-1));
                if is_set && (!is_max){
                    // combime..
                    let buddy_vaddr = self._get_buddy_vaddr(item.vaddr);
                    let mut removed_index = self.list.len();
                    let mut removed_acc:usize = 0;
                    for area in self.list.iter() {
                        if buddy_vaddr == area.vaddr {
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
                    let new_item = BuddyItem {
                        vaddr:
                            if removed_item.vaddr < item.vaddr {
                                removed_item.vaddr
                            } else {
                                item.vaddr
                            }
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
    fn _get_bitmap_index(&self, vaddr:Vaddr) ->Result<usize,isize>{
        let pgs = utils::order2pages(self.order);
        if vaddr < self.start_vaddr {
            return Err(-1);
        }
        let pg = (vaddr - self.start_vaddr.0).0/PAGE_SIZE;
        let mut index = pg/pgs;
        if pg%pgs!=0{
            Err(-1)
        } else {
            index = index/2;
            Ok(index)
        }
    }
    fn _get_buddy_vaddr(&self, vaddr:Vaddr) ->Vaddr{
        let pgs= utils::order2pages(self.order);
        let addr_interval = pgs*PAGE_SIZE;
        let index = (vaddr - self.start_vaddr.0).0 / (pgs*PAGE_SIZE);
        if index%2 == 0 {
            vaddr + addr_interval
        } else {
            vaddr - addr_interval
        }
    }
}

pub fn buddy_test(){

}