use alloc::boxed::Box;
use alloc::sync::Arc;
use core::cell::RefCell;
use core::cmp::{max, min};
use fatfs::{DefaultTimeProvider, FileSystem, FsOptions, IntoStorage, IoBase, LossyOemCpConverter, SeekFrom};
use crate::{debug_sync, SpinLock, trace_sync};
use crate::debug;
use crate::fs::{FatDev, FatFs};
use crate::io::{ BlockReadWrite};
use crate::io::sdcard::SDCardDev;
use crate::io::virtio::VirtioDev;

#[cfg(feature = "qemu")]
lazy_static!{
    static ref GLOBALFATFS:GlobalFatfs<VirtioDev> = GlobalFatfs{
        inner: Arc::new(SpinLock::new(fatfs::FileSystem::new(VirtioDev::new(),FsOptions::new()).unwrap()))
    };
}

#[cfg(feature = "k210")]
lazy_static!{
    static ref GLOBALFATFS:GlobalFatfs<SDCardDev> = GlobalFatfs{
        inner: Arc::new(SpinLock::new(fatfs::FileSystem::new(SDCardDev::new(),FsOptions::new()).unwrap()))
    };
}

struct GlobalFatfs<T:BlockReadWrite>{
    inner : Arc<SpinLock<FileSystem<BlkStorage<T>,DefaultTimeProvider,LossyOemCpConverter>>>
}

unsafe impl<T:BlockReadWrite> Sync for GlobalFatfs<T> {}

impl<T:BlockReadWrite> GlobalFatfs<T>{
    fn get_fs(&self)->Arc<SpinLock<FileSystem<BlkStorage<T>,DefaultTimeProvider,LossyOemCpConverter>>>{
        self.inner.clone()
    }
}

#[cfg(feature = "qemu")]
pub fn get_fatfs()->Arc<SpinLock<FileSystem<BlkStorage<VirtioDev>,DefaultTimeProvider,LossyOemCpConverter>>>{
    GLOBALFATFS.get_fs()
}

#[cfg(feature = "k210")]
pub fn get_fatfs()->Arc<SpinLock<FileSystem<BlkStorage<SDCardDev>,DefaultTimeProvider,LossyOemCpConverter>>>{
    GLOBALFATFS.get_fs()
}

pub fn fat_init(){

}

impl IntoStorage<BlkStorage<VirtioDev>> for VirtioDev {
    fn into_storage(self) -> BlkStorage<VirtioDev> {
        BlkStorage{
            blk_dev: self,
            pos:0
        }
    }
}

impl IntoStorage<BlkStorage<SDCardDev>> for SDCardDev {
    fn into_storage(self) -> BlkStorage<SDCardDev> {
        BlkStorage{
            blk_dev: self,
            pos:0
        }
    }
}

pub struct BlkStorage<T:BlockReadWrite> {
    blk_dev: T,
    pos:u64
}

impl<T:BlockReadWrite> BlkStorage<T> {

}

impl<T:BlockReadWrite> IoBase for BlkStorage<T> { type Error = (); }

impl<T:BlockReadWrite> fatfs::Read for BlkStorage<T>{
    fn read(&mut self, buf: &mut [u8]) -> Result<usize, Self::Error> {
        let mut pos = self.pos as usize;
        let read_max = pos+buf.len();
        let mut read_pos:usize = 0;
        while pos < read_max {
            let b_no = (pos/512) as usize;
            let b_off = (pos%512) as usize;
            let mut _read_buf = [0 as u8;512];
            self.blk_dev.read_block(b_no,&mut _read_buf);
            let read_res = read_max-pos;
            let real_read_len = min(read_res,512-b_off);
            buf[read_pos..read_pos+real_read_len].copy_from_slice(&_read_buf[b_off..b_off+real_read_len]);
            pos+=real_read_len;
            read_pos+=real_read_len;
        }
        self.pos += (buf.len()) as u64;
        Ok(buf.len())
    }
}

impl<T:BlockReadWrite> fatfs::Write for BlkStorage<T> {
    fn write(&mut self, buf: &[u8]) -> Result<usize, Self::Error> {
        let mut pos = self.pos as usize;
        let write_max = pos+buf.len();
        let mut write_pos:usize = 0;
        debug_sync!("write block {}, off {}, len {}",(pos/512),(pos%512),buf.len());
        while pos < write_max {
            let b_no = (pos/512) as usize;
            let b_off = (pos%512) as usize;

            let mut _read_buf = [0 as u8;512];
            self.blk_dev.read_block(b_no,&mut _read_buf);
            let write_res = write_max -pos;
            let mut real_write_len = min(write_res, 512-b_off);
            _read_buf[b_off..b_off+real_write_len].copy_from_slice(
                &buf[write_pos..write_pos+real_write_len]
            );
            //write back
            self.blk_dev.write_block(b_no,& _read_buf);
            pos+= real_write_len;
            write_pos += real_write_len;
        }
        self.pos += (buf.len()) as u64;
        Ok(buf.len())
    }

    fn flush(&mut self) -> Result<(), Self::Error> {
        Ok(())
    }
}

impl<T:BlockReadWrite> fatfs::Seek for BlkStorage<T> {
    fn seek(&mut self, pos: SeekFrom) -> Result<u64, Self::Error> {
        match pos {
            SeekFrom::Start(off) => {
                self.pos = off;
            }
            SeekFrom::End(off) => {
                panic!("BlkStorage can`t get end");
            }
            SeekFrom::Current(off) => {
                trace_sync!("seek cur {}",off);
                self.pos = self.pos.wrapping_add_signed(off);
            }
        }
        Ok(self.pos)
    }
}

pub fn new_fat_fs()->FatFs{
    fatfs::FileSystem::new(FatDev::new(),FsOptions::new()).unwrap()
}