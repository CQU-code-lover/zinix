use alloc::string::String;
use alloc::sync::Arc;
use fatfs::{Date, DateTime, DefaultTimeProvider, Dir, File, FileAttributes, LossyOemCpConverter, SeekFrom, Time};
use crate::{println, SpinLock};
use crate::fs::dfile::DFILE_TYPE::*;
use crate::fs::fat::BlkStorage;
use crate::io::virtio::VirtioDev;

lazy_static!{
    static ref STDIN:Arc<DFile> = Arc::new(DFile::new(DFTYPE_STDIN));
    static ref STDOUT:Arc<DFile> = Arc::new(DFile::new(DFTYPE_STDOUT));
}

pub fn get_stdout()->Arc<DFile>{
    STDOUT.clone()
}

pub fn get_stdin()->Arc<DFile>{
    STDIN.clone()
}

pub enum DFILE_TYPE{
    DFTYPE_STDIN,
    DFTYPE_STDOUT,
    DFTYPE_FILE
}

pub struct DirEntryWrapper<'a> {
    pub dir: Option<Dir<'a,BlkStorage<VirtioDev>, DefaultTimeProvider, LossyOemCpConverter>>,
    pub file: Option<File<'a,BlkStorage<VirtioDev>, DefaultTimeProvider, LossyOemCpConverter>>,
    pub attributes:FileAttributes,
    pub accessd:Date,
    pub created:DateTime,
    pub modified:DateTime,
    pub len:usize
}

impl<'a> Default for DirEntryWrapper<'a> {
    fn default() -> Self {
        DirEntryWrapper{
            dir: None,
            file: None,
            attributes: Default::default(),
            accessd: Date::new(1980,1,1),
            created: DateTime::new(Date::new(1980,1,1),Time::new(0,0,0,0)),
            modified: DateTime::new(Date::new(1980,1,1),Time::new(0,0,0,0)),
            len:0
        }
    }
}

impl<'a> DirEntryWrapper<'a> {
    pub fn is_dir(&self)->bool{
        self.dir.is_some()
    }
    pub fn is_file(&self)->bool{
        self.file.is_some()
    }
    pub fn to_dir(self)->Dir<'a,BlkStorage<VirtioDev>, DefaultTimeProvider, LossyOemCpConverter>{
        self.dir.unwrap()
    }
    pub fn to_file(self)->File<'a,BlkStorage<VirtioDev>, DefaultTimeProvider, LossyOemCpConverter>{
        self.file.unwrap()
    }
}

pub struct DFile{
    pub inner:SpinLock<DFMUTInner>
}

impl DFile {
    pub fn new(dftype:DFILE_TYPE)->Self{
        Self{
            inner: SpinLock::new(DFMUTInner::new(dftype))
        }
    }
}

pub struct DFMUTInner{
    dftype:DFILE_TYPE,
}



impl DFMUTInner {
    pub fn new(dftype:DFILE_TYPE)->Self{
        Self{
            dftype
        }
    }
    pub fn write(&mut self,buf:&[u8])->usize{
        match self.dftype {
            DFILE_TYPE::DFTYPE_STDIN => {0}
            DFILE_TYPE::DFTYPE_STDOUT => {
                println!("{:?}",String::from_utf8_lossy(buf));
                buf.len()
            }
            DFILE_TYPE::DFTYPE_FILE => {0}
        }
    }
    pub fn read(&mut self,buf:&[u8])->usize{
        match self.dftype {
            DFILE_TYPE::DFTYPE_STDIN => {
                todo!();
                0
            }
            DFILE_TYPE::DFTYPE_STDOUT => {0}
            DFILE_TYPE::DFTYPE_FILE => {0}
        }
    }
    pub fn seek(seek:SeekFrom){

    }
}