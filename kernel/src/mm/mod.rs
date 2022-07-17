mod addr;
mod page;
pub(crate) mod buddy;

use alloc::boxed::Box;
use alloc::sync::Arc;
use alloc::vec;
use alloc::vec::Vec;
use core::borrow::Borrow;
use core::ptr::{addr_of, NonNull, null};
use bitmaps::Bitmap;
use log::{error, info};
use crate::{consts, SpinLock};
use crate::consts::{DIRECT_MAP_START, PAGE_OFFSET, PAGE_SIZE};
use buddy_system_allocator::LockedHeap;
use riscv::register::fcsr::Flags;
use crate::sync::SpinLockGuard;
use crate::utils::{addr_get_ppn2, addr_get_ppn1, addr_get_ppn0, get_usize_by_addr, set_usize_by_addr};

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
    fn boot_pagetable();
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

lazy_static!{
    static ref KERNEL_PAGETABLE:Arc<SpinLock<PageTable>> = Arc::new(SpinLock::new(create_kernel_pagetable()));
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

#[derive(Clone)]
pub struct PageTable{
    tables : Vec<usize>
}

const WalkRetLevelRoot:usize = 0;
const WalkRetLevelMiddle:usize = 1;
const WalkRetLevelLeaf:usize = 2;

pub struct WalkRet{
    level:usize,
    pte_addr:usize,
}


impl PageTable {
    // alloc时默认不使用大页
    fn _walk_common(&mut self, vaddr:usize, alloc:bool)->Option<WalkRet>{
        let mut pg_addr = self._get_root_page();
        let ppn_arr = [addr_get_ppn2(vaddr),addr_get_ppn1(vaddr),addr_get_ppn0(vaddr)];
        for i in 0..3{
            let index = ppn_arr[i];
            let entry_addr = pg_addr+index*8;
            let entry =unsafe {get_usize_by_addr(entry_addr)};
            let mut pte = PTE::from(entry);
            // 如果是leaf，不管pte是否valid都能返回
            if pte.is_leaf(){
                return Some(WalkRet{
                    level: i,
                    pte_addr: entry_addr
                });
            }

            if pte.vaild() {
                pg_addr = pte.get_point_paddr();
                continue;
            }
            else {
                // not get next
                if alloc {
                    pte.set_ppn(PF_ALLOCATOR.lock().unwrap().get_pf_cleared());
                    //set RWX
                    //此时一定是非叶子页表
                    pte.clear_flags(PTEFlags::R.bits|PTEFlags::W.bits|PTEFlags::X.bits);
                    let new_pte_val = pte.into();
                    // write back pte value
                    unsafe {set_usize_by_addr(entry_addr,new_pte_val)};
                }
                else {
                    return None;
                }
            }
        }
        // bug：三级页表项出现RWX全0情况
        error!("Walk Fault!");
        return None;
    }

    // walk pagetable but not alloc new page.
    // return leaves pagetable`s PTE addr...
    // so we can mapping it...
    pub fn walk(&mut self, vaddr:usize)->Option<WalkRet>{
        return self._walk_common(vaddr,false);
    }
    // walk pagetable and alloc new page when don`t have valid page.
    pub fn walk_alloc(&mut self, vaddr:usize)->WalkRet{
        // alloc是不会返回None，否则会panic
        let r= self._walk_common(vaddr,true);
        match r {
            None=>{
                error!("bug");
                WalkRet{
                    level: 0,
                    pte_addr: 0
                }
            },
            Some(ret)=> ret
        }
    }
    pub fn use_addr(&mut self,vaddr:usize,flags:u8){
        let r = self.walk_alloc(vaddr);
        match r.level {
            WalkRetLevelLeaf => {
                let pte_val =unsafe {get_usize_by_addr(vaddr)};
                let mut pte = PTE::from(pte_val);
                let pf = PF_ALLOCATOR.lock().unwrap().get_pf_cleared();
                self.tables.push(pf);
                pte.set_ppn(pf);
                pte.set_flags(flags);
                let new_pte_val = pte.into();
                unsafe {set_usize_by_addr(vaddr,new_pte_val)};
            },
            _=>{

            }
        }
    }
}

impl PageTable {
    fn _get_root_page(& self) ->usize{
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
        v|self.flags as usize|((self.pnn0<<10) as usize)|((self.pnn1<<19) as usize)|((self.pnn2<<28) as usize)
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
        return self._get_bits(PTEFlags::V.bits);
    }
    fn is_leaf(&self)->bool{
        return !((self._get_bits(PTEFlags::R.bits)==false)&
            (self._get_bits(PTEFlags::W.bits)==false)&
            (self._get_bits(PTEFlags::X.bits)==false));
    }
    fn is_not_leaf(&self)->bool{
        return !self.is_leaf();
    }
    fn set_ppn(&mut self,paddr :usize){
        let np = paddr>>10;
        self.pnn0 = (np&0x1FF) as u16;
        self.pnn1 = ((np>>9)&0x1FF) as u16;
        self.pnn2 = ((np>>18)&0x1FF) as u16;
    }
    fn get_point_paddr(& self)->usize{
        return ((self.pnn0 as usize)|((self.pnn1<<9) as usize)|((self.pnn2<<18) as usize))<<12;
    }
}

fn create_kernel_pagetable()->PageTable{
    let kernel_pagetable_root = boot_pagetable as usize;
    let kp = PageTable{
        tables: vec![kernel_pagetable_root]
    };
    return kp;
}