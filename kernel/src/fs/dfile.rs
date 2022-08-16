use alloc::string::{String, ToString};
use alloc::sync::Arc;
use core::cmp::min;
use fatfs::{Date, DateTime, DefaultTimeProvider, Dir, File, FileAttributes, LossyOemCpConverter, Read, SeekFrom, Time, Write};
use crate::{print, println, SpinLock};
use crate::fs::dfile::DFILE_TYPE::*;
use crate::fs::{DirAlias, FileAlias, get_dentry_from_dir};
use crate::fs::fat::{BlkStorage, get_fatfs};
use crate::fs::fcntl::OpenFlags;
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
    pub inner:SpinLock<OldDFMutInner>
}

impl OldDFile {
    pub fn new_io(dftype:DFILE_TYPE) ->Self{
        Self{
            inner: SpinLock::new(OldDFMutInner::new(dftype, String::new()))
        }
    }
    pub fn new_file(path:String)->Self{
        Self{
            inner: SpinLock::new(OldDFMutInner::new(DFTYPE_FILE, path))
        }
    }
    pub fn new_dir(path:String)->Self{
        Self{
            inner: SpinLock::new(OldDFMutInner::new(DFTYPE_DIR, path))
        }
    }
    pub fn new_pipe()->Self{
        Self{
            inner: SpinLock::new(OldDFMutInner::new(DFTYPE_PIPE, String::new()))
        }
    }
}

pub struct OldDFMutInner {
    dftype:DFILE_TYPE,
    path:String,
    pos:usize,
}

impl OldDFMutInner {
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
                print!("{}",String::from_utf8_lossy(buf));
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
                    print!("{}",String::from_utf8_lossy(&buf[i..(i+real_write)]));
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
    ClassInode(Arc<Inode>),
    ClassTerminal(Terminal),
}

pub struct DFileMutInner{
    class:DFileClass,
    pos:usize,
    open_flags:OpenFlags,
}

pub struct DFile {
    inner:SpinLock<DFileMutInner>
}

