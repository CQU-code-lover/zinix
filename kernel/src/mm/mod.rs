use alloc::boxed::Box;
use alloc::vec;
use alloc::vec::Vec;
use core::borrow::Borrow;
use core::intrinsics::add_with_overflow;
use core::ptr::{addr_of, NonNull, null};
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
    pub fn get_pf_cleared(&mut self)->usize{
        let addr = self.get_pf();
        let slice = addr..addr+PAGE_SIZE;
        slice.into_iter().for_each(|a| unsafe{
            (a as *mut u8).write_volatile(0)
        });
        return addr;
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
    PF_ALLOCATOR.lock().unwrap().init(new_ek,emem);
}

pub fn MmUnitTest(){
    let a = PF_ALLOCATOR.lock().unwrap().get_pf();
    info!("a:{:x}",a);
}

pub struct PageTable{
    tables : Vec<usize>
}

impl PageTable {
    // walk pagetable but not alloc new page.
    fn walk(vaddr:usize)->Option<usize>{
        Some(1)
    }
    // walk pagetable and alloc new page when don`t have valid page.
    fn walk_alloc(){

    }
}

impl PageTable {
    fn getRootPage(& self)->usize{
        self.tables[0]
    }
}

impl Default for PageTable {
    fn default() -> Self {
        PageTable{
            // alloc one pages for root page table
            tables:vec![PF_ALLOCATOR.lock().unwrap().get_pf_cleared()]
        }
    }
}



bitflags! {
    pub struct PTEFlags: u8 {
        const V = 1 << 0;
        const R = 1 << 1;
        const W = 1 << 2;
        const X = 1 << 3;
        const U = 1 << 4;
        const G = 1 << 5;
        const A = 1 << 6;
        const D = 1 << 7;
    }
}

struct PTE{
    flags:u8,
    // rsw:u8, // 只能使用两位
    pnn0:u16, // 只能使用三位
    pnn1:u16, // 只能使用三位
    pnn2:u16, // 只能使用三位
}

impl Into<usize> for PTE {
    fn into(self) -> usize {
        let v:usize = 0;
        v|self.flags|(self.pnn0<<10)|(self.pnn1<<19)|(self.pnn2<<27)
    }
}

impl From<usize> for PTE {
    fn from(v: usize) -> Self {
        PTE{
            flags:v as u8,
            pnn0:((v>>10)&0x1FF) as u16,
            pnn1:((v>>19)&0x1FF) as u16,
            pnn2:((v>>28)&0x1FF) as u16,
        }
    }
}

impl Default for PTE {
    fn default() -> Self {
        PTE{
            flags:0,
            pnn0:0,
            pnn1:0,
            pnn2:0
        }
    }
}

impl PTE {
    fn set_flags(&mut self,flag_mask:u8){
        self.flags |=flag_mask;
    }
    fn clear_flags(&mut self,flag_mask:u8){
        let n_mask = flag_mask^0xFF;
        self.flags &=n_mask;
    }
    fn _get_bits(&self, flag_mask:u8)->bool{
        return self.flags &flag_mask!=0;
    }
    fn vaild(&self)->bool{
        return self._get_bits(PTEFlags.V);
    }
    fn set_pnn(&mut self,paddr :usize){
        let np = paddr>>10;
        self.pnn0 = (np&0x1FF) as u16;
        self.pnn1 = ((np>>9)&0x1FF) as u16;
        self.pnn2 = ((np>>18)&0x1FF) as u16;
    }
}
