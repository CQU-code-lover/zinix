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