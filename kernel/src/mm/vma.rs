use alloc::collections::{BTreeMap, LinkedList};
use alloc::sync::Arc;
use alloc::vec;
use alloc::vec::Vec;
use core::arch::riscv64::fence_i;
use core::cell::RefCell;
use core::cmp::Ordering;
use core::default::Default;
use core::fmt::{Debug, Formatter};
use core::mem::size_of;
use core::ptr::{slice_from_raw_parts, slice_from_raw_parts_mut};
use fatfs::{Read, Seek, SeekFrom, Write};
use log::set_max_level;
use crate::consts::PAGE_SIZE;

use crate::mm::addr::{OldAddr, Paddr, PageAlign, Vaddr};
use crate::mm::{alloc_one_page, get_kernel_pagetable};
use crate::utils::order2pages;
use crate::mm::mm::MmStruct;
use crate::mm::page::Page;
use crate::mm::pagetable::{PageTable, PTEFlags};
use crate::pre::{InnerAccess, ReadWriteOffUnsafe, ReadWriteSingleNoOff, ReadWriteSingleOff, ShowRdWrEx};
use crate::{error_sync, info_sync, println, SpinLock};
use crate::fs::inode::Inode;
use crate::sbi::shutdown;

bitflags! {
    pub struct VmFlags: usize {
        const VM_NONE = 0;
        const VM_READ = 1 << 0;
        const VM_WRITE = 1 << 1;
        const VM_EXEC = 1 << 2;
        const VM_USER = 1 << 3;
        const VM_SHARD = 1 << 4;
        const VM_ANON = 1 << 5;
        const VM_DIRTY = 1<< 6;
    }
}

impl VmFlags {
    pub fn from_mmap(mmap_flags:MmapFlags,prot_flags:MmapProt)->Self{
        let mut ret = Self::VM_NONE;
        if prot_flags.contains(MmapProt::PROT_READ){
            ret|=Self::VM_READ;
        }
        if prot_flags.contains(MmapProt::PROT_WRITE){
            ret|=Self::VM_WRITE;
        }
        if prot_flags.contains(MmapProt::PROT_EXEC){
            ret|=Self::VM_EXEC;
        }
        if mmap_flags.contains(MmapFlags::MAP_ANONYMOUS){
            ret|=Self::VM_ANON;
        }
        ret|=Self::VM_USER;
        ret
    }
}

bitflags! {
    pub struct MmapFlags: usize {
        const MAP_FILE = 0;
        const MAP_SHARED= 0x01;
        const MAP_PRIVATE = 0x02;
        const MAP_FIXED = 0x10;
        const MAP_ANONYMOUS = 0x20;
    }
}

bitflags! {
    pub struct MmapProt: usize {
        const PROT_READ = 0x1;
        const PROT_WRITE = 0x2;
        const PROT_EXEC = 0x4;
        const PROT_SEM = 0x8;
        const PROT_NONE = 0x0;
        const PROT_GROWSDOWN = 0x01000000;
        const PROT_GROWSUP = 0x02000000;
    }
}

// pub enum VmaType{
//     VmaNone,
//     VmaAnon,
//     VmaFile
// }

pub struct VMA{
    pub start_vaddr: Vaddr,
    pub end_vaddr: Vaddr,
    pub vm_flags:VmFlags,
    pub pages_tree:BTreeMap<Vaddr,Arc<Page>>,
    pub pagetable:Option<Arc<PageTable>>,
    pub file:Option<Arc<Inode>>,
    pub file_off:usize,
    pub file_in_vma_off:usize,
    pub file_len:usize,
    pub phy_pgs_cnt:usize,
    pub cow_write_reserve_pgs:Option<BTreeMap<Vaddr,Arc<Page>>>
}

impl ShowRdWrEx for VMA{
    fn readable(&self) -> bool {
        self.vm_flags.contains(VmFlags::VM_READ)
    }

    fn writeable(&self) -> bool {
        self.vm_flags.contains(VmFlags::VM_WRITE)
    }

    fn execable(&self) -> bool {
        self.vm_flags.contains(VmFlags::VM_EXEC)
    }
}

impl Eq for VMA {}

impl PartialEq<Self> for VMA {
    fn eq(&self, other: &Self) -> bool {
        self.start_vaddr == other.start_vaddr
    }
}

