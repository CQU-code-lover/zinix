mod virtio;

use alloc::string::String;
use alloc::vec::Vec;

pub struct IOBytes<T>{
    inner:T
}

pub enum IOErr {
    Err //default
}

fn default_read_to_end<T:Read+?Sized>(reader:&mut T,buf:&mut Vec<u8>)->Result<usize, IOErr>{
    let mut local_buf = [0 as u8;10];
    let mut cnt = 0;
    loop {
        match reader.read(&mut local_buf){
            Ok(len) => {
                cnt+=len;
                buf.append(&mut local_buf[0..len].to_vec());
                if len!=10{
                    break;
                }
            }
            Err(e) => {
                return Err(e);
            }
        }
    }
    Ok(cnt)
}

pub trait Read {

    fn read(&mut self, buf: &mut [u8]) -> Result<usize, IOErr>;

    fn read_to_end(&mut self, buf: &mut Vec<u8>) -> Result<usize, IOErr> {
        default_read_to_end(self, buf)
    }

    // fn read_to_string(&mut self, buf: &mut String) -> Result<usize, IOErr> {
    //     default_read_to_string(self, buf)
    // }
    //
    // fn read_exact(&mut self, buf: &mut [u8]) -> Result<(),IOErr> {
    //     default_read_exact(self, buf)
    // }
    //
    // fn by_ref(&mut self) -> &mut Self
    //     where
    //         Self: Sized,
    // {
    //     self
    // }
    //
    // fn bytes(self) -> IOBytes<Self>
    //     where
    //         Self: Sized,
    // {
    //     IOBytes { inner: self }
    // }
    //
    // /// Creates an adapter which will chain this stream with another.
    // ///
    // /// The returned `Read` instance will first read all bytes from this object
    // /// until EOF is encountered. Afterwards the output is equivalent to the
    // /// output of `next`.
    // ///
    // /// # Examples
    // ///
    // /// [`File`]s implement `Read`:
    // ///
    // /// [`File`]: crate::fs::File
    // ///
    // /// ```no_run
    // /// use std::io;
    // /// use std::io::prelude::*;
    // /// use std::fs::File;
    // ///
    // /// fn main() -> io::Result<()> {
    // ///     let mut f1 = File::open("foo.txt")?;
    // ///     let mut f2 = File::open("bar.txt")?;
    // ///
    // ///     let mut handle = f1.chain(f2);
    // ///     let mut buffer = String::new();
    // ///
    // ///     // read the value into a String. We could use any Read method here,
    // ///     // this is just one example.
    // ///     handle.read_to_string(&mut buffer)?;
    // ///     Ok(())
    // /// }
    // /// ```
    // fn chain<R: Read>(self, next: R) -> Chain<Self, R>
    //     where
    //         Self: Sized,
    // {
    //     Chain { first: self, second: next, done_first: false }
    // }
    //
    // fn take(self, limit: u64) -> Take<Self>
    //     where
    //         Self: Sized,
    // {
    //     Take { inner: self, limit }
    // }
}
