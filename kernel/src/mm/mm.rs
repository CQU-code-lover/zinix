use alloc::collections::{BTreeMap, LinkedList};
use alloc::sync::Arc;
use alloc::vec;
use alloc::vec::Vec;
use core::cmp::{max, min};
use core::fmt::{Debug, Formatter};
use core::ops::Bound::{Excluded, Included};
use xmas_elf::ElfFile;
use xmas_elf::program::Type::Load;

use crate::consts::{PAGE_SIZE, PHY_MEM_OFFSET, TMMAP_END, TMMAP_START, USER_HEAP_SIZE_NR_PAGES, USER_SPACE_END, USER_SPACE_START, USER_STACK_MAX_ADDR, USER_STACK_SIZE_NR_PAGES};
use crate::fs::inode::Inode;
use crate::mm::addr::{Addr, PageAlign, PFN, Vaddr};
use crate::mm::{alloc_one_page, alloc_pages, get_kernel_pagetable};
use crate::mm::aux::{AT_BASE, AT_CLKTCK, AT_EGID, AT_ENTRY, AT_EUID, AT_FLAGS, AT_GID, AT_HWCAP, AT_NOTELF, AT_NULL, AT_PAGESZ, AT_PHDR, AT_PHENT, AT_PHNUM, AT_PLATFORM, AT_SECURE, AT_UID, AuxHeader, make_auxv};
use crate::utils::order2pages;
use crate::mm::page::Page;
use crate::mm::pagetable::{PageTable, PTEFlags, WalkRet};
use crate::mm::vma::{MmapFlags, VMA, VMAFlags, VmFlags};
use crate::pre::{ReadWriteOffUnsafe, ReadWriteSingleNoOff};
use crate::println;
use crate::sbi::shutdown;

const VMA_CACHE_MAX:usize = 10;

pub struct MmStruct{
    is_kern:bool,
    // vma_cache:VmaCache,
    pub pagetable:Arc<PageTable>,
    vmas: BTreeMap<Vaddr,VMA>,
    start_brk:Vaddr,
    brk:Vaddr
}

impl Debug for MmStruct {
    fn fmt(&self, f: &mut Formatter<'_>) -> core::fmt::Result {
        writeln!(f,"[Debug MmStruct]");
        for i in self.vmas.iter() {
            writeln!(f, "0x{:x}----0x{:x}", i.get_start_vaddr().0, i.get_end_vaddr().0);
        }
        writeln!(f,"[Debug MmStruct End]");
        Ok(())
    }
}

pub struct VmaCache {
    vmas:LinkedList<Arc<VMA>>
}

impl MmStruct {
    pub fn new_kern_mm_by_pagetable(pagetable:PageTable)->Self{
        MmStruct{
            is_kern: true,
            // vma_cache: VmaCache::new(),
            pagetable: Arc::new(pagetable),
            vmas: Default::default(),
            start_brk: Default::default(),
            brk: Default::default()
        }
    }
    pub fn new_empty_user_mm() ->Self{
        MmStruct{
            is_kern:false,
            // vma_cache: VmaCache::new(),
            pagetable: Arc::new(PageTable::new_user()),
            vmas: Default::default(),
            start_brk: Default::default(),
            brk: Default::default()
        }
    }
    // 这个函数调试使用，未分配物理页的地址会panic
    pub unsafe fn __read_single_by_vaddr<T:Copy+Sized>(&self, vaddr:Vaddr) ->T{
        let vv = self.pagetable.get_kvaddr_by_uvaddr(vaddr);
        if vv.is_none(){
            println!("1");
        }
        let vaddr = vv.unwrap();
        vaddr.read_single().unwrap()
    }
    pub fn is_kern(&self)->bool{
        self.is_kern
    }
    pub fn is_user(&self)->bool{
        !self.is_kern
    }