impl PartialOrd<Self> for VMA {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(if self.start_vaddr<other.start_vaddr{
            Ordering::Less
        } else if self.start_vaddr==other.start_vaddr{
            Ordering::Equal
        } else {
            Ordering::Greater
        })
    }
}

impl Ord for VMA {
    fn cmp(&self, other: &Self) -> Ordering {
        self.partial_cmp(other).unwrap()
    }
}

impl Debug for VMA {
    fn fmt(&self, f: &mut Formatter<'_>) -> core::fmt::Result {
        writeln!(f,"range:{:#X}=>{:#X}",self.start_vaddr.0,self.end_vaddr.0);
        writeln!(f,"flags:{:b}",self.vm_flags);
        Ok(())
    }
}

pub fn _vma_flags_2_pte_flags(f:VmFlags)->u8{
    (((f.bits&0b1111)<<1) as u8)|PTEFlags::V.bits()
}

impl VMA {
    pub fn empty(start_vaddr:Vaddr, end_vaddr:Vaddr) ->Self{
        Self{
            start_vaddr,
            end_vaddr,
            vm_flags: VmFlags::VM_NONE,
            pages_tree: Default::default(),
            pagetable: None,
            file: None,
            file_off: 0,
            file_in_vma_off: 0,
            file_len: 0,
            phy_pgs_cnt: 0,
            cow_write_reserve_pgs: None
        }
    }
    pub fn new(start_vaddr: Vaddr, end_vaddr: Vaddr,
               vm_flags:VmFlags,pagetable:Arc<PageTable>, file:Option<Arc<Inode>>,
               file_off:usize,file_in_vma_off:usize,file_len:usize) ->Self{
        if !vm_flags.contains(VmFlags::VM_ANON){
            // file
            assert!(file.is_some());
        }
        VMA {
            start_vaddr,
            end_vaddr,
            vm_flags,
            pages_tree: Default::default(),
            pagetable:Some(pagetable),
            file,
            file_off: 0,
            file_in_vma_off,
            file_len,
            phy_pgs_cnt: 0,
            cow_write_reserve_pgs: None
        }
    }
    pub fn new_anon(start_vaddr: Vaddr, end_vaddr: Vaddr,vm_flags:VmFlags,
                    pagetable:Arc<PageTable>)->Self{
        Self::new(start_vaddr, end_vaddr, vm_flags, pagetable, None, 0, 0, 0)
    }
    pub fn new_file(start_vaddr: Vaddr, end_vaddr: Vaddr,vm_flags:VmFlags,pagetable:Arc<PageTable>,
                    file:Arc<Inode>,file_off:usize,file_in_vma_off:usize,file_len:usize)->Self{
        Self::new(start_vaddr,end_vaddr,vm_flags,pagetable,Some(file),0,file_in_vma_off,file_len)
    }
    pub fn is_anon(&self)->bool{
        self.vm_flags.contains(VmFlags::VM_ANON)
    }
    pub fn is_file(&self)->bool{
        !self.is_anon()
    }
    fn __is_dirty(&self)->bool{
        self.vm_flags.contains(VmFlags::VM_DIRTY)
    }
    fn __clear_dirty(&mut self){
        self.vm_flags.set(VmFlags::VM_DIRTY,false);
    }
    fn __set_dirty(&mut self){
        self.vm_flags.set(VmFlags::VM_DIRTY,true);
    }
    pub fn get_start_vaddr(&self) -> Vaddr {
        self.start_vaddr
    }
    pub fn get_end_vaddr(&self) -> Vaddr {
        self.end_vaddr
    }
    pub fn __set_end_vaddr(&mut self,new:Vaddr) -> Vaddr {
        let ret = self.end_vaddr;
        self.end_vaddr = new;
        ret
    }
    pub fn in_vma(&self, vaddr: Vaddr) ->bool{
        vaddr >=self.start_vaddr && vaddr <self.end_vaddr
    }
    pub fn get_pagetable(&self)->Arc<PageTable>{
        self.pagetable.as_ref().unwrap().clone()
    }
    pub fn get_flags(&self)->VmFlags{
        self.vm_flags
    }
    pub fn _vaddr_have_map(&self, vaddr:Vaddr) ->bool{
        self.pages_tree.contains_key(&vaddr)
    }
    // 为什么要返回错误值？ 可能出现映射区域超出vma范围情况
    // 输入参数要求，vaddr align && vaddr in vma
    // 可能的错误： -1 映射物理页超出vma范围
    //           -1 虚拟页存在映射
    // todo 支持force map
    pub fn _anon_map_one_page(&mut self, page:Arc<Page>, vaddr: Vaddr) ->Result<(),()>{
        debug_assert!(self.is_anon());
        debug_assert!(vaddr.is_align());
        debug_assert!(self.in_vma(vaddr));
        if page.get_order()!=0{
            return Err(());
        }
        let pgs_cnt = order2pages(page.get_order());
        if (self.get_end_vaddr()-vaddr.0).0 < pgs_cnt*PAGE_SIZE {
            return Err(());
        }
        // do map in pagetable
        // vma flags to pte flags
        let pte_flags = _vma_flags_2_pte_flags(self.get_flags());
        if self._vaddr_have_map(vaddr) {
            return Err(());
        }
        match self.pagetable.as_mut().unwrap().map_pages(vaddr, page.get_vaddr().into(), page.get_order(), pte_flags){
            Ok(_) => {}
            Err(_) => {
                return Err(());
            }
        }
        self.phy_pgs_cnt += pgs_cnt;
        if self.pages_tree.insert(vaddr, page).is_some(){
            return Err(());
        }
        Ok(())
    }
    // 只能按照page block的方式unmap
    pub fn _anon_unmap_pages(&mut self, vaddr:Vaddr) ->Option<Arc<Page>>{
        // find pgs from link list
        debug_assert!(self.is_anon());
        debug_assert!(vaddr.is_align());
        debug_assert!(self.in_vma(vaddr));
        match self.pages_tree.remove(&vaddr){
            None => {
                None
            }
            Some(pg) => {
                let order = pg.get_order();
                debug_assert!(self.pagetable.as_mut().unwrap().unmap_pages(vaddr,order).is_ok());
                Some(pg)
            }
        }
    }
    pub fn get_file_inode(&self)->Option<Arc<Inode>> {
        self.file.as_ref().map(
            |x|{
                x.clone()
            }
        )
    }
    pub fn split(&mut self,vaddr:Vaddr)->Option<Self>{
        if self.in_vma(vaddr) {
            return None;
        }
        self.end_vaddr = vaddr;
        if self.is_anon(){
            let new_anon = self.pages_tree.split_off(&vaddr);
            let mut new= Self::new_anon(
                vaddr,
                self.end_vaddr,
                self.vm_flags,
                self.get_pagetable(),
            );
            new.pages_tree = new_anon;
            Some(new)
        } else {
            // let new = Self::new_file(
            //     vaddr,
            //     self.end_vaddr,
            //     self.vm_flags,
            //     self.get_pagetable(),
            //     self.get_file_inode().unwrap(),
            //     self.file_off+((vaddr-self.start_vaddr.0).0)
            // );
            // Some(new)
            todo!()
        }
    }
    pub fn _find_page(&self, vaddr:Vaddr) ->Option<Arc<Page>> {
        self.pages_tree.get(&vaddr).map(|x| {
            x.clone()
        })
    }
    // 相对map pages来说在存在映射时可以不分配物理页并且跳过，这样速度更快
    fn __fast_alloc_one_page(&mut self, vaddr:Vaddr){
        debug_assert!(self.in_vma(vaddr));
        debug_assert!(vaddr.is_align());
        if !self.pages_tree.contains_key(&vaddr) {
            let pages = alloc_one_page().unwrap();
            let flags = self.get_flags();
            if pages.get_paddr().get_inner()==0x87ff9000{
                let pgt = self.pagetable.as_mut().unwrap();
                let paddr:Paddr = pgt._get_root_page_vaddr().into();
                pgt.walk(vaddr.get_inner());
                info_sync!("fast alloc pgt:{:#X},vaddr:{:#X}",paddr,vaddr);
                println!("1");
            }
            let map_ret = self.pagetable.as_mut().unwrap().map_one_page(vaddr, pages.get_paddr(), _vma_flags_2_pte_flags(flags));
            if map_ret.is_err(){
                let pgt = self.pagetable.as_mut().unwrap();
                let paddr:Paddr = pgt._get_root_page_vaddr().into();
                let ret = pgt.walk(vaddr.get_inner()).unwrap();
                let ret_2 = get_kernel_pagetable().walk(vaddr.get_inner());
                error_sync!("fast alloc error pgt:{:#X},vaddr:{:#X}",paddr,vaddr);
                panic!("err");
            }
            self.pages_tree.insert(vaddr, pages);
        }
    }
    // 注意这个分配物理页不一定是连续的
    fn __fast_alloc_pages(&mut self, vaddr:Vaddr, order:usize){
        for i in vaddr.page_addr_iter(order2pages(order)*PAGE_SIZE){
            self.__fast_alloc_one_page(i);
        }
    }
    pub fn __fast_alloc_one_page_and_get(&mut self, vaddr:Vaddr) ->Arc<Page>{
        debug_assert!(self.in_vma(vaddr));
        debug_assert!(vaddr.is_align());
        if !self._vaddr_have_map(vaddr) {

            let pages = alloc_one_page().unwrap();
            let flags = self.get_flags();
            if self.pagetable.as_mut().unwrap().map_one_page(vaddr, pages.get_paddr(), _vma_flags_2_pte_flags(flags)).is_err(){
                todo!()
            }
            self.pages_tree.insert(vaddr, pages.clone());
            return pages;
        } else {
            self._find_page(vaddr).unwrap()
        }
    }
    //单独map到一个新的物理页
    //会强制map
    pub fn _cow_remap_one_page(&mut self,vaddr:Vaddr,data_pg:Arc<Page>)->Result<(),()>{
        self.pagetable.as_ref().unwrap().unmap_pages(vaddr, 0);
        let new_pg = alloc_one_page().unwrap();
        unsafe { new_pg.copy_one_page_data_from(data_pg); }
        let flags = _vma_flags_2_pte_flags(self.vm_flags);
        self.pagetable.as_ref().unwrap().map_one_page(vaddr, new_pg.get_paddr(), flags).unwrap();
        self.pages_tree.insert(vaddr, new_pg);
        Ok(())
    }
    // for lazy map
    pub fn _do_alloc_one_page(&mut self,vaddr:Vaddr)->Result<Arc<Page>,()>{
        if !vaddr.is_align() || !self.in_vma(vaddr) {
            return Err(());
        }
        let mut ret_pg:Option<Arc<Page>> = None;
        if self.is_anon(){
            // alloc and map but not fill with data
            ret_pg = Some(self.__fast_alloc_one_page_and_get(vaddr));
        } else {
            let file_map_start_vaddr = self.start_vaddr+self.file_in_vma_off;
            let file_map_end_vaddr = self.start_vaddr+self.file_in_vma_off+self.file_len;
            if vaddr<file_map_start_vaddr{
                if (vaddr+PAGE_SIZE)>file_map_start_vaddr{
                    let pg = self.__fast_alloc_one_page_and_get(vaddr);
                    ret_pg = Some(pg.clone());
                    let ptr = pg.get_vaddr().get_inner() as *mut u8;
                    let buf = slice_from_raw_parts_mut(ptr,PAGE_SIZE);
                    let mut left_off:usize = 0;
                    let mut right_off:usize = 0;
                    if (vaddr+PAGE_SIZE)<=file_map_end_vaddr{
                        left_off = (file_map_start_vaddr-vaddr.0).0;
                        right_off = PAGE_SIZE;
                    } else {
                        left_off = (file_map_start_vaddr-vaddr.0).0;
                        right_off = (file_map_end_vaddr-vaddr.0).0;
                    }
                    // read from file
                    let need_read = right_off-left_off;
                    let real_in_file_off = self.file_off;
                    let real_read = unsafe { self.file.as_ref().unwrap().read_off_exact(&mut (*buf)[left_off..right_off], real_in_file_off).unwrap() };
                    assert_eq!(real_read,need_read);
                }  else {
                    // map with no data
                    ret_pg = Some(self.__fast_alloc_one_page_and_get(vaddr));
                }
            } else {
                if vaddr>=file_map_end_vaddr{
                    ret_pg = Some(self.__fast_alloc_one_page_and_get(vaddr));
                } else {
                    let mut right_off:usize = 0;
                    if (vaddr+PAGE_SIZE)<=file_map_end_vaddr{
                        right_off = PAGE_SIZE;
                    } else {
                        right_off = (file_map_end_vaddr - vaddr.0).0;
                    }
                    let real_in_file_off = (vaddr-file_map_start_vaddr.0).0+self.file_off;
                    let pg = self.__fast_alloc_one_page_and_get(vaddr);
                    ret_pg = Some(pg.clone());
                    let ptr = pg.get_vaddr().get_inner() as *mut u8;
                    let buf = slice_from_raw_parts_mut(ptr,PAGE_SIZE);
                    let need_read = right_off;
                    let real_read = unsafe { self.file.as_ref().unwrap().read_off_exact(&mut (*buf)[0..right_off], real_in_file_off).unwrap() };
                    assert_eq!(real_read,need_read);
                }
            }
            // 由于修改了page 所以fence.i需要用于清空icache
            // todo check
            if self.execable(){
                unsafe { fence_i(); }
            }
        }
        if self.writeable(){
            // set dirty
            self.__set_dirty();
        }
        Ok(ret_pg.unwrap())
    }
    fn __release_one_page(&mut self,vaddr:Vaddr){
        match self.pages_tree.remove(&vaddr) {
            None => {}
            Some(pg) => {
                // do unmap pagetable
                self.pagetable.as_mut().unwrap()._unmap_one_page(vaddr).unwrap();
                if self.is_anon(){
                    todo!()
                } else {
                    // file
                    let f = self.file.as_ref().unwrap();
                    let off = self.file_off+(vaddr-self.start_vaddr.0).0;
                    let ptr = pg.get_vaddr().get_inner() as *const u8;
                    let buf = slice_from_raw_parts(ptr,PAGE_SIZE);
                    let write_size = unsafe { f.write_off(&*buf, off).unwrap() };
                    assert_eq!(write_size, PAGE_SIZE);
                }
            }
        }
    }
    pub fn _release_all_page(&mut self){
        todo!()
    }
}