impl DFileMutInner {
    pub fn readable(&self)->bool{
        self.open_flags.readable()
    }
    pub fn writeable(&self)->bool{
        self.open_flags.writeable()
    }
    pub fn clone_inode(&self)->Option<Arc<Inode>>{
        match &self.class{
            DFileClass::ClassInode(i) => {
                Some(i.clone())
            }
            _=>{
                None
            }
        }
    }
    pub fn read(&mut self,buf:&mut [u8])->Result<usize,()>{
        if !self.readable(){
            return Err(());
        }
        match &mut self.class {
            DFileClass::ClassInode(inode) => {
                if inode.is_file(){
                    inode.read_off(buf,self.pos).map(|x|{
                        self.pos+=x;
                        x
                    })
                } else {
                    panic!("can`t read dir");
                }
            }
            DFileClass::ClassTerminal(t) => {
                Ok(t.read(buf))
            }
            _ => {
                todo!()
            }
        }
    }
    pub fn write(&mut self,buf:&[u8])->Result<usize,()> {
        if !self.writeable(){
            return Err(());
        }
        match &mut self.class {
            DFileClass::ClassInode(inode) => {
                if inode.is_file(){
                    inode.write_off(buf,self.pos).map(|x|{
                        self.pos+=x;
                        x
                    })
                } else {
                    panic!("can`t write dir");
                }
            }
            DFileClass::ClassTerminal(t) => {
                Ok(t.write(buf))
            }
            _=>{
                todo!()
            }
        }
    }
    pub fn read_all(&mut self,buf:&mut [u8])->Result<usize,usize>{
        let mut buf_pos = 0;
        while buf_pos<buf.len() {
            let real_read = match self.read(&mut buf[buf_pos..]) {
                Ok(v) => {
                    if v==0{
                        return Err(buf_pos);
                    }
                    v
                }
                Err(_) => {
                    return Err(buf_pos);
                }
            };
            buf_pos+=real_read;
        }
        return Ok(buf_pos);
    }
    pub fn write_all(&mut self,buf:&[u8])->Result<usize,usize>{
        let mut buf_pos = 0;
        while buf_pos<buf.len() {
            let real_write = match self.write(&buf[buf_pos..]) {
                Ok(v) => {
                    if v==0{
                        return Err(buf_pos);
                    }
                    v
                }
                Err(_) => {
                    return Err(buf_pos);
                }
            };
            buf_pos+= real_write;
        }
        return Ok(buf_pos);
    }
    pub fn seek(&mut self,pos:SeekFrom)->Result<usize,()>{
        match &self.class{
            DFileClass::ClassInode(inode) => {
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
            DFileClass::ClassTerminal(_) => {
                Ok(0)
            }
        }
    }
}

lazy_static!{
    static ref root_dfile:Arc<DFile> = Arc::new(DFile {
            inner: SpinLock::new(DFileMutInner {
                class: DFileClass::ClassInode(Inode::get_root()),
                pos: 0,
                open_flags: OpenFlags::O_RDONLY
            })
        });
}

// Terminal类型不需要OpenFlags
impl DFile {
    pub fn clone_inode(&self)->Option<Arc<Inode>>{
        self.inner.lock_irq().unwrap().clone_inode()
    }
    pub fn new_stdin() -> Self {
        Self {
            inner: SpinLock::new(DFileMutInner {
                class: DFileClass::ClassTerminal(Terminal {
                    ttype: TerminalType::STDIN
                }),
                pos: 0,
                open_flags: OpenFlags::O_RDONLY
            })
        }
    }
    pub fn new_stdout() -> Self {
        Self {
            inner: SpinLock::new(DFileMutInner {
                class: DFileClass::ClassTerminal(Terminal {
                    ttype: TerminalType::STDOUT
                }),
                pos: 0,
                open_flags: OpenFlags::O_WRONLY
            })
        }
    }
    pub fn new_stderr() -> Self {
        Self {
            inner: SpinLock::new(DFileMutInner {
                class: DFileClass::ClassTerminal(Terminal {
                    ttype: TerminalType::STDERR
                }),
                pos: 0,
                open_flags: OpenFlags::O_WRONLY
            })
        }
    }
    pub fn get_root() -> Arc<Self> {
        root_dfile.clone()
    }
    pub fn from_inode(inode: Arc<Inode>, open_flags: OpenFlags) -> Self {
        Self {
            inner: SpinLock::new(DFileMutInner {
                class: DFileClass::ClassInode(inode),
                pos: 0,
                open_flags
            })
        }
    }
    pub fn open_name(&self, name: &str, open_flags: OpenFlags) -> Option<Self> {
        match &self.inner.lock_irq().unwrap().class {
            DFileClass::ClassInode(inode) => {
                inode.get_sub_node(name).map(|x| {
                    Self {
                        inner: SpinLock::new(
                            DFileMutInner {
                                class: DFileClass::ClassInode(inode.clone()),
                                pos: 0,
                                open_flags
                            }
                        )
                    }
                })
            }
            DFileClass::ClassTerminal(_) => {
                None
            }
        }
    }
    pub fn open_path(&self, path: &str, open_flags: OpenFlags) -> Option<Self> {
        match &self.inner.lock_irq().unwrap().class {
            DFileClass::ClassInode(inode) => {
                inode.get_node_by_path(path).map(|x| {
                    Self {
                        inner: SpinLock::new(
                            DFileMutInner {
                                class: DFileClass::ClassInode(x.clone()),
                                pos: 0,
                                open_flags
                            }
                        )
                    }
                })
            }
            DFileClass::ClassTerminal(_) => {
                None
            }
        }
    }
    pub fn read(&self,buf:&mut [u8])->Result<usize,()>{
        self.inner.lock_irq().unwrap().read(buf)
    }
    pub fn write(&self,buf:&[u8])->Result<usize,()>{
        self.inner.lock_irq().unwrap().write(buf)
    }
    pub fn read_all(&self,buf:&mut [u8])->Result<usize,usize>{
        self.inner.lock_irq().unwrap().read_all(buf)
    }
    pub fn write_all(&self,buf:&[u8])->Result<usize,usize>{
        self.inner.lock_irq().unwrap().write_all(buf)
    }
    pub fn seek(&self,pos:SeekFrom)->Result<usize,()> {
        self.inner.lock_irq().unwrap().seek(pos)
    }
}