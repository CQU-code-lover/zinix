use core::cmp::min;
use core::ops::{Add, AddAssign, Div, DivAssign, Mul, MulAssign, Rem, RemAssign, Sub, SubAssign};
use fatfs::{IoBase, Read, Seek, Write};

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


// 这个trait的write方法没有mut self标记，所以保证读写同步需要自己手动在impl加锁
pub trait ReadWriteOffUnsafe<T:Copy+Sized>{
    unsafe fn read_off(&self,buf:&mut [T],off:usize)->usize;
    unsafe fn write_off(&self,buf:&[T],off:usize)->usize;
}

const WRITE_BY_READER_BUF_LEN:usize = 10;
pub trait WriteByReader<T:Read>: ReadWriteOffUnsafe<u8>{
    unsafe fn write_by_reader(&mut self,reader:&mut T,off:usize,len:usize)->usize{
        let mut buf = [0u8;WRITE_BY_READER_BUF_LEN];
        let mut len_probe = len;
        loop {
            let read_len = reader.read(&mut buf[..min(WRITE_BY_READER_BUF_LEN,len_probe)]).unwrap();
            if read_len == 0{
                break;
            }
            assert!(len_probe>=read_len);
            self.write_off(&buf[..read_len],len-len_probe);
            len_probe -= read_len;
            if len_probe==0{
                break;
            }
        }
        len-len_probe
    }
}

pub trait IOReadeWriteSeek:Read+Write+Seek{}

pub struct FakeReadSource{
    len:usize,
    cur:usize
}

impl IoBase for FakeReadSource { type Error = (); }

impl Read for FakeReadSource {
    fn read(&mut self, buf: &mut [u8]) -> Result<usize, Self::Error> {
        let real_read = min(buf.len(),self.len-self.cur);
        for i in 0..real_read {
            buf[i] = 0;
        }
        self.cur+=real_read;
        Ok(real_read)
    }
}

pub struct FakeWriteSource{
    len:usize,
    cur:usize
}

impl IoBase for FakeWriteSource { type Error = (); }

impl Write for FakeWriteSource {
    fn write(&mut self, buf: &[u8]) -> Result<usize, Self::Error> {
        let real_write =min(buf.len(),self.len-self.cur);
        self.cur += real_write;
        Ok(real_write)
    }

    fn flush(&mut self) -> Result<(), Self::Error> {
        Ok(())
    }
}

pub trait ShowRdWrEx{
    fn readable(&self)->bool;
    fn writeable(&self)->bool;
    fn execable(&self)->bool;
    fn readwriteable(&self)->bool{
        self.readable()&&self.writeable()
    }
}