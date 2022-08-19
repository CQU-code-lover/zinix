use alloc::string::{String, ToString};
use alloc::sync::Arc;
use core::cmp::min;
use fatfs::{Date, DateTime, DefaultTimeProvider, Dir, File, FileAttributes, LossyOemCpConverter, Read, SeekFrom, Time, Write};
use crate::{print, println, SpinLock};
use crate::consts::DIRECT_MAP_START;
use crate::fs::dfile::DFILE_TYPE::*;
use crate::fs::{DirAlias, FileAlias, get_dentry_from_dir};
use crate::fs::dfile::DFileClass::ClassPipe;
use crate::fs::fat::{BlkStorage, get_fatfs};
use crate::fs::fcntl::OpenFlags;
use crate::fs::inode::{Inode};
use crate::fs::pipe::Pipe;
use crate::io::virtio::VirtioDev;
use crate::task::info::{NewStat, S_IFDIR, S_IFREG, S_IRWXG, S_IRWXO, S_IRWXU};
use crate::task::task::get_running;
use crate::utils::{date2second, datetime2second};

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

#[derive(Clone)]
pub enum TerminalType{
    STDIN,
    STDOUT,
    STDERR
}

const TERMINAL_INTER_BUF_LEN:usize = 10;
#[derive(Clone)]
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
    ClassPipe(Arc<Pipe>),
}

pub struct DFileMutInner{
    class:DFileClass,
    pos:usize,
    open_flags:OpenFlags,
    cloexec:bool
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
            DFileClass::ClassPipe(p) => {
                p.read_exact(buf)
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
            DFileClass::ClassPipe(p)=>{
                p.write_exact(buf)
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
            DFileClass::ClassPipe(_) =>{
                // not support seek pipe
                todo!()
            }
        }
    }
}

lazy_static!{
    static ref root_dfile:Arc<DFile> = Arc::new(DFile {
            inner: SpinLock::new(DFileMutInner {
                class: DFileClass::ClassInode(Inode::get_root()),
                pos: 0,
                open_flags: OpenFlags::O_RDONLY,
                cloexec:false
            })
        });
}

// Terminal类型不需要OpenFlags
impl DFile {
    pub fn clone_inode(&self)->Option<Arc<Inode>>{
        self.inner.lock_irq().unwrap().clone_inode()
    }
    // return (read_end,write_end)
    pub fn new_pipe()->(Self,Self){
        let p = Arc::new(Pipe::new());
        p.inc_write();
        let read = Self{
            inner: SpinLock::new(DFileMutInner{
                class: ClassPipe(p.clone()),
                pos: 0,
                open_flags: OpenFlags::O_RDONLY,
                cloexec: false
            })
        };
        let write = Self{
            inner: SpinLock::new(DFileMutInner{
                class: ClassPipe(p.clone()),
                pos: 0,
                open_flags: OpenFlags::O_WRONLY,
                cloexec: false
            })
        };
        (read,write)
    }

    pub fn new_stdin() -> Self {
        Self {
            inner: SpinLock::new(DFileMutInner {
                class: DFileClass::ClassTerminal(Terminal {
                    ttype: TerminalType::STDIN
                }),
                pos: 0,
                open_flags: OpenFlags::O_RDONLY,
                cloexec: false
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
                open_flags: OpenFlags::O_WRONLY,
                cloexec: false
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
                open_flags: OpenFlags::O_WRONLY,
                cloexec: false
            })
        }
    }
    pub fn get_cloexec(&self)->bool{
        self.inner.lock_irq().unwrap().cloexec
    }
    pub fn set_cloexec_to(&self,flag:bool){
        self.inner.lock_irq().unwrap().cloexec = flag
    }
    pub fn is_root_inode(&self)->bool{
        match &self.inner.lock_irq().unwrap().class{
            DFileClass::ClassInode(v) => {
                match v.get_parent(){
                    None => {
                        false
                    }
                    Some(_) => {
                        true
                    }
                }
            }
            _ => {
                false
            }
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
                open_flags,
                cloexec: false
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
                                open_flags,
                                cloexec: false
                            }
                        )
                    }
                })
            }
            DFileClass::ClassTerminal(_) => {
                None
            }
            _ => {
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
                                open_flags,
                                cloexec: false
                            }
                        )
                    }
                })
            }
            DFileClass::ClassTerminal(_) => {
                None
            }
            _ => {
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
    pub fn fill_stat(&self,stat: &mut NewStat)->Result<(),()>{
        match &self.inner.lock_irq().unwrap().class {
            DFileClass::ClassInode(inode) => {
                if inode.get_parent().is_some() {
                    let dentry = inode.get_dentry();
                    let mode = if dentry.is_dir() {
                        S_IFDIR | S_IRWXU | S_IRWXG | S_IRWXO
                    } else {
                        S_IFREG | S_IRWXU | S_IRWXG | S_IRWXO
                    };
                    // s_ino是 inode的指针地址减去偏移
                    stat.fill_info(0,
                                   (inode.as_ref() as *const Inode as usize - DIRECT_MAP_START) as u64,
                                   mode,
                                   1,
                                   dentry.len() as i64,
                                   date2second(dentry.accessed()) as i64,
                                   datetime2second(dentry.modified()) as i64,
                                   datetime2second(dentry.created()) as i64);
                } else {
                    let mode = S_IFDIR | S_IRWXU | S_IRWXG | S_IRWXO;
                    // s_ino是 inode的指针地址减去偏移
                    stat.fill_info(0,
                                   1,
                                   mode,
                                   1,
                                   0,
                                   1000,
                                   1000,
                                   1000
                    )
                }
            }
            _ => {
                return Err(());
            }
        }
        Ok(())
    }
    pub fn readable(&self)->bool{
        self.inner.lock_irq().unwrap().readable()
    }
    pub fn writeable(&self)->bool{
        self.inner.lock_irq().unwrap().writeable()
    }
    pub fn deep_clone(&self)->Self{
        let inner = self.inner.lock_irq().unwrap();
        let new_class =  match &inner.class {
            DFileClass::ClassInode(inode) => {
                DFileClass::ClassInode(inode.clone())
            }
            DFileClass::ClassTerminal(terminal) => {
                DFileClass::ClassTerminal(terminal.clone())
            }
            ClassPipe(pipe) => {
                if inner.writeable(){
                    pipe.inc_write();
                }
                DFileClass::ClassPipe(pipe.clone())
            }
        };
        Self{
            inner: SpinLock::new(DFileMutInner{
                class: new_class,
                pos: inner.pos,
                open_flags: inner.open_flags,
                cloexec: inner.cloexec
            })
        }
    }
}

impl Drop for DFile {
    fn drop(&mut self) {
        //主要针对pipe
        let inner = self.inner.lock_irq().unwrap();
        match &inner.class {
            DFileClass::ClassPipe(p) => {
                if inner.writeable() {
                    p.dec_write();
                }
            }
            _=>{}
        }
    }
}