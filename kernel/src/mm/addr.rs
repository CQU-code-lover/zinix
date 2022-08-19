use core::fmt;
use core::fmt::{Debug, Formatter, UpperHex};
use core::iter::Step;
use core::mem::size_of;
use core::ops::{Add, AddAssign, Deref, DerefMut, Div, DivAssign, Mul, MulAssign, Rem, RemAssign, Sub, SubAssign};
use core::ptr::addr_of;
use fatfs::{IoBase, Read, Seek, SeekFrom, Write};
use crate::{info, println};
use k210_pac::generic::Variant::Val;

use crate::consts::{PAGE_OFFSET, PAGE_SIZE, PHY_MEM_OFFSET};
use crate::pre::{ReadWriteSingleNoOff, InnerAccess, IOReadeWriteSeek, OperatorSet};

pub struct AddrIterator<T>{
    step: usize,
    cur: T,
    end: T
}

impl<T:Addr<usize>+From<usize>+PartialOrd+Ord> Iterator for AddrIterator<T> {
    type Item = T;

    fn next(&mut self) -> Option<Self::Item> {
        if self.cur<self.end {
            let v = Some(self.cur.clone());
            self.cur.set_inner(self.cur.get_inner() + self.step);
            v
        } else {
            None
        }
    }
}

pub trait PageAlign {
    fn get_page_align(&self)->usize{
        PAGE_SIZE
    }
    fn floor(&self)->Self;
    fn ceil(&self)->Self;
    fn is_align(&self)->bool;
    fn align(&mut self);
    fn is_align_n(&self,n:usize)->bool;
}

pub trait Addr<T>: InnerAccess<T> + OperatorSet<usize> + DerefMut + Deref + PageAlign + Step{
    fn unwrap(self)->T {
        self.get_inner()
    }
}

#[repr(C)]
#[derive(Copy, Clone, Ord, PartialOrd, Eq, PartialEq, Default)]
pub struct Paddr(pub usize);

impl UpperHex for Paddr {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        write!(f,"{:#X}",self.0);
        Ok(())
    }
}

impl From<usize> for Paddr {
    fn from(val: usize) -> Self {
        Self(val)
    }
}

impl InnerAccess<usize> for Paddr {
    fn get_inner(&self) -> usize {
        self.0
    }

    fn set_inner(&mut self, val: usize) {
        self.0 = val;
    }
}

impl OperatorSet<usize> for Paddr {}

impl Add<usize> for Paddr {
    type Output = Self;

    fn add(self, rhs: usize) -> Self::Output {
        Self(self.0 + rhs)
    }
}

impl AddAssign<usize> for Paddr {
    fn add_assign(&mut self, rhs: usize) {
        self.0 = self.0 + rhs
    }
}

impl Sub<usize> for Paddr {
    type Output = Self;

    fn sub(self, rhs: usize) -> Self::Output {
        Self(self.0 - rhs)
    }
}

impl SubAssign<usize> for Paddr {
    fn sub_assign(&mut self, rhs: usize) {
        self.0 = self.0 - rhs
    }
}

impl Mul<usize> for Paddr {
    type Output = Self;

    fn mul(self, rhs: usize) -> Self::Output {
        Self(self.0 * rhs)
    }
}

impl MulAssign<usize> for Paddr {
    fn mul_assign(&mut self, rhs: usize) {
        self.0 = self.0 * rhs
    }
}

impl Div<usize> for Paddr {
    type Output = Self;

    fn div(self, rhs: usize) -> Self::Output {
        Self(self.0 / rhs)
    }
}

impl DivAssign<usize> for Paddr {
    fn div_assign(&mut self, rhs: usize) {
        self.0 = self.0 / rhs
    }
}

impl Rem<usize> for Paddr {
    type Output = Self;

    fn rem(self, rhs: usize) -> Self::Output {
        Self(self.0 % rhs)
    }
}

impl RemAssign<usize> for Paddr {
    fn rem_assign(&mut self, rhs: usize) {
        self.0 = self.0 % rhs
    }
}

impl DerefMut for Paddr {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.0
    }
}

