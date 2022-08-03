use alloc::boxed::Box;
use alloc::sync::Arc;
use alloc::vec;
use alloc::vec::Vec;
use core::borrow::Borrow;
use core::ptr::{addr_of, NonNull, null};

use buddy_system_allocator::LockedHeap;
use log::{error, info};
use riscv::register::fcsr::Flags;

use buddy::BuddyAllocator;
use page::PagesManager;
use pagetable::create_kernel_pagetable;
use pagetable::PageTable;

use crate::{consts, info_sync, println, SpinLock, trace_sync};
use crate::consts::{DIRECT_MAP_START, MAX_ORDER, PAGE_OFFSET, PAGE_SIZE, PHY_MEM_OFFSET, PHY_MEM_START};
use crate::mm::addr::{Addr, PFN};
use crate::mm::page::Page;
use crate::mm::pagetable::{PTE, PTEFlags};
use crate::sbi::shutdown;
use crate::sync::SpinLockGuard;
use crate::utils::{addr_get_ppn0, addr_get_ppn1, addr_get_ppn2, get_usize_by_addr, set_usize_by_addr};

pub(crate) mod addr;
pub(crate) mod page;
pub(crate) mod buddy;
pub(crate) mod bitmap;
pub(crate) mod pagetable;
pub(crate) mod vma;
pub(crate) mod mm;
pub(crate) mod aux;

const k210_mem_mb:u32 = 6;
const qemu_mem_mb:u32 = 6;

const BitmapBits:usize = 4096;
const BitmapOneMax:usize = 1024;
const BitmapCnt:usize = BitmapBits/BitmapOneMax;
const HeapPages:usize = 40;

#[global_allocator]
static HEAP_ALLOCATOR: LockedHeap = LockedHeap::empty();

#[alloc_error_handler]
pub fn alloc_error_handler(layout: core::alloc::Layout)->!{
    panic!("Heap allocation error, layout = {:?}", layout);
}

extern "C" {
    fn ekernel();
    fn skernel();
}

lazy_static!{
    static ref KERNEL_PAGETABLE:Arc<SpinLock<PageTable>> = Arc::new(SpinLock::new(create_kernel_pagetable()));
    static ref BUDDY_ALLOCATOR:SpinLock<BuddyAllocator> = SpinLock::new(Default::default());
    static ref PAGES_MANAGER:SpinLock<PagesManager> = SpinLock::new(Default::default());
}

pub unsafe fn kernel_pagetable_flush(){
    KERNEL_PAGETABLE.lock().unwrap().flush_self();
}

pub fn trace_global_buddy(){
    let b:&BuddyAllocator = &*BUDDY_ALLOCATOR.lock_irq().unwrap();
    trace_sync!("{:?}",b);
}

pub fn get_kernel_pagetable()->Arc<SpinLock<PageTable>>{
    KERNEL_PAGETABLE.clone()
}

fn page_init(start_addr:Addr, end_addr:Addr){
    PAGES_MANAGER.lock().unwrap().init(start_addr,end_addr);
}

fn buddy_init(start_addr:Addr,end_addr:Addr){
    BUDDY_ALLOCATOR.lock().unwrap().init(start_addr,end_addr);
}

const VIRT_ADDR:usize = 0x10001000;

fn hardware_address_map_init(){
    let lock = KERNEL_PAGETABLE.lock().unwrap();
    let flags = PTEFlags::V.bits()| PTEFlags::R.bits()| PTEFlags::W.bits()| PTEFlags::X.bits();
    #[cfg(feature = "qemu")]
    {
        for i in 0x10001..0x10300{
           lock._force_map_one(0+PAGE_SIZE*i, 0+PAGE_SIZE*i,0xcf);
        }
    }
    trace_sync!("map virt ok");
}

pub fn _insert_area_for_page_drop(pfn:PFN,order:usize)->Result<(),isize>{
    BUDDY_ALLOCATOR.lock().unwrap().free_area(pfn,order)
}

pub fn mm_init(){
    let sk = skernel as usize;
    let ek = ekernel as usize;
    let new_ek = ek+PAGE_SIZE*HeapPages;
    unsafe {
        HEAP_ALLOCATOR.lock().init(ek,PAGE_SIZE*HeapPages);
    }
    info_sync!("Heap Allocator Init OK!");
    // init PAGE FRAME ALLOCATOR
    let emem = (qemu_mem_mb as usize)*1024*1024+PHY_MEM_START;
    let mut s_addr = Addr(new_ek);
    let mut e_addr = Addr(emem);
    s_addr = s_addr.ceil();
    e_addr = e_addr.floor();
    buddy_init(s_addr,e_addr);
    page_init(s_addr,e_addr);
    hardware_address_map_init();
}

pub fn alloc_pages(order:usize)->Option<Arc<Page>>{
    if order>=MAX_ORDER {
        return None;
    }
    let area = BUDDY_ALLOCATOR.lock().unwrap().alloc_area(order);
    return match area {
        Ok(pfn) => {
            let pgs = PAGES_MANAGER.lock().unwrap().new_pages_block_in_memory(pfn, order);
            pgs.clear_pages_block();
            Some(pgs)
        }
        _ => {
            None
        }
    }
}

pub fn alloc_one_page()->Option<Arc<Page>>{
    alloc_pages(0)
}

// free a pages block..
// the arg 'page' `s ownership will move to this func and drop.
// do same things with 'Drop(page)'
pub fn free_pages(page:Arc<Page>){
    return;
}
