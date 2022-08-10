use alloc::borrow::ToOwned;
use alloc::string::String;
use alloc::vec::Vec;
use fatfs::{Date, DateTime, DefaultTimeProvider, Dir, DirEntry, File, LossyOemCpConverter, Time};
use crate::fs::dfile::DirEntryWrapper;
use crate::fs::fat::{BlkStorage, fat_init, get_fatfs};
use crate::io::virtio::VirtioDev;
use crate::{info_sync, println};
use crate::io::sdcard::SDCardDev;

pub mod fat;
pub mod inode;
pub mod superblock;
pub mod dfile;
pub mod fcntl;

pub fn init_fs(){
    fat_init();
}

#[cfg(feature = "qemu")]
pub type DirAlias<'a> = Dir<'a,BlkStorage<VirtioDev>, DefaultTimeProvider, LossyOemCpConverter>;
#[cfg(feature = "k210")]
pub type DirAlias<'a> = Dir<'a,BlkStorage<SDCardDev>, DefaultTimeProvider, LossyOemCpConverter>;
#[cfg(feature = "qemu")]
pub type FileAlias<'a> = File<'a,BlkStorage<VirtioDev>, DefaultTimeProvider, LossyOemCpConverter>;
#[cfg(feature = "k210")]
pub type FileAlias<'a> = File<'a,BlkStorage<SDCardDev>, DefaultTimeProvider, LossyOemCpConverter>;

pub fn get_dentry_from_dir<'a,'b>(in_dir:DirAlias<'a>,path:&'b str)->Option<DirEntryWrapper<'a>>{
    let name_array_pre:Vec<&str> = path.split("/").collect();
    let name_array:Vec<&str> = name_array_pre.into_iter().filter(
        |x| {
            if (*x).is_empty(){ false } else { true }
        }
    ).collect();
    println!("{:?}",name_array);
    let mut wrapper = DirEntryWrapper::default();
    if name_array.is_empty() {
        wrapper.dir = Some(in_dir);
        return Some(wrapper);
    }
    let mut dir_probe = in_dir;
    let mut i = 0;
    let mut last_file:Option<FileAlias> = None;
    if name_array.len()==0 {
        return None;
    }
    for name in name_array.iter(){
        if i==(name_array.len()-1) {
            let mut find_flag = false;
            for item in dir_probe.iter() {
                let dir_entry = item.unwrap();
                let find_name = dir_entry.file_name();
                if dir_entry.file_name().eq(&(*name)){
                    // fill dir entry attr
                    info_sync!("file:{}",dir_entry.file_name());
                    wrapper.attributes = dir_entry.attributes();
                    wrapper.len = dir_entry.len() as usize;
                    wrapper.accessd = dir_entry.accessed();
                    wrapper.created = dir_entry.created();
                    wrapper.modified = dir_entry.modified();
                    if dir_entry.is_dir(){
                        dir_probe = dir_entry.to_dir();
                    } else {
                        last_file = Some(dir_entry.to_file());
                    }
                    find_flag = true;
                }
            }
            if !find_flag{
                return None;
            }
        } else {
            let result = dir_probe.open_dir(*name);
            match result {
                Ok(dir) => {
                    dir_probe = dir;
                }
                Err(_) => {
                    return None;
                }
            }
        }
        i+=1;
    }
    match last_file {
        None => {
            wrapper.dir = Some(dir_probe)
        }
        Some(file) => {
            wrapper.file = Some(file)
        }
    }
    Some(wrapper)
}
