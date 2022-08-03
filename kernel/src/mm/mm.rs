use alloc::collections::LinkedList;
use alloc::sync::Arc;
use alloc::vec;
use alloc::vec::Vec;
use core::cmp::{max, min};
use core::fmt::{Debug, Formatter};
use xmas_elf::ElfFile;
use xmas_elf::program::Type::Load;

use crate::consts::{PAGE_SIZE, PHY_MEM_OFFSET, USER_HEAP_SIZE_NR_PAGES, USER_SPACE_END, USER_SPACE_START, USER_STACK_MAX_ADDR, USER_STACK_SIZE_NR_PAGES};
use crate::mm::addr::{Addr, PFN};
use crate::mm::{alloc_one_page, alloc_pages, get_kernel_pagetable};
use crate::mm::aux::{AT_BASE, AT_CLKTCK, AT_EGID, AT_ENTRY, AT_EUID, AT_FLAGS, AT_GID, AT_HWCAP, AT_NOTELF, AT_NULL, AT_PAGESZ, AT_PHDR, AT_PHENT, AT_PHNUM, AT_PLATFORM, AT_SECURE, AT_UID, AuxHeader, make_auxv};
use crate::mm::buddy::order2pages;
use crate::mm::page::Page;
use crate::mm::pagetable::{PageTable, PTEFlags, WalkRet};
use crate::mm::vma::{VMA, VMAFlags};
use crate::println;
use crate::sbi::shutdown;

const VMA_CACHE_MAX:usize = 10;

pub struct MmStruct{
    vma_cache:VmaCache,
    pub pagetable:Arc<PageTable>,
    vmas : LinkedList<Arc<VMA>>
}

impl Debug for MmStruct {
    fn fmt(&self, f: &mut Formatter<'_>) -> core::fmt::Result {
        writeln!(f,"[Debug MmStruct]");
        for i in self.vmas.iter() {
            writeln!(f,"0x{:x}----0x{:x}",i.get_start_addr().0,i.get_end_addr().0);
        }
        writeln!(f,"[Debug MmStruct End]");
        Ok(())
    }
}

pub struct VmaCache {
    vmas:LinkedList<Arc<VMA>>
}

impl MmStruct {
    pub fn new_empty() ->Self{
        MmStruct{
            vma_cache: VmaCache::new(),
            pagetable: Arc::new(PageTable::new_user()),
            vmas: Default::default()
        }
    }
    pub fn find_vma(&mut self, vaddr:Addr)->Option<Arc<VMA>>{
        let check_ret = self.vma_cache.check(vaddr);
        match  check_ret{
            None => {}
            Some(_) => {
                return check_ret;
            }
        }
        let mut ret:Option<Arc<VMA>> = None;
        for vma in &self.vmas {
            if vma.in_vma(vaddr) {
                ret = Some(vma.clone());
                break;
            }
        }
        ret
    }
    pub fn find_vma_intersection(&mut self,start_addr:Addr, end_addr:Addr)->Option<Arc<VMA>>{
        match self.find_vma(end_addr) {
            None => {
                None
            }
            Some(vma) => {
                if vma.get_start_addr()<=start_addr {
                    Some(vma)
                } else {
                    None
                }
            }
        }
    }
    pub fn merge_vmas(&mut self){

    }
    // private func for insert vma.
    // not check if the vma is valid.
    pub fn _insert_vma(&mut self,vma:Arc<VMA>){
        if self.vmas.is_empty() {
            self.vmas.push_back(vma);
            return;
        }
        let mut cursor = self.vmas.cursor_front_mut();
        while match cursor.index() {
            Some(_)=>true,
            None=>false
        }{
            if cursor.current().unwrap().get_end_addr()<=vma.get_start_addr(){
                cursor.insert_after(vma);
                return ;
            }
            cursor.move_next();
        }
        panic!("_insert_vma Bug");
    }
    // arg len not check
    fn _get_unmapped_area_assign(&mut self,len:usize,flags:u8,vaddr:Addr)->Option<Arc<VMA>>{
        assert!(vaddr.is_align());
        let mut ret :Option<Arc<VMA>> = None;
        // todo! check mm range
        if self.vmas.is_empty() {
            return  Some(VMA::new(
                vaddr,
                vaddr+Addr(len),
                self.pagetable.clone(),
                flags,
            ));
        }
        let mut cursor = self.vmas.cursor_front_mut();
        while match cursor.index() {
            Some(_)=>true,
            None=>false
        }{
            let cur = cursor.current().unwrap();
            if cur.in_vma(vaddr) {
                break;
            }
            match cursor.peek_next() {
                None => {
                    // cur is last node
                    // todo check Mm range
                    ret = Some(VMA::new(
                        vaddr,
                        vaddr+Addr(len),
                        self.pagetable.clone(),
                        flags,
                    ));
                    break;
                }
                Some(next) => {
                    if next.get_start_addr()>vaddr{
                        // have found a valid hole
                        if (next.get_start_addr()-vaddr).0 >=len {
                            ret = Some(VMA::new(
                                vaddr,
                                vaddr+Addr(len),
                                self.pagetable.clone(),
                                flags
                            ));
                            break;
                        }
                    }
                }
            }
            cursor.move_next();
        }
        ret
    }

