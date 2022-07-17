use alloc::collections::LinkedList;
use alloc::sync::Weak;
use alloc::vec;
use alloc::vec::Vec;
use core::fmt::{Debug, Formatter, write};
use core::ops::Add;
use core::slice::range;
use bitmaps::Bitmap;
use log::{error, log_enabled, set_max_level};
use riscv::interrupt::free;
use crate::consts::MAX_ORDER;
use crate::{cpu_local, println};
use crate::mm::addr::{Addr, PFN};
use crate::mm::page::Page;
use crate::sbi::shutdown;

pub struct BuddyItem {
    first_page: PFN,
    pg_cnt : usize
}

struct BuddyBitMap {
    inner : Vec<u64>
}

impl BuddyBitMap {
    fn default()->BuddyBitMap {
        return BuddyBitMap {
            inner: vec![]
        }
    }
    fn init(&mut self,len:usize){
        let mut cnt = len/64;
        if len%64 != 0{
            cnt +=1;
        }
        self.inner.resize_with(cnt ,|| {
            0
        });
    }
    fn new(len:usize)->BuddyBitMap{
        let mut s = Self::default();
        s.init(len);
        return s;
    }
    fn _expand_to(&mut self,new_len:usize){
        self.inner.resize_with(new_len, || {
            0
        })
    }
    fn _expand_double(&mut self)->usize {
        let new_len = self.inner.len() *2;
        self._expand_to(new_len);
        return self.inner.len();
    }
    fn expand_for(&mut self,target_pos:usize){
        let mut len_target = target_pos/64;
        if target_pos%64 != 0 {
            len_target += 1;
        }
        while len_target>=self.inner.len() {
            self._expand_double();
        }
    }
    fn set(&mut self, pos:usize){
        let pos1 = pos/64;
        let pos2 = pos%64;
        if pos1>=self.inner.len(){
            self.expand_for(pos);
        }
        let mut bits = self.inner[pos1];
        let mask = (1<<pos2) as u64;
        if bits&mask == 0 {
            bits+=mask;
            self.inner[pos1] = bits;
        }
    }
    fn clear(&mut self,pos :usize){
        let pos1 = pos/64;
        let pos2 = pos%64;
        if pos1 >= self.inner.len(){
            self.expand_for(pos);
            return;
        }
        let mut bits = self.inner[pos1];
        let mask = !((1<<pos2) as u64);
        bits = bits & mask;
        self.inner[pos1] = bits;
    }
    fn get(&mut self, pos:usize)->bool{
        let pos1 = pos/64;
        let pos2 = pos%64;
        if pos1<self.inner.len(){
            let bits = self.inner[pos1];
            let mask = (1<<pos2) as u64;
            if bits&mask != 0 {
                true
            } else {
                false
            }
        } else {
            self.expand_for(pos);
            false
        }
    }
    fn turn_over(&mut self, pos:usize){
        if self.get(pos){
            self.clear(pos);
        } else {
            self.set(pos);
        }
    }
}

impl Debug for BuddyBitMap {
    fn fmt(&self, f: &mut Formatter<'_>) -> core::fmt::Result {
        for i in 0..self.inner.len() {
            write!(f,"{:X} : {:b}\n",i*64,self.inner[i]);
        }
        core::fmt::Result::Ok(())
    }
}

pub struct BuddyAllocator {
    free_areas:Vec<FreeArea>,
    first_pfn : PFN
}

fn order2pages(order:usize)->usize{
    return if order < MAX_ORDER {
        1 << order
    } else {
        0
    }
}

impl BuddyAllocator{
    fn default()->BuddyAllocator {
        return BuddyAllocator{
            free_areas: Vec::new(),
            first_pfn : PFN::from(0)
        };
    }

    fn init_areas(&mut self,start_pfn:PFN,pg_cnt:usize){
        self.first_pfn = start_pfn;
        let mut pfn_probe = start_pfn;
        let pfn_end = start_pfn.clone().step_n(pg_cnt);
        for i in 0..MAX_ORDER {
            self.free_areas.push(FreeArea::default());
            let free_area = & mut self.free_areas[i];
            free_area.order = i;
            free_area.first_pfn = start_pfn;
            let pgs = order2pages(i);
            let mut area_cnt:usize = 0;
            while pfn_probe.clone().step_n(pgs) < pfn_end {
                free_area.free_cnt += 1;
                free_area.list.push_back(BuddyItem{
                    first_page: pfn_probe,
                    pg_cnt:pgs
                });
                area_cnt += 1;
                pfn_probe.step_n(pgs);
            }
            // set zone_map
            let bitmap_len =area_cnt/2;
            if area_cnt%2 != 0{
                bitmap_len+=1;
            }
            free_area.bitmap = BuddyBitMap::new(bitmap_len);
        }
    }

    fn init(&mut self,s_addr:Addr,e_addr:Addr){
        let ns_addr = s_addr.floor();
        let ne_addr = e_addr.ceil();
        let pg_cnt = (ne_addr - ns_addr).get_pg_cnt();
    }
}

pub struct FreeArea {
    list : LinkedList<BuddyItem>,
    bitmap: BuddyBitMap,
    first_pfn : PFN,
    free_cnt: usize,
    order: usize
}

impl FreeArea {
    fn default() ->FreeArea{
        FreeArea{
            list: LinkedList::new(),
            bitmap: BuddyBitMap::default(),
            first_pfn: PFN(0),
            free_cnt: 0,
            order: 0
        }
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
    fn insert_area(&mut self,item:BuddyItem)->Option<BuddyItem>{
        self.free_cnt += 1;
        match self._get_bitmap_index(item.first_page) {
            Ok(index) => {
                // check bitmap
                let is_set = self.bitmap.get(index);
                if is_set {
                    // combime..
                    for area in self.list.iter() {

                    }
                    self.free_cnt -= 2;
                    self.bitmap.clear(index);
                } else {
                    self.list.push_back(item);
                    self.bitmap.set(index);
                }
            }
            _ => {
                panic!("insert buddy item fail..");
            }
        }
    }
    fn _get_bitmap_index(&self,pfn:PFN)->Result<usize,isize>{
        let pgs = order2pages(self.order);
        if pfn< self.first_pfn {
            return Err(-1);
        }
        let pg = (pfn - self.first_pfn).0;
        let index = pg/pgs;
        if pg%pgs!=0{
            Err(-1)
        } else {
            index = index/2;
            Ok(index)
        }
    }
    fn _get_buddy_PFN(&self,pfn :PFN){

    }
}

pub fn buddy_test(){
    let mut b = BuddyBitMap::new(256);
    let j =b.get(1);
    b.set(1);
    let k = b.get(1);
    b.clear(1);
    let m = b.get(1);
    b.turn_over(1);
    b.turn_over(2);
    b.turn_over(1);
    assert_eq!(j,false);
    assert_eq!(k,true);
    assert_eq!(m,false);
    println!("buddy test OK!");
    println!("{:?}",b);
    shutdown();
}