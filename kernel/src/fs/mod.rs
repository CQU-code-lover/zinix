use crate::fs::fat::fat_init;

pub mod fat;
pub mod inode;
pub mod superblock;
pub mod dfile;

pub fn init_fs(){
    fat_init();
}