    // must page align
    pub fn get_unmapped_area(&mut self,len:usize,flags:u8,vaddr:Option<Addr>)->Option<Arc<VMA>>{
        // check len
        if len%PAGE_SIZE!=0{
            panic!("get_unmapped_area fail, len is not page align");
        }

        if vaddr.is_some(){
            return self._get_unmapped_area_assign(len,flags,vaddr.unwrap());
        }
        if self.vmas.is_empty() {
            return  Some(VMA::new(
                Addr(USER_SPACE_START),
                Addr(USER_SPACE_START)+Addr(len),
                self.pagetable.clone(),
                flags,
            ));
        }
        let mut ret :Option<Arc<VMA>> = None;
        let mut cursor = self.vmas.cursor_front_mut();
        while match cursor.index() {
            Some(_)=>true,
            None=>false
        }{
            let cur_end_addr = cursor.current().unwrap().get_end_addr();
            match cursor.peek_next() {
                None => {
                    // cur is last node
                    // check Mm range
                    // todo check Mm range
                    ret = Some(VMA::new(
                        cur_end_addr,
                        cur_end_addr+Addr(len),
                        self.pagetable.clone(),
                        flags
                    ));
                    break;
                }
                Some(next) => {
                    if (next.get_start_addr()-cur_end_addr).0 >= len {
                        // have found a valid hole
                        ret = Some(VMA::new(
                            cur_end_addr,
                            cur_end_addr+Addr(len),
                            self.pagetable.clone(),
                            flags
                        ));
                        break;
                    }
                }
            }
            cursor.move_next();
        }
        ret
    }
    pub fn get_unmapped_area_and_insert(&mut self,len:usize,flags:u8,vaddr:Option<Addr>,data:Option<&[u8]>,offset :usize)->Option<Arc<VMA>>{
        let vma = self.get_unmapped_area(len,flags,vaddr).map(|x|{
            self._insert_vma(x.clone());
            x
        });
        if data.is_some() {
            let s = vma.as_ref().unwrap().get_start_addr();
            let mut pos:usize = 0;
            let data_inner = data.unwrap();
            let mut first_pg = true;
            let mut in_page_offset = offset;
            for i in s.page_iter(Addr(data_inner.len()+offset).ceil().0){
                self.alloc_phy_pages_check(i,0,|addr,len|{
                    let write_buf = &data_inner[pos..];
                    let real_write =  if first_pg {
                        min(PAGE_SIZE-offset,write_buf.len())
                    } else {
                        min(PAGE_SIZE,write_buf.len())
                    };
                    for j in 0..real_write{
                        unsafe {
                            *((addr.0+in_page_offset+j) as *mut u8) = write_buf[j];
                        }
                    }
                });
                in_page_offset =0;
                if first_pg {
                    pos += PAGE_SIZE-offset;
                    first_pg = false;
                } else {
                    pos += PAGE_SIZE;
                }
                if pos>=data_inner.len(){
                    break;
                }
            }
        }
        vma
    }
    pub fn alloc_phy_pages_no_check<F>(&mut self, vaddr:Addr,order:usize,f:F) ->Result<(),isize>
    where
        F:FnOnce(Addr,usize)->()
    {
        if !vaddr.is_align() {
            return Err(-1);
        }
        let page_cnt = order2pages(order);
        if page_cnt==0 {
            return Err(-1);
        }
        match self.find_vma(vaddr) {
            None => {
                // vma is not allocated for this vaddr
                Err(-1)
            }
            Some(vma) => {
                let pages = alloc_pages(order).unwrap();
                let allocated_pages_vaddr:Addr = pages.get_pfn().into();
                f(allocated_pages_vaddr, page_cnt*PAGE_SIZE);
                vma.map_pages(pages, vaddr);
                Ok(())
            }
        }
    }
    pub fn alloc_phy_pages_check<F>(&mut self, vaddr:Addr,order:usize,f:F) ->Result<(),isize>
    where
        F:FnOnce(Addr,usize)->()
    {
        match self.pagetable.walk(vaddr.0){
            None => {
                // ok
                self.alloc_phy_pages_no_check(vaddr,order,f)
            }
            Some(walk) => {
                // last level
                if walk.get_level()==2 {
                    // ok
                    self.alloc_phy_pages_no_check(vaddr,order,f)
                } else {
                    // big page
                    Err(-1)
                }
            }
        }
    }
    // 只能在页表已经install的时候使用
    pub unsafe fn flush(&self){
        self.pagetable.flush_self();
    }
    pub unsafe fn install_pagetable(&self){
        self.pagetable.install();
    }
    pub fn new_from_elf(elf_bytes:&[u8]) ->(Self, Vec<AuxHeader>, usize){
        let mut mm = Self::new_empty();
        let elf = ElfFile::new(elf_bytes).unwrap();
        let elf_header = elf.header;
        assert_eq!([0x7f, 0x45, 0x4c, 0x46],elf_header.pt1.magic);
        let ph_count = elf_header.pt2.ph_count();
        let mut head_va:usize = 0;
        let mut load_end = Addr(0);

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
                let mut s_addr = Addr(ph.virtual_addr() as usize);
                let offset = (s_addr-s_addr.floor()).0;
                // align start addr
                s_addr = s_addr.floor();
                if offset == 0{
                    head_va = s_addr.0;
                }
                let size_aligned = Addr(ph.mem_size() as usize + offset).ceil().0;
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
                let a2 = elf.header.pt2.entry_point();
                let area = mm.get_unmapped_area_and_insert(
                    size_aligned,
                    vma_flags,
                    Some(s_addr),
                    Some(&elf.input[ph.offset() as usize..(ph.offset() + ph.file_size()) as usize]),
                    offset
                ).unwrap();

                load_end = max(load_end,area.get_end_addr());
            }
        }

        // heap
        let heap_start = load_end.ceil()+Addr(PAGE_SIZE);
        mm.get_unmapped_area_and_insert(
            USER_HEAP_SIZE_NR_PAGES*PAGE_SIZE,
            VMAFlags::VM_READ.bits()|VMAFlags::VM_WRITE.bits()|VMAFlags::VM_USER.bits(),
            Some(heap_start),
            None,
            0
        ).unwrap();
        // let re = get_kernel_pagetable().lock().unwrap().walk(0x3000).unwrap();
        mm.alloc_phy_pages_check(heap_start,4,
                                 |x,y| {}
        );

        mm.get_unmapped_area_and_insert(
            USER_STACK_SIZE_NR_PAGES*PAGE_SIZE,
            VMAFlags::VM_READ.bits()|VMAFlags::VM_WRITE.bits()|VMAFlags::VM_USER.bits(),
            Some(Addr(USER_STACK_MAX_ADDR-(USER_STACK_SIZE_NR_PAGES*PAGE_SIZE))),
            None,
            0
        ).unwrap();
        // alloc all phy page for user stack
        mm.alloc_phy_pages_check(Addr(USER_STACK_MAX_ADDR-(USER_STACK_SIZE_NR_PAGES*PAGE_SIZE)),4,
        |x,y| {}
        );
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
    fn check(&mut self,vaddr:Addr)->Option<Arc<VMA>>{
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