impl Deref for Paddr {
    type Target = usize;

    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

impl PageAlign for Paddr {
    fn floor(&self)->Self{
        let align = self.get_page_align();
        let m = self.add(align);
        self.clone()/align*align
    }
    fn ceil(&self)->Self{
        let align = self.get_page_align();
        if self.is_align(){
            self.floor()
        } else {
            self.floor() + align
        }
    }
    fn is_align(&self)->bool{
        self.get_inner()%self.get_page_align() == 0
    }
    fn align(&mut self) {
        self.0 = self.floor().get_inner();
    }
    fn is_align_n(&self, n: usize)->bool {
        self.get_inner()%n == 0
    }
}

impl Step for Paddr {
    fn steps_between(start: &Self, end: &Self) -> Option<usize> {
        if start<=end {
            Some(end.0-start.0)
        } else {
            None
        }
    }

    fn forward_checked(start: Self, count: usize) -> Option<Self> {
        match start.0.checked_add(count) {
            None => {
                None
            }
            Some(v) => {
                Some(Paddr(v))
            }
        }
    }

    fn backward_checked(start: Self, count: usize) -> Option<Self> {
        match start.0.checked_sub(count) {
            None => {
                None
            }
            Some(v) => {
                Some(Paddr(v))
            }
        }
    }
}

impl Addr<usize> for Paddr{}

impl Into<Vaddr> for Paddr {
    fn into(self) -> Vaddr {
        Vaddr(self.0+PHY_MEM_OFFSET)
    }
}

impl Debug for Paddr {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        f.write_fmt(format_args!("[Paddr:{:#X}]", self.0))
    }
}

impl Paddr {
    pub fn addr_iter(&self,len: usize,step:usize)->AddrIterator<Self>{
        AddrIterator{
            step,
            cur: self.clone(),
            end: self.clone()+len
        }
    }
    pub fn page_addr_iter(&self,len:usize)->AddrIterator<Self>{
        self.addr_iter(len,self.get_page_align())
    }
}

#[repr(C)]
#[derive(Copy, Clone, Ord, PartialOrd, Eq, PartialEq, Default)]
pub struct Vaddr(pub usize);

impl UpperHex for Vaddr {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        write!(f,"{:#X}",self.0);
        Ok(())
    }
}

impl InnerAccess<usize> for Vaddr {
    fn get_inner(&self) -> usize {
        self.0
    }

    fn set_inner(&mut self, val: usize) {
        self.0 = val;
    }
}

impl From<usize> for Vaddr {
    fn from(val: usize) -> Self {
        Self(val)
    }
}

impl OperatorSet<usize> for Vaddr {}

impl Add<usize> for Vaddr {
    type Output = Self;

    fn add(self, rhs: usize) -> Self::Output {
        Self(self.0 + rhs)
        // Self(match self.0.checked_add(rhs){
        //     None => {
        //         panic!("pp");
        //     }
        //     Some(s) => {
        //         s
        //     }
        // })
    }
}

impl AddAssign<usize> for Vaddr {
    fn add_assign(&mut self, rhs: usize) {
        self.0 = self.0 + rhs
    }
}

impl Sub<usize> for Vaddr {
    type Output = Self;

    fn sub(self, rhs: usize) -> Self::Output {
        Self(self.0 - rhs)
    }
}

impl SubAssign<usize> for Vaddr {
    fn sub_assign(&mut self, rhs: usize) {
        self.0 = self.0 - rhs
    }
}

impl Mul<usize> for Vaddr {
    type Output = Self;

    fn mul(self, rhs: usize) -> Self::Output {
        Self(self.0 * rhs)
    }
}

impl MulAssign<usize> for Vaddr {
    fn mul_assign(&mut self, rhs: usize) {
        self.0 = self.0 * rhs
    }
}

impl Div<usize> for Vaddr {
    type Output = Self;

    fn div(self, rhs: usize) -> Self::Output {
        Self(self.0 / rhs)
    }
}

impl DivAssign<usize> for Vaddr {
    fn div_assign(&mut self, rhs: usize) {
        self.0 = self.0 / rhs
    }
}

impl Rem<usize> for Vaddr {
    type Output = Self;

