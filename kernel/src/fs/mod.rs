use alloc::borrow::ToOwned;
use alloc::string::String;
use alloc::vec::Vec;
use fatfs::{Date, DateTime, DefaultTimeProvider, Dir, DirEntry, File, LossyOemCpConverter, Time};
use crate::fs::dfile::DirEntryWrapper;
use crate::fs::fat::{BlkStorage, fat_init, get_fatfs};
use crate::io::virtio::VirtioDev;
use crate::println;

pub mod fat;
pub mod inode;
pub mod superblock;
pub mod dfile;

pub fn init_fs(){
    fat_init();
}

pub fn get_dentry_from_dir<'a,'b>(in_dir:Dir<'a,BlkStorage<VirtioDev>, DefaultTimeProvider, LossyOemCpConverter>,path:&'b str)->Option<DirEntryWrapper<'a>>{
    let name_array:Vec<&str> = path.split("/").collect();
    println!("{:?}",name_array);
    let mut wrapper = DirEntryWrapper::default();
    let mut dir_probe = in_dir;
    let mut i = 0;
    let mut last_file:Option<File<BlkStorage<VirtioDev>, DefaultTimeProvider, LossyOemCpConverter>> = None;
    if name_array.len()==0 {
        return None;
    }
    for name in name_array.iter(){
        if i==(name_array.len()-1) {
            let mut find_flag = false;
            for item in dir_probe.iter() {
                let dir_entry = item.unwrap();
                wrapper.attributes = dir_entry.attributes();
                wrapper.len = dir_entry.len() as usize;
                wrapper.accessd = dir_entry.accessed();
                wrapper.created = dir_entry.created();
                wrapper.modified = dir_entry.modified();
                let find_name = dir_entry.file_name();
                if dir_entry.file_name().eq(&(*name).to_uppercase()){
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