use alloc::string::{String, ToString};
use alloc::sync::Arc;
use core::cmp::min;
use fatfs::{Date, DateTime, DefaultTimeProvider, Dir, File, FileAttributes, LossyOemCpConverter, Read, SeekFrom, Time, Write};
use crate::{println, SpinLock};
use crate::fs::dfile::DFILE_TYPE::*;
use crate::fs::{DirAlias, FileAlias, get_dentry_from_dir};
use crate::fs::fat::{BlkStorage, get_fatfs};
use crate::fs::inode::{Inode};
use crate::io::virtio::VirtioDev;
use crate::task::task::get_running;

lazy_static!{
    static ref STDIN:Arc<OldDFile> = Arc::new(OldDFile::new_io(DFTYPE_STDIN));
    static ref STDOUT:Arc<OldDFile> = Arc::new(OldDFile::new_io(DFTYPE_STDOUT));
}

pub fn get_stdout()->Arc<OldDFile>{
    STDOUT.clone()
}

pub fn get_stdin()->Arc<OldDFile>{
    STDIN.clone()
}

pub fn get_stderr()->Arc<OldDFile>{
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

pub struct OldDFile {
    pub inner:SpinLock<DFMUTInner>
}

impl OldDFile {
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
                        let s= v.to_file().write(buf).unwrap();
                        self.pos += s;
                        return s;
                    }
                }
            }
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
                panic!("Dftype not found");
            }
        }
    }
    pub fn seek(&mut self,seek:SeekFrom){

    }
}

pub enum TerminalType{
    STDIN,
    STDOUT,
    STDERR
}

const TERMINAL_INTER_BUF_LEN:usize = 10;
pub struct Terminal{
    ttype:TerminalType
}

impl Terminal {
    pub fn read(&mut self,buf:&[u8])->usize{
        match self.ttype {
            TerminalType::STDIN => {
                0
            }
            _ => {
                0
            }
        }
    }
    pub fn write(&mut self,buf:&[u8])->usize{
        match self.ttype {
            TerminalType::STDOUT|TerminalType::STDERR => {
                let buf_len = buf.len();
                let mut i = 0;
                while i<buf_len{
                    let real_write = min(TERMINAL_INTER_BUF_LEN,buf_len-i);
                    println!("{}",String::from_utf8_lossy(&buf[i..(i+real_write)]));
                    i+=real_write;
                }
                buf_len
            }
            _ => {
                0
            }
        }
    }
    pub fn seek(&mut self,pos:SeekFrom)->usize{
        todo!()
    }
}

pub enum DFileClass{
    Inode(Arc<Inode>),
    Terminal(Terminal),
}

pub struct DFile {
    class:DFileClass,
    pos:usize
}

impl DFile {
    pub fn new_stdin()->Self{
        Self{
            class: DFileClass::Terminal(Terminal{
                ttype: TerminalType::STDIN
            }),
            pos: 0
        }
    }
    pub fn new_stdout()->Self{
        Self{
            class: DFileClass::Terminal(Terminal{
                ttype: TerminalType::STDOUT
            }),
            pos: 0
        }
    }
    pub fn new_stderr()->Self{
        Self{
            class: DFileClass::Terminal(Terminal{
                ttype: TerminalType::STDERR
            }),
            pos: 0
        }
    }
    pub fn open_root()->Self{
        DFile {
            class: DFileClass::Inode(Inode::get_root()),
            pos: 0
        }
    }
    pub fn from_inode(inode:Arc<Inode>)->Self{
        Self{
            class: DFileClass::Inode(inode),
            pos: 0
        }
    }
    pub fn open_name(&self,name:&str)->Option<Self>{
        match &self.class {
            DFileClass::Inode(inode) => {
                inode.get_sub_node(name).map(|x| {
                    Self {
                        class: DFileClass::Inode(inode.clone()),
                        pos: 0,
                    }
                })
            }
            DFileClass::Terminal(_) => {
                None
            }
        }
    }
    pub fn open_path(&self,path:&str)->Option<Self>{
        match &self.class {
            DFileClass::Inode(inode) => {
                inode.get_node_by_path(path).map(|x| {
                    Self {
                        class: DFileClass::Inode(inode.clone()),
                        pos: 0,
                    }
                })
            }
            DFileClass::Terminal(_) => {
                None
            }
        }
    }
    pub fn read(&mut self,buf:&mut [u8])->Result<usize,()>{
        match &mut self.class {
            DFileClass::Inode(inode) => {
                if inode.is_file(){
                    inode.read_off(buf,self.pos).map(|x|{
                        self.pos+=x;
                        x
                    })
                } else {
                    panic!("can`t read dir");
                }
            }
            DFileClass::Terminal(t) => {
                Ok(t.read(buf))
            }
            _ => {
                todo!()
            }
        }
    }
    pub fn write(&mut self,buf:&mut [u8])->Result<usize,()> {
        match &mut self.class {
            DFileClass::Inode(inode) => {
                if inode.is_file(){
                    inode.write_off(buf,self.pos).map(|x|{
                        self.pos+=x;
                        x
                    })
                } else {
                    panic!("can`t write dir");
                }
            }
            DFileClass::Terminal(t) => {
                Ok(t.write(buf))
            }
            _=>{
                todo!()
            }
        }
    }
    pub fn seek(&mut self,pos:SeekFrom)->Result<usize,()>{
        match &self.class{
            DFileClass::Inode(inode) => {
                if inode.is_file(){
                    let max_pos = inode.get_dentry().len() as usize;
                    match pos{
                        SeekFrom::Start(v) => {
                            if v<=(max_pos as u64){
                                self.pos = v as usize;
                                Ok(self.pos)
                            }else{
                                Err(())
                            }
                        }
                        SeekFrom::End(v) => {
                            match max_pos.checked_add_signed(v as isize){
                                None => {
                                    // overflow
                                    Err(())
                                }
                                Some(s) => {
                                    if s>max_pos{
                                        Err(())
                                    } else {
                                        self.pos = s;
                                        Ok(s as usize)
                                    }
                                }
                            }
                        }
                        SeekFrom::Current(v) => {
                            match self.pos.checked_add_signed(v as isize){
                                None => {
                                    // overflow
                                    Err(())
                                }
                                Some(s) => {
                                    if s>max_pos || s<0 {
                                        Err(())
                                    } else {
                                        self.pos = s;
                                        Ok(s as usize)
                                    }
                                }
                            }
                        }
                    }
                } else {
                    panic!("can`t seek dir");
                }
            }
            DFileClass::Terminal(_) => {
                Ok(0)
            }
        }
    }
}