    fn rem(self, rhs: usize) -> Self::Output {
        Self(self.0 % rhs)
    }
}

impl RemAssign<usize> for Vaddr {
    fn rem_assign(&mut self, rhs: usize) {
        self.0 = self.0 % rhs
    }
}

impl DerefMut for Vaddr {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.0
    }
}

impl Deref for Vaddr {
    type Target = usize;

    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

impl PageAlign for Vaddr {
    fn floor(&self)->Self{
        let align = self.get_page_align();
        let m = self.add(align);
        self.clone()/align*align
    }
    fn ceil(&self)->Self{
        let align = self.get_page_align();
        if self.is_align(){
            self.floor()
        } else {
            self.floor() + align
        }
    }
    fn is_align(&self)->bool{
        self.get_inner()%self.get_page_align() == 0
    }
    fn align(&mut self) {
        self.0 = self.floor().get_inner();
    }

    fn is_align_n(&self, n: usize)->bool {
        self.get_inner()%n == 0
    }
}

impl Step for Vaddr {
    fn steps_between(start: &Self, end: &Self) -> Option<usize> {
        if start<=end {
            Some(end.0-start.0)
        } else {
            None
        }
    }

    fn forward_checked(start: Self, count: usize) -> Option<Self> {
        match start.0.checked_add(count) {
            None => {
                None
            }
            Some(v) => {
                Some(Vaddr(v))
            }
        }
    }

    fn backward_checked(start: Self, count: usize) -> Option<Self> {
        match start.0.checked_sub(count) {
            None => {
                None
            }
            Some(v) => {
                Some(Vaddr(v))
            }
        }
    }
}

impl Addr<usize> for Vaddr {}

impl Into<Paddr> for Vaddr {
    fn into(self) -> Paddr {
        Paddr(self.0-PHY_MEM_OFFSET)
    }
}

impl Debug for Vaddr {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        f.write_fmt(format_args!("[Vaddr:{:#X}]", self.0))
    }
}

impl Vaddr {
    pub fn addr_iter(&self,len: usize,step:usize)->AddrIterator<Self>{
        AddrIterator{
            step,
            cur: self.clone(),
            end: self.clone()+len
        }
    }
    pub fn page_addr_iter(&self,len:usize)->AddrIterator<Self>{
        self.addr_iter(len,self.get_page_align())
    }

}

impl<T:Copy> ReadWriteSingleNoOff<T> for Vaddr{
    unsafe fn write_single(&mut self, val: T)->Option<()> {
        assert!(self.is_align_n(size_of::<T>()));
        (self.0 as *mut T).write_volatile(val);
        Some(())
    }

    unsafe fn read_single(&self) -> Option<T> {
        assert!(self.is_align_n(size_of::<T>()));
        let ret = (self.0 as *const T).read_volatile();
        Some(ret)
    }
}

impl IoBase for Vaddr {
    type Error = ();
}

impl Read for Vaddr {
    fn read(&mut self, buf: &mut [u8]) -> Result<usize, Self::Error> {
        let read_len = buf.len();
        let mut buf_pos = 0;
        for i in self.addr_iter(read_len,1) {
            unsafe { buf[buf_pos] = i.read_single().unwrap(); }
            buf_pos+=1;
        }
        *self += read_len;
        Ok(read_len)
    }
}

impl Write for Vaddr {
    fn write(&mut self, buf: &[u8]) -> Result<usize, Self::Error> {
        let write_len = buf.len();
        let mut buf_pos = 0;
        for mut i in self.addr_iter(write_len, 1) {
            unsafe { i.write_single(buf[buf_pos]).unwrap(); }
            buf_pos += 1;
        }
        *self += write_len;
        Ok(write_len)
    }

    fn flush(&mut self) -> Result<(), Self::Error> {
        Ok(())
    }
}

impl Seek for Vaddr {
    fn seek(&mut self, pos: SeekFrom) -> Result<u64, Self::Error> {
        match pos {
            SeekFrom::Start(v) => {
                panic!("Addr Seek Start Not Allowed");
            }
            SeekFrom::End(v) => {
                panic!("Addr Seek End Not Allowed");
            }
            SeekFrom::Current(v) => {
                if v>0{
                    *self+= (v as usize);
                } else {
                    *self-= ((-v) as usize);
                }
                return Ok(self.get_inner() as u64);
            }
        }
        Err(())
    }
}

