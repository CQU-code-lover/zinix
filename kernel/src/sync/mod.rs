pub mod cpu_local;

use core::cell::UnsafeCell;
use core::hint::spin_loop;
use core::ops::{Deref, DerefMut};
use core::sync::atomic::{AtomicBool, Ordering};
use log::error;
use crate::asm::{disable_irq, enable_irq};
use crate::println;

pub type LockResult<Guard> = Result<Guard, u32>;


pub struct SpinLock<T:?Sized> {
    inner:AtomicBool,
    data:UnsafeCell<T>
}

unsafe impl<T: ?Sized+Send> Send for SpinLock<T>{}

unsafe impl<T: ?Sized+Send> Sync for SpinLock<T>{}

pub struct SpinLockGuard<'a,T:?Sized> {
    spinlock:&'a SpinLock<T>,
    irq_lock : bool,
    irq_state: usize
}

impl <T> SpinLock<T> {
    pub fn new(data:T)->SpinLock<T>{
        SpinLock{
            inner:AtomicBool::new(false),
            data:UnsafeCell::new(data)
        }
    }
}

impl<T:?Sized> SpinLock<T> {
    pub fn try_lock(&self){
        while self.inner.compare_and_swap(false, true, Ordering::Acquire) != false {
            let mut try_count = 0;
            // Wait until the lock looks unlocked before retrying
            while self.inner.load(Ordering::Relaxed) {
                spin_loop();
                try_count += 1;
                if try_count == 0x100000 {
                    panic!("Dead Lock!");
                }
            }
        }
    }

    pub fn lock(&self)->LockResult<SpinLockGuard<T>>{
        self.try_lock();
        Ok(SpinLockGuard::new(self,false,0))
    }

    pub fn lock_irq(&self)->LockResult<SpinLockGuard<T>>{
        let irq_state = disable_irq();
        self.try_lock();
        Ok(SpinLockGuard::new(self,true,irq_state))
    }

    fn _unlock(&self){
        self.inner.store(false,Ordering::Release);
    }
}

impl<'a,T:?Sized> SpinLockGuard<'a,T> {
    fn new(spinlock:&'a SpinLock<T>,irq_lock:bool,irq_state:usize)->Self{
        SpinLockGuard{
            spinlock,
            irq_lock,
            irq_state
        }
    }
}

impl<T:?Sized> Drop for SpinLockGuard<'_,T>{
    fn drop(&mut self) {
        self.spinlock._unlock();
        if self.irq_lock{
            enable_irq(self.irq_state)
        }
    }
}

impl<'a,T:?Sized> Deref for SpinLockGuard<'a,T> {
    type Target = T;

    fn deref<'b>(&'b self) -> &'b T {
        unsafe {&*self.spinlock.data.get()}
    }
}

impl<T:?Sized> DerefMut for SpinLockGuard<'_,T> {
    fn deref_mut(&mut self) -> &mut Self::Target {
        unsafe {&mut *self.spinlock.data.get()}
    }
}

pub fn get_irq_lock()->SpinLock<u8> {
    SpinLock::new(0u8)
}