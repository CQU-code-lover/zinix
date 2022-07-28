use alloc::sync::Arc;
use alloc::vec::Vec;
use fatfs::info;
use virtio_drivers::{VirtIOBlk, VirtIOHeader};
use crate::consts::PHY_MEM_OFFSET;
use crate::io::{BlockRead, BlockReadWrite, BlockWrite};
use crate::mm::addr::{Addr, PFN};
use crate::mm::alloc_pages;
use crate::mm::buddy::{order2pages, pages2order};
use crate::mm::page::Page;
use crate::{info_sync, println, SpinLock};

#[allow(unused)]
const VIRTIO0: usize = 0x10001000+PHY_MEM_OFFSET;

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
    static ref DMA_FMS:SpinLock<Vec<Arc<Page>>> = SpinLock::new(Vec::new());
}

fn dma_fms_insert(pg:Arc<Page>){
    DMA_FMS.lock().unwrap().push(pg);
}

fn dma_fms_remove(pfn:PFN)->Arc<Page>{
    let mut fms = DMA_FMS.lock().unwrap();
    let mut index: usize = 0;
    for i in fms.iter(){
        if i.get_pfn() == pfn{
            break;
        }
        index += 1;
    }
    if index<fms.len(){
        return fms.remove(index);
    }
    panic!("remove dma fail");
}

#[no_mangle]
pub extern "C" fn virtio_dma_alloc(pages: usize) -> PhysAddr{
    let pg = alloc_pages(pages2order(pages)).unwrap();
    println!("{}",pages);
    dma_fms_insert(pg.clone());
    let addr:Addr = pg.get_pfn().into();
    let ret = addr.get_paddr();
    info_sync!("{}\n",ret);
    return ret;
}

#[no_mangle]
pub extern "C" fn virtio_dma_dealloc(paddr: PhysAddr, pages: usize) -> i32{
    let pg = dma_fms_remove(Addr(Addr(paddr).get_vaddr()).into());
    let len = pg.get_inner_guard().get_friend().len() + 1;
    // safe checker
    info_sync!("dma drop pgs");
    assert_eq!(len,pages);
    0
}

#[no_mangle]
pub extern "C" fn virtio_phys_to_virt(paddr: PhysAddr) -> VirtAddr {
    paddr+ PHY_MEM_OFFSET
}

#[no_mangle]
pub extern "C" fn virtio_virt_to_phys(vaddr: VirtAddr) -> PhysAddr {
    vaddr- PHY_MEM_OFFSET
}