// Read Write Seek for Vaddr
impl IOReadeWriteSeek for Vaddr {

}

// unit test
pub fn addr_test(){
    let paddr = Paddr::from(0x1000);
    assert_eq!(paddr,Paddr(0x1000));
    let vaddr = Vaddr::from(0x1000+PHY_MEM_OFFSET);
    assert_eq!(vaddr,Vaddr(0x1000+PHY_MEM_OFFSET));
    assert!(paddr.is_align());
    assert!(!((paddr+2).is_align()));
    assert!((paddr+2).is_align_n(2));
    assert!(!((paddr+2).is_align_n(10)));
    assert_eq!({let n:Vaddr = paddr.into();n},vaddr);
    assert_eq!({let n:Paddr = vaddr.into();n},paddr);
    assert_eq!({let mut n = Paddr(0x1024);n.align();n},Paddr(0x1000));
    assert_eq!({let mut n = Vaddr(0x1024);n.align();n},Vaddr(0x1000));
    assert_eq!(Paddr(0x1001).floor(),Paddr(0x1000));
    assert_eq!(Paddr(0x1001).ceil(),Paddr(0x2000));
    assert_eq!(Paddr(0x1000).floor(),Paddr(0x1000));
    assert_eq!(Paddr(0x1000).ceil(),Paddr(0x1000));
    assert_eq!(Vaddr(0x1001).floor(),Vaddr(0x1000));
    assert_eq!(Vaddr(0x1001).ceil(),Vaddr(0x2000));
    assert_eq!(Vaddr(0x1000).floor(),Vaddr(0x1000));
    assert_eq!(Vaddr(0x1000).ceil(),Vaddr(0x1000));
    assert_eq!({let mut v = Vaddr(0x1003);v-=3;v},Vaddr(0x1000));
    assert_eq!({let mut v = Paddr(0x1003);v-=3;v},Paddr(0x1000));
    assert_eq!({let mut v = Vaddr(0x1010);v+=1;v},Vaddr(0x1011));
    assert_eq!({let mut v = Paddr(0x1010);v+=1;v},Paddr(0x1011));
    assert_eq!({let mut v = Vaddr(0x1010);v/=4096;v},Vaddr(0x1));
    assert_eq!({let mut v = Paddr(0x1010);v/=4096;v},Paddr(0x1));
    assert_eq!({let mut v = Vaddr(0x2000);v/=4096;v},Vaddr(0x2));
    assert_eq!({let mut v = Paddr(0x2000);v/=4096;v},Paddr(0x2));
    assert_eq!({let mut v = Vaddr(0x1010);v%=4096;v},Vaddr(0x10));
    assert_eq!({let mut v = Paddr(0x1010);v%=4096;v},Paddr(0x10));
    assert_eq!({let mut v = Vaddr(0x2000);v%=4096;v},Vaddr(0x0));
    assert_eq!({let mut v = Paddr(0x2000);v%=4096;v},Paddr(0x0));
    info!("Addr Test OK!");
    let va = Vaddr(0x1002);
    let mut cnt:usize = 0;
    for i in va.addr_iter(10,2){
        assert_eq!(i,va+2*cnt);
        cnt += 1;
    }
    assert_eq!(cnt,5);

    let pa = Paddr(0x1000);
    cnt = 0;
    for i in pa.page_addr_iter(pa.get_page_align()*20){
        assert_eq!(i,pa+pa.get_page_align()*cnt);
        cnt += 1;
    }
    assert_eq!(cnt,20);
}

#[repr(C)]
#[derive(Copy, Clone, Ord, PartialOrd, Eq, PartialEq)]
pub struct OldAddr(pub usize);

impl Default for OldAddr {
    fn default() -> Self {
        OldAddr(0)
    }
}

impl Step for OldAddr {
    fn steps_between(start: &Self, end: &Self) -> Option<usize> {
        if start<=end {
            Some(end.0-start.0)
        } else {
            None
        }
    }