impl Drop for VMA {
    fn drop(&mut self) {
        for (vaddr,v) in &self.pages_tree{
            assert_eq!(v.get_order(), 0);
            let p:Paddr = self.pagetable.as_mut().unwrap()._get_root_page_vaddr().into();
            info_sync!("VMA unmap pgt:{:#X},vaddr:{:#X}",p,vaddr);
            self.pagetable.as_mut().unwrap()._unmap_one_page(*vaddr);
        }
    }
}

// // todo 安全性 是否需要加锁才能访问page
// impl ReadWriteOffUnsafe<u8> for VMA {
//     unsafe fn read_off(&self, buf: &mut [u8], off: usize) -> usize {
//         let size = 1;
//         let buf_size = buf.len() * size;
//         assert!(Vaddr(off).is_align_n(size));
//         assert!(self.start_vaddr+buf_size+off < self.end_vaddr);
//         assert!(self.start_vaddr+off < self.end_vaddr);
//         let start = self.start_vaddr;
//         let mut page_now = self.__fast_alloc_one_page_and_get(start);
//         page_now.seek(SeekFrom::Start(off as u64));
//         let mut buf_index:usize = 0;
//         while buf_index < buf.len() {
//             let read_len = page_now.read(&mut buf[buf_index..]).unwrap();
//             if read_len == 0 {
//                 // change pages
//                 let vaddr_now = start+off + buf_index*size;
//                 if buf_index!=buf.len(){
//                     assert!(vaddr_now.is_align());
//                 }
//                 page_now = self.__fast_alloc_one_page_and_get(vaddr_now);
//                 page_now.seek(SeekFrom::Start(0));
//             } else {
//                 buf_index+=read_len;
//             }
//         }
//         buf_size
//     }
//
//     unsafe fn write_off(&self, buf: &[u8], off: usize) -> usize {
//         let size = 1;
//         let buf_size = buf.len() * size;
//         assert!(Vaddr(off).is_align_n(size));
//         assert!(self.start_vaddr+buf_size+off < self.end_vaddr);
//         assert!(self.start_vaddr+off < self.end_vaddr);
//         let start = self.start_vaddr;
//         let mut page_now = self.__fast_alloc_one_page_and_get(start);
//         page_now.seek(SeekFrom::Start(off as u64));
//         let mut buf_index:usize = 0;
//         while buf_index < buf.len() {
//             let write_len = page_now.write(&buf[buf_index..]).unwrap();
//             if write_len == 0 {
//                 // change pages
//                 let vaddr_now = start+off + buf_index*size;
//                 if buf_index!=buf.len(){
//                     assert!(vaddr_now.is_align());
//                 }
//                 page_now = self.__fast_alloc_one_page_and_get(vaddr_now);
//                 page_now.seek(SeekFrom::Start(0));
//             } else {
//                 buf_index+= write_len;
//             }
//         }
//         buf_size
//     }
// }