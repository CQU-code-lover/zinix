use core::fmt;
use core::fmt::{Debug, Formatter};
use core::ops::{Add, AddAssign, Sub, SubAssign};
use core::ptr::addr_of;

use crate::consts::{PAGE_OFFSET, PAGE_SIZE};

#[repr(C)]
#[derive(Copy, Clone, Ord, PartialOrd, Eq, PartialEq)]
pub struct Addr(pub usize);

impl Default for Addr {
    fn default() -> Self {
        Addr(0)
    }
}

// PFN
#[repr(C)]
#[derive(Copy, Clone, Ord, PartialOrd, Eq, PartialEq)]
pub struct PFN(pub usize);

impl Debug for Addr {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        f.write_fmt(format_args!("ADDR:{:#x}", self.0))
    }
}

impl Add for Addr{
    type Output = Addr;

    fn add(self, rhs: Self) -> Self::Output {
        return  Addr(self.0+rhs.0);
    }
}

impl Sub for Addr{
    type Output = Addr;

    fn sub(self, rhs: Self) -> Self::Output {
        return  Addr(self.0-rhs.0);
    }
}

impl AddAssign for Addr {
    fn add_assign(&mut self, rhs: Self) {
        self.0 += rhs.0
    }
}

impl SubAssign for Addr {
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

impl From<usize> for Addr {
    fn from(v: usize) -> Self { Self(v) }
}

impl From<usize> for PFN {
    fn from(v: usize) -> Self { Self(v) }
}

impl From<Addr> for PFN {
    fn from(v: Addr) -> Self { Self(v.0) }
}

impl Addr {
    pub fn floor(&self)->Addr{
        Addr::from((self.0/PAGE_SIZE)*PAGE_SIZE)
    }
    pub fn ceil(&self)->Addr{
        Addr::from(
            if self.0%PAGE_SIZE == 0{
                self.floor().0
            } else {
                self.floor().0 + PAGE_SIZE
            }
        )
    }
    pub fn get_pg_cnt(&self)->usize{
        return self.0/PAGE_SIZE;
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