    fn forward_checked(start: Self, count: usize) -> Option<Self> {
        match start.0.checked_add(count) {
            None => {
                None
            }
            Some(v) => {
                Some(OldAddr(v))
            }
        }
    }

    fn backward_checked(start: Self, count: usize) -> Option<Self> {
        match start.0.checked_sub(count) {
            None => {
                None
            }
            Some(v) => {
                Some(OldAddr(v))
            }
        }
    }
}

// PFN
#[repr(C)]
#[derive(Copy, Clone, Ord, PartialOrd, Eq, PartialEq)]
pub struct PFN(pub usize);

impl Debug for OldAddr {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        f.write_fmt(format_args!("ADDR:{:#x}", self.0))
    }
}

impl Add for OldAddr {
    type Output = OldAddr;

    fn add(self, rhs: Self) -> Self::Output {
        return  OldAddr(self.0+rhs.0);
    }
}

impl Sub for OldAddr {
    type Output = OldAddr;

    fn sub(self, rhs: Self) -> Self::Output {
        return  OldAddr(self.0-rhs.0);
    }
}

impl AddAssign for OldAddr {
    fn add_assign(&mut self, rhs: Self) {
        self.0 += rhs.0
    }
}

impl SubAssign for OldAddr {
    fn sub_assign(&mut self, rhs: Self) {
        self.0 -= rhs.0
    }
}

impl Debug for PFN {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        f.write_fmt(format_args!("PFN:{:#x}", self.0))
    }
}

impl Add for PFN{
    type Output = PFN;

    fn add(self, rhs: Self) -> Self::Output {
        return  PFN(self.0+rhs.0);
    }
}

impl Sub for PFN{
    type Output = PFN;

    fn sub(self, rhs: Self) -> Self::Output {
        return  PFN(self.0-rhs.0);
    }
}

impl From<usize> for OldAddr {
    fn from(v: usize) -> Self { Self(v) }
}

impl From<PFN> for OldAddr {
    fn from(pfn: PFN) -> Self {
        Self(pfn.get_addr_usize())
    }
}

impl From<usize> for PFN {
    fn from(v: usize) -> Self { Self(v>>PAGE_OFFSET) }
}

impl From<OldAddr> for PFN {
    fn from(v: OldAddr) -> Self { Self(v.0>>PAGE_OFFSET) }
}

pub struct AddrPageIter{
    end: OldAddr,
    cur: OldAddr
}

impl Iterator for AddrPageIter{
    type Item = OldAddr;

    fn next(&mut self) -> Option<Self::Item> {
        let ret = if self.cur<self.end{
            Some(self.cur)
        } else {
            None
        };
        self.cur+= OldAddr(PAGE_SIZE);
        ret
    }
}

impl OldAddr {
    pub fn floor(&self)-> OldAddr {
        OldAddr::from((self.0/PAGE_SIZE)*PAGE_SIZE)
    }
    pub fn ceil(&self)-> OldAddr {
        OldAddr::from(
            if self.0%PAGE_SIZE == 0{
                self.floor().0
            } else {
                self.floor().0 + PAGE_SIZE
            }
        )
    }
    pub fn is_align(&self)->bool{
        self.0%PAGE_SIZE == 0
    }
    pub fn get_pg_cnt(&self)->usize{
        return self.0/PAGE_SIZE;
    }
    pub fn get_paddr(&self)->usize {
        self.0 - PHY_MEM_OFFSET
    }
    pub fn get_vaddr(&self)->usize {
        self.0 + PHY_MEM_OFFSET
    }
    pub fn page_iter(&self,len:usize)->AddrPageIter {
        assert!(self.is_align());
        AddrPageIter{ end: *self + OldAddr(len), cur: *self }
    }
}

impl PFN {
    pub fn step_n(&mut self,n:usize)->Self{
        self.0+=n;
        *self
    }
    pub fn step_one(&mut self)->Self{
        self.0+=1;
        *self
    }
    pub fn get_addr_usize(&self)->usize{
        self.0<<PAGE_OFFSET
    }
}