    pub fn find_vma(&mut self, vaddr: Vaddr) ->Option<&mut VMA>{
        for (k,v) in self.vmas.range_mut((Included(&vaddr),Excluded(&vaddr))) {
            if v.in_vma(vaddr.clone()){
                // find
                return Some(v);
            }
        }
        None
    }
    pub fn find_vma_intersection(&mut self, start_addr: Vaddr, end_addr: Vaddr) ->Option<&mut VMA>{
        match self.find_vma(end_addr) {
            None => {
                None
            }
            Some(vma) => {
                if vma.get_start_vaddr()<=start_addr {
                    Some(vma)
                } else {
                    None
                }
            }
        }
    }
    pub fn merge_vmas(&mut self){
        todo!()
    }
    pub fn _insert_no_check(&mut self,vma:VMA){
        self.vmas.insert(vma.get_start_vaddr(),vma);
    }
    fn __alloc_unmapped_fixed(&self, vaddr:Vaddr, len:usize, range_start:Vaddr, range_end:Vaddr) ->Option<VMA>{
        for (k,_) in self.vmas.range(&vaddr..&range_end) {
            if (k.clone() - vaddr.0).0 >= len{
                return Some(VMA::empty(vaddr,vaddr+len));
            }
            break;
        }
        None
    }
    fn __alloc_unmapped_core(&self,vaddr:Option<Vaddr>,len:usize,to_high:bool,range_start:Vaddr,range_end:Vaddr)->Option<VMA>{
        debug_assert!(vaddr.is_align());
        debug_assert!(Vaddr(len).is_align());
        debug_assert!(range_start.is_align());
        debug_assert!(range_end.is_align());
        debug_assert!(range_start<range_end);
        if len<(range_end-range_start.0).0{
            return None;
        }
        if vaddr.is_some(){
            let vi = vaddr.unwrap();
            debug_assert!(vi>=range_start);
            debug_assert!(vi<range_end);
            return self.__alloc_unmapped_fixed(vi, len, range_start, range_end);
        }
        return if to_high {
            let mut last_end = range_start;
            for (_, v) in self.vmas.range(&range_start..&range_end) {
                let inv = (v.get_start_vaddr() - last_end.0).0;
                if len <= inv {
                    // find
                    return Some(VMA::empty(last_end, last_end + len));
                } else {
                    last_end = v.get_end_vaddr();
                }
            }
            let inv = (range_end - last_end.0).0;
            if len <= inv {
                // find
                Some(VMA::empty(last_end, last_end + len))
            } else {
                None
            }
        } else {
            let mut last_start = range_end;
            for (_, v) in self.vmas.range(&range_start..&range_end).rev() {
                let inv = (last_start - v.get_end_vaddr().0).0;
                if len <= inv {
                    // find
                    return Some(VMA::empty(last_start - len, last_start));
                } else {
                    last_start = v.get_start_vaddr();
                }
            }
            let inv = (last_start - range_start.0).0;
            if len <= inv {
                // find
                Some(VMA::empty(last_start - len, last_start))
            } else {
                None
            }
        }
    }
    pub fn alloc_mmap_anon(&self,vaddr:Option<Vaddr>,len:usize,map_flags:MmapFlags)->Option<VMA> {
        let to_high = false;
    }
    pub fn alloc_mmap_file()->Option<VMA> {
        todo!()
    }
    // kmap 默认不需要指定vaddr
    pub fn alloc_kmap_anon(&self,vaddr:Option<Vaddr>,len:usize)->Option<VMA> {

    }
    pub fn alloc_kmap_file(&self,vaddr:Option<Vaddr>,len:usize,file:Arc<Inode>,off:usize)->Option<VMA> {
        let to_high = true;
        self.__alloc_unmapped_core(vaddr,len,to_high,Vaddr(TMMAP_START),Vaddr(TMMAP_END)).map(
            |mut vma| {
                vma.pagetable = self.pagetable.clone();
                vma.file_off = off;
                vma.file = Some(file);
                vma.vm_flags = VmFlags::VM_READ|VmFlags::VM_WRITE|VmFlags::VM_EXEC;
                vma
            }
        )
    }
    // arg len not check
    // helper func, can be only invoked by get_unmapped_area_no_insert
    // vaddr=>vaddr+len must in map space, checked by caller
    // fn _get_unmapped_area_assign_no_insert(&mut self, len:usize, flags:u8, vaddr: Vaddr) ->Option<Arc<VMA>>{
    //     assert!(vaddr.is_align());
    //     let mut ret :Option<Arc<VMA>> = None;
    //     if self.vmas.is_empty() {
    //         return  Some(VMA::new(
    //             vaddr,
    //             vaddr+ len,
    //             self.pagetable.clone(),
    //             flags,
    //         ));
    //     }
    //     let mut cursor = self.vmas.cursor_front_mut();
    //     while match cursor.index() {
    //         Some(_)=>true,
    //         None=>false
    //     }{
    //         let cur = cursor.current().unwrap();
    //         if cur.in_vma(vaddr) {
    //             break;
    //         }
    //         match cursor.peek_next() {
    //             None => {
    //                 // cur is last node
    //                 ret = Some(VMA::new(
    //                     vaddr,
    //                     vaddr+ len,
    //                     self.pagetable.clone(),
    //                     flags,
    //                 ));
    //                 break;
    //             }
    //             Some(next) => {
    //                 if next.get_start_vaddr()>vaddr{
    //                     // have found a valid hole
    //                     if (next.get_start_vaddr()-vaddr.0).0 >=len {
    //                         ret = Some(VMA::new(
    //                             vaddr,
    //                             vaddr+ len,
    //                             self.pagetable.clone(),
    //                             flags
    //                         ));
    //                         break;
    //                     }
    //                 }
    //             }
    //         }
    //         cursor.move_next();
    //     }
    //     ret
    // }
    //
    // // must page align
    // // will do check of mm range
    // // core function
    // pub fn _get_unmapped_area_no_insert(&mut self, len:usize, flags:u8, vaddr:Option<Vaddr>) ->Option<Arc<VMA>>{
    //     // check len
    //     assert_eq!(len % PAGE_SIZE, 0);
    //     let (space_start,space_end) = if self.is_kern(){
    //         (Vaddr(TMMAP_START),Vaddr(TMMAP_END))
    //     } else {
    //         (Vaddr(USER_SPACE_START),Vaddr(USER_SPACE_END))
    //     };
    //     if vaddr.is_some(){
    //         assert!(vaddr.as_ref().unwrap().is_align());
    //         let vaddr_inner = vaddr.unwrap();
    //         if vaddr_inner>=space_start&&vaddr_inner<space_end {
    //             return self._get_unmapped_area_assign_no_insert(len, flags, vaddr_inner);
    //         } else {
    //             return None;
    //         }
    //     }
    //     if self.vmas.is_empty() {
    //         return  Some(VMA::new(
    //             space_start,
    //             {let end = space_start + len;
    //                 if end>=space_end {
    //                     return None;
    //                 }
    //                 end},
    //             self.pagetable.clone(),
    //             flags,
    //         ));
    //     }
    //     let mut ret :Option<Arc<VMA>> = None;
    //     let mut cursor = self.vmas.cursor_front_mut();
    //     while match cursor.index() {
    //         Some(_)=>true,
    //         None=>false
    //     }{
    //         let cur_end_addr = cursor.current().unwrap().get_end_vaddr();
    //         match cursor.peek_next() {
    //             None => {
    //                 // cur is last node
    //                 // check Mm range
    //                 // todo check Mm range
    //                 ret = Some(VMA::new(
    //                     cur_end_addr,
    //                     {
    //                         let end = cur_end_addr+ len;
    //                         if end>=space_end{
    //                             return None;
    //                         }
    //                         end
    //                     },
    //                     self.pagetable.clone(),
    //                     flags
    //                 ));
    //                 break;
    //             }
    //             Some(next) => {
    //                 if (next.get_start_vaddr()-cur_end_addr.0).0 >= len {
    //                     // have found a valid hole
    //                     ret = Some(VMA::new(
    //                         cur_end_addr,
    //                         cur_end_addr+ len,
    //                         self.pagetable.clone(),
    //                         flags
    //                     ));
    //                     break;
    //                 }
    //             }
    //         }
    //         cursor.move_next();
    //     }
    //     ret
    // }

