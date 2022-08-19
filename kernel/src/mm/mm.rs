use alloc::collections::{BTreeMap, LinkedList};
use alloc::sync::Arc;
use alloc::vec;
use alloc::vec::Vec;
use core::cmp::{max, min};
use core::fmt::{Debug, Formatter};
use core::ops::Bound::{Excluded, Included};
use xmas_elf::ElfFile;
use xmas_elf::program::Type::Load;

use crate::consts::{MMAP_TOP, PAGE_OFFSET, PAGE_SIZE, PHY_MEM_OFFSET, KMAP_END, KMAP_START, USER_HEAP_VMA_INIT_NR_PAGES, USER_SPACE_END, USER_SPACE_START, USER_STACK_MAX_ADDR, USER_STACK_SIZE_NR_PAGES};
use crate::fs::inode::Inode;
use crate::mm::addr::{Addr, PageAlign, PFN, Vaddr};
use crate::mm::{alloc_one_page, alloc_pages, get_kernel_pagetable};
use crate::mm::aux::{AT_BASE, AT_CLKTCK, AT_EGID, AT_ENTRY, AT_EUID, AT_FLAGS, AT_GID, AT_HWCAP, AT_NOTELF, AT_NULL, AT_PAGESZ, AT_PHDR, AT_PHENT, AT_PHNUM, AT_PLATFORM, AT_SECURE, AT_UID, AuxHeader, make_auxv};
use crate::utils::order2pages;
use crate::mm::page::Page;
use crate::mm::pagetable::{PageTable, PTEFlags, WalkRet};
use crate::mm::vma::{_vma_flags_2_pte_flags, MmapFlags, MmapProt, VMA, VmFlags};
use crate::pre::{ReadWriteOffUnsafe, ReadWriteSingleNoOff, ShowRdWrEx};
use crate::{println, SpinLock, warn_sync};
use crate::sbi::shutdown;

const VMA_CACHE_MAX:usize = 10;

pub struct MmStruct{
    is_kern:bool,
    // vma_cache:VmaCache,
    pub pagetable:Arc<PageTable>,
    vmas: BTreeMap<Vaddr,VMA>,
    start_brk:Vaddr,
    brk:Vaddr,
    pub cow_target:Option<Arc<SpinLock<MmStruct>>>
}

impl Debug for MmStruct {
    fn fmt(&self, f: &mut Formatter<'_>) -> core::fmt::Result {
        writeln!(f,"[Debug MmStruct]");
        for (k,v) in self.vmas.iter() {
            writeln!(f, "0x{:x}----0x{:x}", v.get_start_vaddr().0, v.get_end_vaddr().0);
        }
        writeln!(f,"[Debug MmStruct End]");
        Ok(())
    }
}

pub struct VmaCache {
    vmas:LinkedList<Arc<VMA>>
}

#[cfg(not(feature = "copy_on_write"))]
pub fn new_mm_by_old(old:Arc<SpinLock<MmStruct>>) ->MmStruct{
    let mut old_locked = old.lock_irq().unwrap();
    let new_pagetable = PageTable::new_user();
    let mut new = MmStruct::new_empty_user_mm_by_pagetable(new_pagetable);
    new.brk = old_locked.brk;
    new.start_brk = old_locked.start_brk;
    // copy vmas
    for (vaddr,vma) in old_locked.vmas.range_mut(&Vaddr(USER_SPACE_START)..&Vaddr(USER_SPACE_END)){
        let mut new_vma = VMA{
            start_vaddr: vma.start_vaddr,
            end_vaddr: vma.end_vaddr,
            vm_flags: vma.vm_flags,
            pages_tree: BTreeMap::new(),
            pagetable: Some(new.pagetable.clone()),
            file: vma.file.as_ref().map(|x|{x.clone()}),
            file_off: vma.file_off,
            file_in_vma_off: vma.file_in_vma_off,
            file_len: vma.file_len,
            phy_pgs_cnt: vma.phy_pgs_cnt,
            cow_write_reserve_pgs: None
        };
        let vma_pgt = vma.pagetable.as_ref().unwrap();
        for (vv, pg) in vma.pages_tree.iter() {
            // change map flags
            let new_pg = new_vma.__fast_alloc_one_page_and_get(vv.clone());
            unsafe { new_pg.copy_one_page_data_from(pg.clone()); }
        }
        assert!(new.vmas.insert(vaddr.clone(),new_vma).is_none());
    }
    new
}

