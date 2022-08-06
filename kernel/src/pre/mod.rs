use core::ops::{Add, AddAssign, Div, DivAssign, Mul, MulAssign, Rem, RemAssign, Sub, SubAssign};
use fatfs::{Read, Seek, Write};

pub trait InnerAccess<T>: Copy+From<T>{
    fn get_inner(&self)->T;
    fn set_inner(&mut self,val:T);
    fn set_and_get_inner(&mut self,val:T)->T{
        let old = self.get_inner();
        self.set_inner(val);
        old
    }
}

pub trait OperatorSet<Rhs>:Add<Rhs> + AddAssign<Rhs> + Sub<Rhs> + SubAssign<Rhs> + Mul<Rhs> + MulAssign<Rhs> + Div<Rhs> + DivAssign<Rhs> + Rem<Rhs> + RemAssign<Rhs>{}

pub trait ReadWriteSingleNoOff<T:Copy>{
    unsafe fn write_single(&mut self, val:T)->Option<()>;
    unsafe fn read_single(&self) ->Option<T>;
}

// 注意write方法使用inmut ref，write与read实现的时候必须加上锁保护
// read加锁保护在write时同时进行了read造成race condition
pub trait ReadWriteSingleOff<T:Copy>{
    unsafe fn write_single_off(&self, val:T, off:usize)->Option<()>;
    unsafe fn read_single_off(&self, off:usize) ->Option<T>;
}

pub trait IOReadeWriteSeek:Read+Write+Seek{}