    // len/vaddr必须align
    // 会自动插入area
    // pub fn get_unmapped_area(&mut self, len:usize, flags:u8, vaddr:Option<Vaddr>)->Option<Arc<VMA>> {
    //     self._get_unmapped_area_no_insert(len, flags, vaddr).map(|area|{
    //         self.__insert_vma(area.clone());
    //         area
    //     })
    // }

    // pub fn get_unmapped_area_alloc(&mut self, len:usize, flags:u8, vaddr:Option<Vaddr>) ->Option<Arc<VMA>>{
    //     self.get_unmapped_area(len,flags,vaddr).map(|area|{
    //         let addr_ = if vaddr.is_some(){
    //             vaddr.unwrap()
    //         } else {
    //             area.get_start_vaddr()
    //         };
    //         for i in addr_.page_addr_iter(len){
    //             area._annon_fast_alloc_one_page(i);
    //         }
    //         area
    //     })
    // }

    // 只能在页表已经install的时候使用
    pub unsafe fn flush(&self){
        self.pagetable.flush_self();
    }
    pub unsafe fn install_pagetable(&self){
        self.pagetable.install();
    }
    pub fn new_from_elf(elf_bytes:&[u8]) ->(Self, Vec<AuxHeader>, usize){
        let mut mm = Self::new_empty_user_mm();
        let elf = ElfFile::new(elf_bytes).unwrap();
        let elf_header = elf.header;
        assert_eq!([0x7f, 0x45, 0x4c, 0x46],elf_header.pt1.magic);
        let ph_count = elf_header.pt2.ph_count();
        let mut head_va:usize = 0;
        let mut load_end = Vaddr(0);

        let entry = elf.header.pt2.entry_point();

        let mut auxv = Vec::new();

        auxv.push(AuxHeader{aux_type: AT_PHENT, value: elf.header.pt2.ph_entry_size() as usize});// ELF64 header 64bytes
        // todo AT_PHNUM
        auxv.push(AuxHeader{aux_type: AT_PHNUM, value: 0 as usize});

        // auxv.push(AuxHeader{aux_type: AT_PHNUM, value: ph_count as usize});
        auxv.push(AuxHeader{aux_type: AT_PAGESZ, value: PAGE_SIZE as usize});
        auxv.push(AuxHeader{aux_type: AT_BASE, value: 0 as usize});
        auxv.push(AuxHeader{aux_type: AT_FLAGS, value: 0 as usize});
        auxv.push(AuxHeader{aux_type: AT_ENTRY, value: entry as usize});
        auxv.push(AuxHeader{aux_type: AT_UID, value: 0 as usize});
        auxv.push(AuxHeader{aux_type: AT_EUID, value: 0 as usize});
        auxv.push(AuxHeader{aux_type: AT_GID, value: 0 as usize});
        auxv.push(AuxHeader{aux_type: AT_EGID, value: 0 as usize});
        auxv.push(AuxHeader{aux_type: AT_PLATFORM, value: 0 as usize});
        auxv.push(AuxHeader{aux_type: AT_HWCAP, value: 0 as usize});
        auxv.push(AuxHeader{aux_type: AT_CLKTCK, value: 100 as usize});
        auxv.push(AuxHeader{aux_type: AT_SECURE, value: 0 as usize});
        auxv.push(AuxHeader{aux_type: AT_NOTELF, value: 0x112d as usize});
        let a = elf.header.pt2.ph_offset();

        // todo AT_PHDR
        // let ph_head_addr = head_va + elf.header.pt2.ph_offset() as usize;
        // auxv.push(AuxHeader{aux_type: AT_PHDR, value: ph_head_addr as usize});
        auxv.push(AuxHeader{aux_type: AT_NULL, value: 0 as usize});
        let a1 = elf.header.pt2.entry_point();

        for ph in elf.program_iter() {
            if ph.get_type().unwrap() == Load {
                let mut s_addr = Vaddr(ph.virtual_addr() as usize);
                let offset = (s_addr-s_addr.floor().0).0;
                // align start addr
                s_addr = s_addr.floor();
                if offset == 0{
                    head_va = s_addr.0;
                }
                let size_aligned = Vaddr(ph.mem_size() as usize + offset).ceil().0;
                let ph_flags = ph.flags();
                let mut vma_flags:u8 = VMAFlags::VM_USER.bits();
                if ph_flags.is_read() {
                    vma_flags|=VMAFlags::VM_READ.bits();
                }
                if ph_flags.is_write() {
                    vma_flags|=VMAFlags::VM_WRITE.bits();
                }
                if ph_flags.is_execute() {
                    vma_flags|=VMAFlags::VM_EXEC.bits();
                }

                let mut area = mm.get_unmapped_area_alloc(size_aligned, vma_flags, Some(s_addr)).unwrap();

                unsafe { area.write_off(&elf.input[ph.offset() as usize..(ph.offset() + ph.file_size()) as usize] ,offset);}
                load_end = max(load_end,area.get_end_vaddr());
            }
        }
        // heap
        let heap_start = load_end.ceil()+ PAGE_SIZE;

        mm.get_unmapped_area(USER_HEAP_SIZE_NR_PAGES*PAGE_SIZE,
                             VMAFlags::VM_READ.bits()|VMAFlags::VM_WRITE.bits()|VMAFlags::VM_USER.bits(),
                             Some(heap_start)
        ).unwrap();

        // let v_ = mm.pagetable.get_kvaddr_by_uvaddr(Vaddr(0x277B0)).unwrap();
        // let vv:usize  = unsafe{v_.read_single()}.unwrap();
        // println!("{:#X}",vv);
        // shutdown();
        // mm.alloc_phy_pages_check(heap_start,4,
        //                          |x,y| {}
        // );

        mm.get_unmapped_area_alloc(
            USER_STACK_SIZE_NR_PAGES*PAGE_SIZE,
            VMAFlags::VM_READ.bits()|VMAFlags::VM_WRITE.bits()|VMAFlags::VM_USER.bits(),
            Some(Vaddr(USER_STACK_MAX_ADDR-(USER_STACK_SIZE_NR_PAGES*PAGE_SIZE))),
        ).unwrap();
        // alloc all phy page for user stack
        // mm.alloc_phy_pages_check(Vaddr(USER_STACK_MAX_ADDR-(USER_STACK_SIZE_NR_PAGES*PAGE_SIZE)), 4,
        //                          |x,y| {}
        // );
        (mm,auxv,elf.header.pt2.entry_point() as usize)
    }
}

impl VmaCache {
    fn new()->Self{
        VmaCache{
            vmas: LinkedList::new()
        }
    }
    fn len(&self)->usize {
        self.vmas.len()
    }
    fn push_new_cache(&mut self, vma:Arc<VMA>){
        if self.vmas.len()==VMA_CACHE_MAX{
            self.vmas.pop_back();
        }
        self.vmas.push_front(vma);
    }
    fn check(&mut self, vaddr: Vaddr) ->Option<Arc<VMA>>{
        let mut cursor = self.vmas.cursor_front_mut();
        while match cursor.index() {
            Some(_)=>{
                true
            }
            None=>false
        }{
            // unsafe func to bypass borrow checker?
            // the impl of current is unsafe.
            if cursor.current().unwrap().in_vma(vaddr){
                // cache hit
                let cur = cursor.remove_current().unwrap();
                self.vmas.push_front(cur.clone());
                return Some(cur);
            }
            cursor.move_next();
        }
        // cache miss
        None
    }
}