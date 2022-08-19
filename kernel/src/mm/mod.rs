use alloc::boxed::Box;
use alloc::sync::Arc;
use alloc::vec;
use alloc::vec::Vec;
use core::borrow::Borrow;
use core::ptr::{addr_of, NonNull, null};

use buddy_system_allocator::LockedHeap;
use log::{error, info};
use riscv::asm::sfence_vma_all;
use riscv::register::fcsr::Flags;

use buddy::BuddyAllocator;
use page::PagesManager;
use pagetable::create_kernel_pagetable;
use pagetable::PageTable;

use crate::{consts, info_sync, println, SpinLock, trace_sync};
use crate::consts::{DEV_REMAP_START, DIRECT_MAP_START, MAX_ORDER, PAGE_OFFSET, PAGE_SIZE, PHY_MEM_OFFSET, PHY_MEM_START};
use crate::mm::addr::{addr_test, OldAddr, PageAlign, PFN, Vaddr};
use crate::mm::bitmap::bitmap_test;
use crate::mm::mm::MmStruct;
use crate::mm::page::Page;
use crate::mm::pagetable::{create_kernel_mm, PTE, PTEFlags};
use crate::pre::{InnerAccess, ReadWriteSingleNoOff};
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
pub(crate) mod kmap;

const k210_mem_mb:u32 = 6;
const qemu_mem_mb:u32 = 128;

const BitmapBits:usize = 4096;
const BitmapOneMax:usize = 1024;
const BitmapCnt:usize = BitmapBits/BitmapOneMax;
#[cfg(feature = "k210")]
const HeapPages:usize = 40;
#[cfg(feature = "qemu")]
const HeapPages:usize = 1024;

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
    // static ref KERNEL_PAGETABLE:Arc<SpinLock<PageTable>> = Arc::new(SpinLock::new(create_kernel_pagetable()));
    static ref BUDDY_ALLOCATOR:SpinLock<BuddyAllocator> = SpinLock::new(Default::default());
    static ref PAGES_MANAGER:SpinLock<PagesManager> = SpinLock::new(Default::default());
    static ref KERNEL_MM:Arc<SpinLock<MmStruct>> = Arc::new(SpinLock::new(create_kernel_mm()));
}

pub fn trace_global_buddy(){
    let b:&BuddyAllocator = &*BUDDY_ALLOCATOR.lock_irq().unwrap();
    trace_sync!("{:?}",b);
}

pub fn get_kernel_pagetable()->Arc<PageTable>{
    KERNEL_MM.lock_irq().unwrap().pagetable.clone()
}

pub fn get_kernel_mm()->SpinLockGuard<'static,MmStruct> {
    KERNEL_MM.lock_irq().unwrap()
}

fn page_init(start_addr: Vaddr, end_addr: Vaddr){
    PAGES_MANAGER.lock().unwrap().init(start_addr,end_addr);
}

fn buddy_init(start_addr: Vaddr, end_addr: Vaddr){
    BUDDY_ALLOCATOR.lock().unwrap().init(start_addr,end_addr);
}

fn k210_remap(pgt:Arc<PageTable>){
    pgt._force_map_one(0x38000000,0x38000000,0xcf);
    pgt._force_map_one(0x38001000,0x38001000,0xcf);
}

fn hardware_remapping(){
    let pgt = get_kernel_pagetable();
    let flags = PTEFlags::V.bits()| PTEFlags::R.bits()| PTEFlags::W.bits()| PTEFlags::X.bits();
    #[cfg(feature = "qemu")]
    {
        // 清空0x40000000映射部分 这部分是k210专用
        let v:usize =unsafe{(pgt._get_root_page_vaddr()+8).read_single().unwrap()};
        unsafe{(pgt._get_root_page_vaddr()+8).write_single(0).unwrap();}
        for i in 0x10001..0x10300{
           pgt._force_map_one(0+PAGE_SIZE*i+DEV_REMAP_START, 0+PAGE_SIZE*i, 0xcf);
        }
    }
    #[cfg(feature = "k210")]
    {
        k210_remap(pgt);
    }
    unsafe { sfence_vma_all(); }
    info_sync!("remap hardware ok");
}

pub fn _insert_area_for_page_drop(vaddr:Vaddr, order:usize) ->Result<(),isize>{
    BUDDY_ALLOCATOR.lock().unwrap().free_area(vaddr, order)
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
    #[cfg(feature = "qemu")]
    let mbs = qemu_mem_mb;
    #[cfg(feature = "k210")]
    let mbs = k210_mem_mb;

    let emem = (mbs as usize)*1024*1024+PHY_MEM_START;
    let mut s_addr = Vaddr(new_ek);
    let mut e_addr = Vaddr(emem);
    s_addr = s_addr.ceil();
    e_addr = e_addr.floor();
    buddy_init(s_addr,e_addr);
    page_init(s_addr,e_addr);
    hardware_remapping();
}

pub fn alloc_pages(order:usize)->Option<Arc<Page>>{
    if order>=MAX_ORDER {
        return None;
    }
    let area = BUDDY_ALLOCATOR.lock().unwrap().alloc_area(order);
    return match area {
        Ok(vaddr) => {
            let pgs = PAGES_MANAGER.lock().unwrap().new_pages_block_in_memory(vaddr, order);
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

pub fn mm_test(){
    bitmap_test();
    addr_test();
    shutdown();
}
