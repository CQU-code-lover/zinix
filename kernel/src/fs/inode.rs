use alloc::collections::LinkedList;
use alloc::string::{String, ToString};
use alloc::sync::{Arc, Weak};
use alloc::vec::Vec;
use core::borrow::BorrowMut;
use fatfs::{Error, Read, Seek, SeekFrom, Write};
use crate::fs::{DirAlias, DirEntryAlias, FileAlias, get_dentry_from_dir, get_sub_dentry, get_unsafe_global_fatfs};
use crate::fs::dfile::DirEntryWrapper;
use crate::{info_sync, SpinLock};

lazy_static!{
    static ref root_inode:Arc<Inode> = Inode::_create_root();
}

pub enum InodeClass{
    Dir(DirAlias<'static>),
    File(FileAlias<'static>),
}

// index node
pub struct Inode{
    parent:Option<Arc<Inode>>,
    name:String,
    this:Weak<Inode>,
    inner:SpinLock<InodeMutInner>
}

pub struct InodeMutInner{
    children:LinkedList<Weak<Inode>>,
    class:InodeClass
}

// todo 检查是否同步
unsafe impl Send for InodeMutInner {}

impl InodeMutInner {
    pub fn new_by_file(file:FileAlias<'static>)->Self{
        Self{
            children: LinkedList::new(),
            class: InodeClass::File(file)
        }
    }
    pub fn new_by_dir(dir:DirAlias<'static>) ->Self{
        Self{
            children: LinkedList::new(),
            class: InodeClass::Dir(dir)
        }
    }
}

impl Inode {
    pub fn get_root()->Arc<Inode>{
        let ret = root_inode.clone();
        ret
    }
    fn _create_root()->Arc<Self>{
        let mut s =unsafe {
             Self {
                parent: None,
                name: "".to_string(),
                this: Default::default(),
                inner: SpinLock::new(InodeMutInner::new_by_dir(get_unsafe_global_fatfs().root_dir()))
            }
        };
        let mut arc = Arc::new(s);
        let mut_ptr = arc.as_ref() as *const Inode as *mut Inode;
        unsafe { (*mut_ptr).this = Arc::downgrade(&arc); }
        arc
    }
    // 不检查是否有重合inode 需要调用者管理
    pub fn _new_dir_inode(&self, dir:DirAlias<'static>, name:&str) ->Arc<Self>{
        let self_node = self.this.upgrade().unwrap();
        let mut node = Arc::new(Self{
            parent: Some(self_node),
            name:name.to_string(),
            this: Default::default(),
            inner:SpinLock::new(InodeMutInner::new_by_dir(dir))
        });
        let mut_ptr = node.as_ref() as *const Inode as *mut Inode;
        unsafe { (*mut_ptr).this = Arc::downgrade(&node); }
        node
    }
    pub fn _new_file_inode(&self, file:FileAlias<'static>, name:&str) ->Arc<Self>{
        let self_node = self.this.upgrade().unwrap();
        let mut node = Arc::new(Self{
            parent: Some(self_node),
            name:name.to_string(),
            this: Default::default(),
            inner:SpinLock::new(InodeMutInner::new_by_file(file))
        });
        let mut_ptr = node.as_ref() as *const Inode as *mut Inode;
        unsafe { (*mut_ptr).this = Arc::downgrade(&node); }
        node
    }
    pub fn get_parent(&self)->Option<Arc<Inode>>{
        self.parent.as_ref().map(|v|{v.clone()})
    }
    pub fn get_dentry(&self)->DirEntryAlias{
        match &self.parent.as_ref().unwrap().inner.lock_irq().unwrap().class{
            InodeClass::Dir(dir) => {
                get_sub_dentry(dir,&self.name).unwrap()
            }
            _ => {
                panic!("bug");
            }
        }
    }
    // todo 优化sub node获取方式
    pub fn get_sub_node(&self,name:&str)->Option<Arc<Self>>{
        let mut inner = self.inner.lock_irq().unwrap();
        // check if this node is a dir
        match &inner.class {
            InodeClass::Dir(_) => {}
            _ => {
                return None;
            }
        };
        let mut cursor = inner.children.cursor_front_mut();
        let mut current = cursor.current();
        while current.is_some(){
            match current.unwrap().upgrade(){
                None => {
                    // 原对象已经回收 delete这个节点
                    cursor.remove_current();
                }
                Some(v) => {
                    if v.name.eq(name) {
                        // find target
                        return Some(v.clone());
                    }
                }
            }
            cursor.move_next();
            current = cursor.current();
        }
        // not find in children
        match get_sub_dentry(match &inner.class{
            InodeClass::Dir(v) => {v}
            _ => {
                panic!("bug");
            }
        }, name){
            None => {
                None
            }
            Some(s) => {
                if s.is_dir(){
                    let new_node = self._new_dir_inode(s.to_dir(), name);
                    inner.children.push_back(Arc::downgrade(&new_node));
                    Some(new_node)
                } else if s.is_file(){
                    let new_node = self._new_file_inode(s.to_file(), name);
                    inner.children.push_back(Arc::downgrade(&new_node));
                    Some(new_node)
                } else {
                    None
                }
            }
        }
    }
    // read write seek 有mutinner的spinlock保护
    // 所以只需要 imut即可
    // 从start开始的off读写，可以被锁保护
    pub fn read_off(&self,buf: &mut [u8],off:usize)->Result<usize,()>{
        let mut lock = self.inner.lock_irq().unwrap();
        match &mut lock.class {
            InodeClass::File(f) => {
                match f.seek(SeekFrom::Start(off as u64)) {
                    Ok(_) => {}
                    Err(_) => {
                        return Err(());
                    }
                }
                let len = match f.read(buf){
                    Ok(l) => {
                        l
                    }
                    Err(_) => {
                        return Err(());
                    }
                };
                Ok(len)
            }
            _ => {
                todo!()
            }
        }
    }
    pub fn write_off(&self,buf: &[u8],off:usize)->Result<usize,()>{
        let mut lock = self.inner.lock_irq().unwrap();
        match &mut lock.class {
            InodeClass::File(f) => {
                match f.seek(SeekFrom::Start(off as u64)) {
                    Ok(_) => {}
                    Err(_) => {
                        return Err(());
                    }
                }
                let len = match f.write(buf){
                    Ok(l) => {
                        l
                    }
                    Err(_) => {
                        return Err(());
                    }
                };
                Ok(len)
            }
            _ => {
                todo!()
            }
        }
    }
    pub fn read_off_exact(&self,buf: &mut [u8],off:usize)->Result<usize,()>{
        let need_len = buf.len();
        let mut buf_pos:usize = 0;
        while buf_pos<need_len {
            match self.read_off(&mut buf[buf_pos..],off+buf_pos) {
                Ok(len) => {
                    if len==0{
                        //无法继续读
                        return Ok(buf_pos);
                    }
                    buf_pos+=len;
                }
                Err(_) => {
                    return Err(());
                }
            }
        }
        Ok(need_len)
    }
    pub fn write_off_exact(&self,buf: &[u8],off:usize)->Result<usize,()>{
        Err(())
    }

    pub fn get_self(&self)->Arc<Self>{
        self.this.upgrade().unwrap()
    }
    pub fn get_node_by_path(&self,path:&str)->Option<Arc<Self>>{
        let name_array_pre:Vec<&str> = path.split("/").collect();
        let name_array:Vec<&str> = name_array_pre.into_iter().filter(
            |x| {
                if (*x).is_empty()||(*x).eq("."){
                    false
                } else {
                    true
                }
            }
        ).collect();
        if name_array.is_empty(){
            // get the path node
            return Some(self.get_self());
        }
        let mut node_probe = self.get_self();
        for name in name_array {
            match  node_probe.get_sub_node(name){
                Some(n) => {
                    node_probe = n;
                }
                None => {
                    return None;
                }
            }
        }
        Some(node_probe)
    }
    pub fn is_file(&self)->bool {
        match &self.inner.lock_irq().unwrap().class{
            InodeClass::File(_) => {true}
            _=> {false}
        }
    }
    pub fn is_dir(&self)->bool{
        match &self.inner.lock_irq().unwrap().class{
            InodeClass::Dir(_) => {true}
            _=> {false}
        }
    }
}