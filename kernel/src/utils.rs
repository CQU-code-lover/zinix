use core::ptr::{addr_of, addr_of_mut};
use crate::consts::{DIRECT_MAP_START, PAGE_OFFSET, PAGE_SIZE};

pub fn addr_page_align_upper(addr:usize) ->usize{
    let mut ret = addr-(addr&PAGE_SIZE);
    if (ret&PAGE_SIZE) != 0 {
        ret+=PAGE_SIZE;
    }
    return ret;
}

pub fn addr_page_align_lower(addr:usize)->usize{
    return addr-(addr&PAGE_SIZE);
}

pub fn vaddr2paddr(addr:usize)->usize{
    return addr-DIRECT_MAP_START;
}

pub fn paddr2vaddr(addr:usize)->usize{
    return addr+DIRECT_MAP_START;
}

pub fn addr_get_ppn0(vaddr:usize)->usize{
    (vaddr>>12)&0x1FF
}

pub fn addr_get_ppn1(vaddr:usize)->usize{
    (vaddr>>21)&0x1FF
}

pub fn addr_get_ppn2(vaddr:usize)->usize{
    (vaddr>>30)&0x1FF
}

pub unsafe  fn get_usize_by_addr(vaddr:usize)->usize{
    let ptr = vaddr as *mut usize;
    ptr.read_volatile()
}

pub unsafe fn set_usize_by_addr(vaddr:usize,val:usize){
    let ptr = vaddr as *mut usize;
    ptr.write_volatile(val);
}

pub unsafe fn memcpy(dest:usize,src: usize,len:usize){
    for i in 0..len{
        *((dest+i) as *mut u8) = *((dest+i) as *mut u8);
    }
}