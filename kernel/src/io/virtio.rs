use alloc::collections::LinkedList;
use alloc::sync::Arc;
use alloc::vec::Vec;
use fatfs::{info, trace};
use virtio_drivers::{VirtIOBlk, VirtIOHeader};
use crate::consts::PHY_MEM_OFFSET;
use crate::io::{BlockRead, BlockReadWrite, BlockWrite};
use crate::mm::addr::{OldAddr, Paddr, PFN, Vaddr};
use crate::mm::alloc_pages;
use crate::mm::page::Page;
use crate::{info_sync, println, SpinLock, trace_sync};
use crate::pre::InnerAccess;
use crate::utils::{order2pages, pages2order};

#[allow(unused)]
const VIRTIO0: usize = 0x10001000;

pub struct VirtioDev {
    inner:SpinLock<VirtIOBlk<'static>>
}

impl BlockRead for VirtioDev {
    fn read_block(&self, blk_no: usize, buf: &mut [u8]) {
        self.inner.lock_irq().unwrap().read_block(blk_no,buf).expect("read block fail")
    }
}

impl BlockWrite for VirtioDev {
    fn write_block(&self, blk_no: usize, buf: &[u8]) {
        self.inner.lock_irq().unwrap().write_block(blk_no,buf).expect("write block fail")
    }
}

impl BlockReadWrite for VirtioDev{}

impl VirtioDev {
    pub fn new()->Self{
        VirtioDev{
            inner: SpinLock::new(
                VirtIOBlk::new(
                    unsafe { &mut *(VIRTIO0 as *mut VirtIOHeader) }
                ).unwrap()
            )
        }
    }
}

type PhysAddr = usize;
type VirtAddr = usize;

lazy_static!{
    static ref DMA_FMS:SpinLock<LinkedList<Arc<Page>>> = SpinLock::new(LinkedList::new());
}

fn dma_fms_insert(pg:Arc<Page>){
    DMA_FMS.lock().unwrap().push_back(pg);
}

fn dma_fms_remove(vaddr:Vaddr)->Arc<Page>{
    let mut fms = DMA_FMS.lock().unwrap();
    let mut index: usize = 0;
    let mut cursor = fms.cursor_front_mut();
    while cursor.current().is_some() {
        if cursor.current().unwrap().get_vaddr() == vaddr {
            return cursor.remove_current().unwrap();
        }
        cursor.move_next();
    }
    panic!("remove dma fail");
}

#[no_mangle]
pub extern "C" fn virtio_dma_alloc(pages: usize) -> PhysAddr{
    let pg = alloc_pages(pages2order(pages)).unwrap();
    dma_fms_insert(pg.clone());
    let paddr: Paddr = pg.get_vaddr().into();
    let ret = paddr.get_inner();
    trace_sync!("virtio dma alloc paddr {:#X}, pages {:#X}\n",ret,pages);
    return ret;
}

#[no_mangle]
pub extern "C" fn virtio_dma_dealloc(paddr: PhysAddr, pages: usize) -> i32{
    let pg = dma_fms_remove(Paddr(paddr).into());
    trace_sync!("virtio dma dealloc paddr:{:#X} ,pgs:{:#X}",paddr,pages);
    // safe checker
    assert_eq!(pg.get_block_page_cnt(),pages);
    0
}

#[no_mangle]
pub extern "C" fn virtio_phys_to_virt(paddr: PhysAddr) -> VirtAddr {
    let p:Vaddr = Paddr(paddr).into();
    // trace_sync!("virtio phy=>virt: {:#X}=>{:#X}",paddr,p.get_inner());
    p.get_inner()
}

#[no_mangle]
pub extern "C" fn virtio_virt_to_phys(vaddr: VirtAddr) -> PhysAddr {
    let v:Paddr = Vaddr(vaddr).into();
    // trace_sync!("virtio virt=>phy: {:#X}=>{:#X}",vaddr,v.get_inner());
    v.get_inner()
}