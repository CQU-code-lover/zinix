use alloc::string::String;
use alloc::sync::Arc;
use fatfs::{Date, DateTime, DefaultTimeProvider, Dir, File, FileAttributes, LossyOemCpConverter, Read, SeekFrom, Time};
use crate::{println, SpinLock};
use crate::fs::dfile::DFILE_TYPE::*;
use crate::fs::{DirAlias, FileAlias, get_dentry_from_dir};
use crate::fs::fat::{BlkStorage, get_fatfs};
use crate::io::virtio::VirtioDev;
use crate::task::task::get_running;

lazy_static!{
    static ref STDIN:Arc<DFile> = Arc::new(DFile::new_io(DFTYPE_STDIN));
    static ref STDOUT:Arc<DFile> = Arc::new(DFile::new_io(DFTYPE_STDOUT));
}

pub fn get_stdout()->Arc<DFile>{
    STDOUT.clone()
}

pub fn get_stdin()->Arc<DFile>{
    STDIN.clone()
}

pub fn get_stderr()->Arc<DFile>{
    STDOUT.clone()
}

pub enum DFILE_TYPE{
    DFTYPE_STDIN,
    DFTYPE_STDOUT,
    DFTYPE_FILE,
    DFTYPE_DIR,
    DFTYPE_PIPE
}

pub struct DirEntryWrapper<'a> {
    pub dir: Option<DirAlias<'a>>,
    pub file: Option<FileAlias<'a>>,
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
    pub fn to_dir(self)->DirAlias<'a>{
        self.dir.unwrap()
    }
    pub fn to_file(self)->FileAlias<'a>{
        self.file.unwrap()
    }
}

pub struct DFile{
    pub inner:SpinLock<DFMUTInner>
}

impl DFile {
    pub fn new_io(dftype:DFILE_TYPE) ->Self{
        Self{
            inner: SpinLock::new(DFMUTInner::new(dftype,String::new()))
        }
    }
    pub fn new_file(path:String)->Self{
        Self{
            inner: SpinLock::new(DFMUTInner::new(DFTYPE_FILE,path))
        }
    }
    pub fn new_dir(path:String)->Self{
        Self{
            inner: SpinLock::new(DFMUTInner::new(DFTYPE_DIR,path))
        }
    }
    pub fn new_pipe()->Self{
        Self{
            inner: SpinLock::new(DFMUTInner::new(DFTYPE_PIPE,String::new()))
        }
    }
}

pub struct DFMUTInner{
    dftype:DFILE_TYPE,
    path:String,
    pos:usize,
}

impl DFMUTInner {
    pub fn new(dftype:DFILE_TYPE,path:String)->Self{
        Self{
            dftype,
            path,
            pos:0
        }
    }
    pub fn write(&mut self,buf:&[u8])->usize{
        match self.dftype {
            DFILE_TYPE::DFTYPE_STDIN => {0}
            DFILE_TYPE::DFTYPE_STDOUT => {
                println!("{}",String::from_utf8_lossy(buf));
                buf.len()
            }
            DFILE_TYPE::DFTYPE_FILE => {0}
            _ => {
                todo!()
            }
        }
    }
    pub fn read(&mut self, buf: &mut [u8]) ->usize{
        match self.dftype {
            DFILE_TYPE::DFTYPE_STDIN => {
                todo!();
                0
            }
            DFILE_TYPE::DFTYPE_STDOUT => {0}
            DFILE_TYPE::DFTYPE_FILE => {
                let fs = get_fatfs();
                let lock = fs.lock_irq().unwrap();
                let running = get_running();
                let tsk = running.lock_irq().unwrap();
                let wrapper = get_dentry_from_dir(lock.root_dir(),tsk.pwd_ref());
                match wrapper {
                    None => {
                        return 0;
                    }
                    Some(v) => {
                        let s= v.to_file().read(buf).unwrap();
                        self.pos += s;
                        return s;
                    }
                }
            }
            _ => {

            }
        }
    }
    pub fn seek(seek:SeekFrom){

    }
}