#[cfg(feature = "copy_on_write")]
pub fn new_mm_by_old(old:Arc<SpinLock<MmStruct>>) ->MmStruct{
    let mut old_locked = old.lock_irq().unwrap();
    if old_locked.cow_target.is_some(){
        // 正在cow其他mm
        todo!()
    }
    let new_pagetable = old_locked.new_cow_pagetable();
    let mut new = MmStruct::new_empty_user_mm_by_pagetable(new_pagetable);
    new.cow_target = Some(old.clone());
    new.brk = old_locked.brk;
    new.start_brk = old_locked.start_brk;
    // copy vmas
    for (vaddr,vma) in old_locked.vmas.range_mut(&Vaddr(USER_SPACE_START)..&Vaddr(USER_SPACE_END)){
        warn_sync!("{:?}",vma);
        // writeable需要设置为不可write
        if vma.writeable(){
            vma.cow_write_reserve_pgs = Some(BTreeMap::new());
            let vma_pgt = vma.pagetable.as_ref().unwrap();
            for (vv,pg) in vma.pages_tree.iter(){
                // change map flags
                let new_flags = _vma_flags_2_pte_flags(vma.vm_flags&(!VmFlags::VM_WRITE));
                vma_pgt.change_map_flags(vv.clone(),new_flags).unwrap();
            }
        }
        let new_vma = VMA{
            start_vaddr: vma.start_vaddr,
            end_vaddr: vma.end_vaddr,
            vm_flags: vma.vm_flags,
            // 空tree map 不关联任何物理页
            pages_tree: BTreeMap::new(),
            pagetable: Some(new.pagetable.clone()),
            file: vma.file.as_ref().map(|x|{x.clone()}),
            file_off: vma.file_off,
            file_in_vma_off: vma.file_in_vma_off,
            file_len: vma.file_len,
            phy_pgs_cnt: vma.phy_pgs_cnt,
            cow_write_reserve_pgs: None
        };
        assert!(new.vmas.insert(vaddr.clone(),new_vma).is_none());
    }
    new
}

