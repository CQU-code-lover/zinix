use alloc::vec;
use alloc::vec::Vec;
use core::fmt::{Debug, Formatter};

use log::info;

use crate::{info_sync, println};

pub struct Bitmap {
    inner : Vec<u64>,
    _len: usize
}

impl Bitmap {
    pub fn default()-> Bitmap {
        return Bitmap {
            inner: vec![],
            _len:0
        }
    }
    pub fn init(&mut self,len:usize){
        let mut cnt = len/64;
        if len%64 != 0{
            cnt +=1;
        }
        self.inner.resize_with(cnt ,|| {
            0
        });
        self._len = len;
    }
    pub fn new(len:usize)-> Bitmap {
        let mut s = Self::default();
        s.init(len);
        return s;
    }
    fn _expand_cap_to_bytes(&mut self, new_bytes_len:usize){
        self.inner.resize_with(new_bytes_len, || {
            0
        });
    }
    fn _expand_cap_double(&mut self){
        let new_len = self.inner.len() *2;
        self._expand_cap_to_bytes(new_len);
    }
    pub fn expand_cap_for(&mut self, target_pos:usize){
        while target_pos>=self.cap() {
            self._expand_cap_double();
        }
    }
    pub fn auto_expand_cap(&mut self, pos:usize) {
        if pos>= self.cap() {
            self.expand_cap_for(pos);
        }
    }
    pub fn cap(&self)->usize{
        return self.inner.len()*64;
    }
    pub fn len(&self)->usize{
        return self._len;
    }
    fn _adjust_len(&mut self,pos : usize){
        if pos>self.len() {
            self._len = pos;
        }
    }
    pub fn set(&mut self, pos:usize){
        let pos1 = pos/64;
        let pos2 = pos%64;
        self.auto_expand_cap(pos);
        let mut bits = self.inner[pos1];
        let mask = (1 as u64) <<pos2;
        if bits&mask == 0 {
            bits+=mask;
            self.inner[pos1] = bits;
        }
        self._adjust_len(pos);
    }
    pub fn clear(&mut self,pos :usize){
        let pos1 = pos/64;
        let pos2 = pos%64;
        self._adjust_len(pos);
        if pos >= self.cap(){
            self.expand_cap_for(pos);
            return;
        }
        let mut bits = self.inner[pos1];
        let mask = !((1 as u64) <<pos2);
        bits = bits & mask;
        self.inner[pos1] = bits;
    }
    pub fn get(&mut self, pos:usize)->bool{
        let pos1 = pos/64;
        let pos2 = pos%64;
        self._adjust_len(pos);
        if pos<self.cap(){
            let bits = self.inner[pos1];
            let mask = (1 as u64)<<pos2;
            if bits&mask != 0 {
                true
            } else {
                false
            }
        } else {
            self.expand_cap_for(pos);
            false
        }
    }
    pub fn turn_over(&mut self, pos:usize){
        self._adjust_len(pos);
        if self.get(pos){
            self.clear(pos);
        } else {
            self.set(pos);
        }
    }
}

impl Debug for Bitmap {
    fn fmt(&self, f: &mut Formatter<'_>) -> core::fmt::Result {
        write!(f,"[Bitmap Debug]\n");
        write!(f,"addr: {:p}\n",self);
        write!(f,"len:{}\tcap:{}\n",self.len(),self.cap());
        // write!(f,"index  value\n");
        for i in 0..self.inner.len() {
            write!(f,"{:X} : {:b}\n",i*64,self.inner[i]);
        }
        write!(f,"[Bitmap Debug End]\n");
        core::fmt::Result::Ok(())
    }
}

pub fn bitmap_test(){
    let mut b = Bitmap::new(256);
    let j =b.get(1);
    b.set(1);
    let k = b.get(1);
    b.clear(1);
    let m = b.get(1);
    b.turn_over(1);
    b.turn_over(2);
    b.turn_over(1);
    assert_eq!(j,false);
    assert_eq!(k,true);
    assert_eq!(m,false);
    info_sync!("\n{:?}",b);
    info_sync!("bitmap test OK!");
}