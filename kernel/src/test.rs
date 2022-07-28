use alloc::string::String;
use alloc::vec;
use fatfs::{FsOptions, IntoStorage, Read, Seek, SeekFrom, Write};
use riscv::register::sstatus::Sstatus;
use xmas_elf::ElfFile;
use crate::io::virtio::VirtioDev;
use crate::mm::addr::Addr;
use crate::mm::{alloc_pages, get_kernel_pagetable};
use crate::{info_sync, println};
use crate::asm::{enable_irq, r_sstatus, SSTATUS_SIE};
use crate::fs::fat::get_fatfs;
use crate::io::BlockRead;
use crate::io::sdcard::{new_sdcard, SDCardDev};
use crate::mm::mm::MmStruct;
use crate::sbi::shutdown;

pub unsafe fn do_test(){
    let fs_g = get_fatfs();
    let fs= fs_g.lock().unwrap();
    // for t in f.root_dir().iter() {
    //     let ff = t.unwrap();
    //     println!("file : {}, {}",ff.file_name(),ff.len());
    // }
    let mut f=  fs.root_dir().open_file("a.out").unwrap();
    let ptr = alloc_pages(3).unwrap().get_pfn().get_addr_usize() as *mut [u8;32768];
    let read_buf = &mut *ptr;
    let mut cnt: usize = 0;
    loop {
        let read = f.read(&mut read_buf[cnt..]).unwrap();
        cnt += read;
        if read == 0 {
            break;
        }
    }
    println!("read len : {}", cnt);
    let (a,b) = MmStruct::new_from_elf(&read_buf[..cnt]);
    shutdown();
}