impl MmStruct {
    // 创建cow使用的页表，所有的页表项都要重新分配，但是页使用共享映射并且去除write
    fn new_cow_pagetable(&self)->PageTable{
        let new_user = PageTable::new_user();
        let old_pagetable = self.pagetable.clone();
        // 只需要cow用户空间
        for (vaddr,vma) in self.vmas.range(&Vaddr(USER_SPACE_START)..&Vaddr(USER_SPACE_END)) {
            for (va,pg) in vma.pages_tree.iter() {
                let pa = pg.get_paddr();
                let new_flags = vma.vm_flags&(!VmFlags::VM_WRITE);
                new_user.map_one_page(va.clone(),pa.clone(),_vma_flags_2_pte_flags(new_flags));
            }
        }
        new_user
    }
    pub fn new_kern_mm_by_pagetable(pagetable:PageTable)->Self{
        Self{
            is_kern: true,
            // vma_cache: VmaCache::new(),
            pagetable: Arc::new(pagetable),
            vmas: Default::default(),
            start_brk: Default::default(),
            brk: Default::default(),
            cow_target:None
        }
    }
    pub fn new_empty_user_mm_by_pagetable(pagetable:PageTable)->Self{
        Self{
            is_kern:false,
            // vma_cache: VmaCache::new(),
            pagetable: Arc::new(pagetable),
            vmas: Default::default(),
            start_brk: Default::default(),
            brk: Default::default(),
            cow_target: None
        }
    }
    pub fn new_empty_user_mm() ->Self{
        Self{
            is_kern:false,
            // vma_cache: VmaCache::new(),
            pagetable: Arc::new(PageTable::new_user()),
            vmas: Default::default(),
            start_brk: Default::default(),
            brk: Default::default(),
            cow_target: None
        }
    }
    pub fn get_brk(&self)->Vaddr{
        self.brk
    }
    pub fn get_start_brk(&self)->Vaddr{
        self.start_brk
    }
    pub fn set_brk(&mut self,new:Vaddr)->Vaddr{
        let ret = self.brk;
        self.brk = new;
        ret
    }
    pub fn _expand_brk(&mut self,new_brk:Vaddr)->Result<(),()> {
        let m = self.vmas.range(self.start_brk..Vaddr(MMAP_TOP)).skip(1).next();
        match m {
            None => {
                if new_brk>Vaddr(MMAP_TOP) {
                    return Err(());
                }
            }
            Some((start_vaddr,_)) => {
                if *start_vaddr<new_brk{
                    return Err(());
                }
            }
        }
        self.find_vma(self.start_brk).unwrap().__set_end_vaddr(new_brk);
        self.set_brk(new_brk);
        Ok(())
    }
    pub fn _shrink_brk(&mut self,new_brk:Vaddr)->Result<(),()>{
        match self.find_vma(self.start_brk).unwrap().split(new_brk){
            None => {
                Err(())
            }
            Some(_) => {
                Ok(())
            }
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
        for (k,v) in self.vmas.iter_mut() {
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
            } else {
                return None;
            }
        }
        // range中没有vma
        Some(VMA::empty(vaddr,vaddr+len))
    }
    fn __alloc_unmapped_core(&self,vaddr:Option<Vaddr>,len:usize,to_high:bool,range_start:Vaddr,range_end:Vaddr)->Option<VMA>{
        debug_assert!(Vaddr(len).is_align());
        debug_assert!(range_start.is_align());
        debug_assert!(range_end.is_align());
        debug_assert!(range_start<range_end);
        if len>=(range_end-range_start.0).0{
            return None;
        }
        if vaddr.is_some(){
            let vi = vaddr.unwrap();
            //检查range
            if (vi+len)>range_end{
                return None;
            }
            debug_assert!(vi.is_align());
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
    // mmap must set VM_USER
    pub fn alloc_mmap_anon(&self,vaddr:Option<Vaddr>,len:usize,map_flags:MmapFlags,prot_flags:MmapProt)->Option<VMA> {
        let to_high = false;
        self.__alloc_unmapped_core(vaddr,len,to_high,Vaddr(USER_SPACE_START),Vaddr(MMAP_TOP)).map(
            |mut vma| {
                vma.pagetable = Some(self.pagetable.clone());
                vma.vm_flags = VmFlags::from_mmap(map_flags,prot_flags);
                vma
            }
        )
    }
    pub fn alloc_mmap_file(&self, vaddr:Option<Vaddr>, len:usize, file:Arc<Inode>, file_off:usize,file_len:usize, map_flags:MmapFlags, prot_flags:MmapProt) ->Option<VMA> {
        let to_high = false;
        assert!(file_len<=len);
        self.__alloc_unmapped_core(vaddr,len,to_high,Vaddr(USER_SPACE_START),Vaddr(MMAP_TOP)).map(
            |mut vma| {
                vma.pagetable = Some(self.pagetable.clone());
                vma.file_off = file_off;
                vma.file = Some(file);
                vma.vm_flags = VmFlags::from_mmap(map_flags,prot_flags);
                vma.file_len = file_len;
                vma.file_in_vma_off = 0;
                vma
            }
        )
    }
    // kmap 默认不需要指定vaddr
    pub fn alloc_kmap_anon(&self,len:usize)->Option<VMA> {
        let to_high = true;
        self.__alloc_unmapped_core(None, len, to_high, Vaddr(KMAP_START), Vaddr(KMAP_END)).map(
            |mut vma| {
                vma.pagetable = Some(self.pagetable.clone());
                vma.vm_flags = VmFlags::VM_READ|VmFlags::VM_WRITE|VmFlags::VM_EXEC|VmFlags::VM_ANON;
                vma
            }
        )
    }
    pub fn alloc_kmap_file(&self, len:usize, file:Arc<Inode>, file_off:usize, file_len:usize) ->Option<VMA> {
        let to_high = true;
        assert!(file_len<=len);
        self.__alloc_unmapped_core(None, len, to_high, Vaddr(KMAP_START), Vaddr(KMAP_END)).map(
            |mut vma| {
                vma.pagetable = Some(self.pagetable.clone());
                vma.file_off = file_off;
                vma.file = Some(file);
                vma.file_len = file_len;
                vma.file_in_vma_off = 0;
                vma.vm_flags = VmFlags::VM_READ|VmFlags::VM_WRITE|VmFlags::VM_EXEC;
                vma
            }
        )
    }
    pub fn drop_vma(&mut self,vaddr:Vaddr)->Option<VMA>{
        self.vmas.remove(&vaddr)
    }
    // 只能在页表已经install的时候使用
    pub unsafe fn flush(&self){
        self.pagetable.flush_self();
    }
    pub unsafe fn install_pagetable(&self){
        self.pagetable.install();
    }
    pub fn new_from_elf(elf_bytes:&[u8],file_inode:Arc<Inode>) ->(Self, Vec<AuxHeader>, usize){
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
                let mut vma_flags = VmFlags::VM_USER;
                if ph_flags.is_read() {
                    vma_flags|=VmFlags::VM_READ;
                }
                if ph_flags.is_write() {
                    vma_flags|=VmFlags::VM_WRITE;
                }
                if ph_flags.is_execute() {
                    vma_flags|=VmFlags::VM_EXEC;
                }
                let mut vma_end = Vaddr(0);
                match mm.__alloc_unmapped_core(Some(s_addr),size_aligned,true,Vaddr(USER_SPACE_START),Vaddr(USER_SPACE_END)){
                    None => {
                        panic!("alloc vma fail");
                    }
                    Some(mut vma) => {
                        // todo file in vma限制在PAGE_SIZE内
                        vma.file_in_vma_off = offset;
                        debug_assert!(offset<PAGE_SIZE);
                        vma.file = Some(file_inode.clone());
                        vma.vm_flags = vma_flags;
                        vma.pagetable = Some(mm.pagetable.clone());
                        vma.file_len = ph.file_size() as usize;
                        vma.file_off = ph.offset() as usize;
                        vma_end = vma.get_end_vaddr();
                        mm._insert_no_check(vma);
                    }
                }
                // let mut area = mm.get_unmapped_area_alloc(size_aligned, vma_flags, Some(s_addr)).unwrap();
                //
                // unsafe { area.write_off(&elf.input[ph.offset() as usize..(ph.offset() + ph.file_size()) as usize] ,offset);}
                // load_end = max(load_end,area.get_end_vaddr());
                load_end = max(load_end,vma_end);
            }
        }

        // heap
        let heap_start = load_end.ceil()+ PAGE_SIZE;
        mm.start_brk = heap_start;
        mm.brk = heap_start + USER_HEAP_VMA_INIT_NR_PAGES *PAGE_SIZE;
        match mm.__alloc_unmapped_core(Some(mm.start_brk),USER_HEAP_VMA_INIT_NR_PAGES*PAGE_SIZE,true,
                                       Vaddr(USER_SPACE_START),Vaddr(USER_SPACE_END)){
            None => {
                panic!("user heap alloc fail");
            }
            Some(mut v) => {
                v.vm_flags = VmFlags::VM_READ|VmFlags::VM_WRITE|VmFlags::VM_EXEC|VmFlags::VM_USER|VmFlags::VM_ANON;
                v.pagetable = Some(mm.pagetable.clone());
                // not alloc anon pages
                // for i in 0..USER_STACK_SIZE_NR_PAGES{
                //     v._do_alloc_one_page(stack_top+i*PAGE_SIZE);
                // }
                mm._insert_no_check(v);
            }
        }

        //
        // mm.get_unmapped_area(USER_HEAP_SIZE_NR_PAGES*PAGE_SIZE,
        //                      VMAFlags::VM_READ.bits()|VMAFlags::VM_WRITE.bits()|VMAFlags::VM_USER.bits(),
        //                      Some(heap_start)
        // ).unwrap();

        // let v_ = mm.pagetable.get_kvaddr_by_uvaddr(Vaddr(0x277B0)).unwrap();
        // let vv:usize  = unsafe{v_.read_single()}.unwrap();
        // println!("{:#X}",vv);
        // shutdown();
        // mm.alloc_phy_pages_check(heap_start,4,
        //                          |x,y| {}
        // );

        // mm.get_unmapped_area_alloc(
        //     USER_STACK_SIZE_NR_PAGES*PAGE_SIZE,
        //     VMAFlags::VM_READ.bits()|VMAFlags::VM_WRITE.bits()|VMAFlags::VM_USER.bits(),
        //     Some(Vaddr(USER_STACK_MAX_ADDR-(USER_STACK_SIZE_NR_PAGES*PAGE_SIZE))),
        // ).unwrap();
        let stack_top = Vaddr::from(USER_STACK_MAX_ADDR - USER_STACK_SIZE_NR_PAGES*PAGE_SIZE);
        match mm.__alloc_unmapped_core(Some(stack_top),USER_STACK_SIZE_NR_PAGES*PAGE_SIZE,true,
                                       Vaddr(USER_SPACE_START),Vaddr(USER_SPACE_END)){
            None => {
                panic!("user stack alloc fail");
            }
            Some(mut v) => {
                v.vm_flags = VmFlags::VM_READ|VmFlags::VM_WRITE|VmFlags::VM_EXEC|VmFlags::VM_USER|VmFlags::VM_ANON;
                v.pagetable = Some(mm.pagetable.clone());
                // alloc all anon pages
                for i in 0..USER_STACK_SIZE_NR_PAGES{
                    v._do_alloc_one_page(stack_top+i*PAGE_SIZE);
                }
                mm._insert_no_check(v);
            }
        }
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