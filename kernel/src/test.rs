use alloc::string::{String, ToString};
use alloc::sync::Arc;
use alloc::vec;
use core::arch::asm;
use core::ptr::slice_from_raw_parts;
use fatfs::{FsOptions, IntoStorage, Read, Seek, SeekFrom, Write};
use riscv::register::sstatus::Sstatus;
use xmas_elf::ElfFile;
use crate::io::virtio::{virtio_test, VirtioDev};
use crate::mm::addr::{OldAddr, PageAlign, Vaddr};
use crate::mm::{alloc_pages, get_kernel_pagetable, mm_test};
use crate::{info_sync, println, SpinLock, Task};
use crate::asm::{enable_irq, r_sstatus, SSTATUS_SIE};
use crate::fs::dfile::DFileClass::ClassInode;
use crate::fs::fat::get_fatfs;
use crate::fs::inode::Inode;
use crate::fs::pipe::Pipe;
use crate::io::BlockRead;
use crate::io::sdcard::{new_sdcard, SDCardDev};
use crate::mm::kmap::KmapToken;
use crate::mm::mm::MmStruct;
use crate::pre::InnerAccess;
use crate::sbi::shutdown;

unsafe fn test_kmap(){
    let node = Inode::get_root().get_sub_node("2.txt").unwrap();
    let file_len = node.get_dentry().len() as usize;
    let token = KmapToken::new_file(Vaddr(file_len).ceil().0,node,0,file_len-40).unwrap();
    let vaddr = token.get_vaddr();
    let v = vaddr.get_inner() as *const u8;
    let buf = slice_from_raw_parts(v,file_len);
    println!("{}",String::from_utf8_lossy(&*buf));
    shutdown()
}

struct PipeWrapper(Arc<Pipe>);

impl Clone for PipeWrapper {
    fn clone(&self) -> Self {
        Self(self.0.clone())
    }
}

unsafe impl Sync for PipeWrapper {}

lazy_static!{
    static ref g_pipe:PipeWrapper = PipeWrapper(Arc::new(Pipe::new()));
}

fn reader(){
    info_sync!("start reader!");
    let mut pipe = g_pipe.clone();
    pipe.0.inc_write();
    let mut buf = [0u8;20];
    pipe.0.read_exact(&mut buf);
    println!("{:?}",buf);
    loop {

    }
}

fn writer(){
    info_sync!("start writer!");
    let pipe = g_pipe.clone();
    let buf = [1u8;19];
    pipe.0.write_exact(&buf);
    println!("{:?}",buf);
    pipe.0.dec_write();
    loop {

    }
}

fn test_pipe(){
    let pipe = Pipe::new();
    Task::create_kern_task_and_run(reader);
    Task::create_kern_task_and_run(writer);

    loop {

    }
}

pub unsafe fn do_test(){
    // test_pipe();
    // mm_test();
    // test_kmap();
    // Task::create_user_task_and_run("clone",vec![]);
    // virtio_test();
    // shutdown();
    // Task::create_user_task_and_run("entry-static.exe",vec!["statvfs".to_string()]);
    // shutdown();
    // Task::create_user_task_and_run("busybox_unstripped",vec!["yes".to_string()]);
    // Task::create_user_task_and_run("busybox_unstripped",vec!["busybox".to_string(),"cat".to_string(),"2.txt".to_string()]);
    Task::create_user_task_and_run("busybox_unstripped",vec!["busybox".to_string(),"sh".to_string(),"busybox_testcode.sh".to_string()]);
}
