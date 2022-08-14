use alloc::string::{String, ToString};
use alloc::vec;
use fatfs::{FsOptions, IntoStorage, Read, Seek, SeekFrom, Write};
use riscv::register::sstatus::Sstatus;
use xmas_elf::ElfFile;
use crate::io::virtio::{virtio_test, VirtioDev};
use crate::mm::addr::OldAddr;
use crate::mm::{alloc_pages, get_kernel_pagetable, mm_test};
use crate::{info_sync, println, Task};
use crate::asm::{enable_irq, r_sstatus, SSTATUS_SIE};
use crate::fs::dfile::DFileClass::ClassInode;
use crate::fs::fat::get_fatfs;
use crate::fs::inode::Inode;
use crate::io::BlockRead;
use crate::io::sdcard::{new_sdcard, SDCardDev};
use crate::mm::mm::MmStruct;
use crate::sbi::shutdown;

pub unsafe fn do_test(){
    // mm_test();
    Task::create_user_task_and_run("m.o",vec![]);
    // virtio_test();
    // shutdown();
    // Task::create_user_task_and_run("entry-static.exe",vec!["statvfs".to_string()]);
    // shutdown();
    // Task::create_user_task_and_run("busybox_unstripped",vec!["yes".to_string()]);
    // Task::create_user_task_and_run("busybox_unstripped",vec!["busybox".to_string(),"cat".to_string(),"2.txt".to_string()]);
}
