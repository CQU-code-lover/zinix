use alloc::boxed::Box;
use alloc::vec;
use alloc::vec::Vec;
use core::borrow::Borrow;
use core::ptr::{NonNull, null};
use bitmaps::Bitmap;
use log::{error, info};
use crate::{consts, SpinLock};
use crate::consts::{DIRECT_MAP_START, PAGE_OFFSET, PAGE_SIZE};
use buddy_system_allocator::LockedHeap;
use crate::sync::SpinLockGuard;

const k210_mem_mb:u32 = 6;
const qemu_mem_mb:u32 = 6;
const BitmapBits:usize = 4096;
const BitmapOneMax:usize = 1024;
const BitmapCnt:usize = BitmapBits/BitmapOneMax;
const HeapPages:usize = 4;

#[global_allocator]
static HEAP_ALLOCATOR: LockedHeap = LockedHeap::empty();

lazy_static!{
    static ref PF_ALLOCATOR: SpinLock<PF_Allocator> = SpinLock::new(PF_Allocator::default());
}

#[alloc_error_handler]
pub fn alloc_error_handler(layout: core::alloc::Layout)->!{
    panic!("Heap allocation error, layout = {:?}", layout);
}

extern "C" {
    fn ekernel();
    fn skernel();
}

pub struct PF_Allocator {
    mem_start : usize,
    mem_end : usize,
    bitmaps :Vec<Bitmap<BitmapOneMax>>,
}

impl PF_Allocator {
    fn default()->Self{
        PF_Allocator{
            mem_start:0,
            mem_end:0,
            bitmaps:vec![],
        }
    }
    fn init(&mut self,start:usize,end:usize){
        self.mem_start = start-(start&PAGE_SIZE)+PAGE_SIZE;
        self.mem_end = end-(end&PAGE_SIZE)+PAGE_SIZE;
        if self.mem_end<=self.mem_start{
            error!("PF allocator init fail!");
        }
        let len = self.mem_end-self.mem_start;
        let pg_cnt = len/PAGE_SIZE;
        let mut bm_cnt = pg_cnt/BitmapOneMax;
        if bm_cnt%BitmapOneMax != 0{
            bm_cnt+=1;
        }
        for i in 0..bm_cnt{
            self.bitmaps.push(Bitmap::new());
        }
        let ss = pg_cnt-BitmapOneMax*(bm_cnt-1);
        for j in ss..BitmapOneMax{
            let bm_index = self.bitmaps.len()-1;
            self.bitmaps[bm_index].set(j,true);
        }
        info!("PF Allocator Init OK!");
    }
    pub fn get_pf(&mut self) ->usize{
        for bitmapIndex in 0..self.bitmaps.len() {
            let k = self.bitmaps[bitmapIndex].first_false_index();
            match k {
                None=>break,
                Some(index)=>{
                    self.bitmaps[bitmapIndex].set(index, true);
                    return bitmapIndex*BitmapOneMax+index*PAGE_SIZE+self.mem_start;
                }
            }
        };
        // not get one
        error!("Can`t get PAGE FRAME!");
        return  0;
    }
    pub fn put_pf(&mut self,addr:usize){
        // check
        if addr<self.mem_start||addr>(self.mem_end-PAGE_SIZE){
            error!("Can`t put PAGE FRAME!");
        }
        let pfn = (addr-self.mem_start)/PAGE_SIZE;
        let a = pfn/BitmapOneMax;
        let b = pfn%BitmapOneMax;
        //check
        if self.bitmaps[a].get(b)==false{
            error!("Can`t put PAGE FRAME!");
        }
        self.bitmaps[a].set(b,false);
    }
}

pub fn mm_init(){
    let sk = skernel as usize;
    let ek = ekernel as usize;
    let new_ek = ek+PAGE_SIZE*HeapPages;
    unsafe {
        HEAP_ALLOCATOR.lock().init(ek,PAGE_SIZE*HeapPages);
    }
    info!("Heap Allocator Init OK!");
    // init PAGE FRAME ALLOCATOR
    let emem = (qemu_mem_mb as usize)*1024*1024+sk;
    info!("{:x},{:x},{:x}",sk,new_ek,emem);
    PF_ALLOCATOR.lock().unwrap().init(new_ek,emem);
}


pub fn UnitTest(){
    let a = PF_ALLOCATOR.lock().unwrap().get_pf();
    info!("a:{:x}",a);
}