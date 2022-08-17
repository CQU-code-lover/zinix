use alloc::collections::LinkedList;
use alloc::sync::Arc;
use core::cmp::min;
use core::default::default;
use core::sync::atomic::{AtomicU8, Ordering};
use xmas_elf::symbol_table::Visibility::Default;
use crate::{SpinLock, Task};
use crate::task::{add_task, scheduler};
use crate::task::task::get_running;
use crate::task::task::TaskStatus::TaskSleeping;

pub struct Pipe {
    buffer: SpinLock<PipeRingBuffer>,
    write_cnt:AtomicU8,
    wait_write:SpinLock<LinkedList<Arc<SpinLock<Task>>>>,
    wait_read:SpinLock<LinkedList<Arc<SpinLock<Task>>>>
}

impl Pipe {
    pub fn new()->Self{
        Self{
            buffer: SpinLock::new(PipeRingBuffer::new()),
            write_cnt: AtomicU8::new(0),
            wait_write: SpinLock::new(default()),
            wait_read: SpinLock::new(default())
        }
    }
    fn __have_writer(&self)->bool{
        self.write_cnt.load(Ordering::SeqCst) != 0
    }
    pub fn inc_write(&self){
        self.write_cnt.fetch_add(1,Ordering::SeqCst);
    }
    pub fn dec_write(&self){
        if self.write_cnt.fetch_sub(1,Ordering::SeqCst)==1 {
            // writer全部释放 注意此处需要唤醒所有等待的reader，不然会导致无法醒来
            self.wake_up_read();
        }
    }
    fn wake_up_write(&self){
        let len = self.wait_write.lock().unwrap().len();
        if len == 0{
            return;
        }
        for i in 0..len {
            add_task(self.wait_write.lock_irq().unwrap().pop_front().unwrap());
        }
    }
    fn wake_up_read(&self){
        let len = self.wait_read.lock().unwrap().len();
        if len == 0{
            return;
        }
        for i in 0..len {
            add_task(self.wait_read.lock_irq().unwrap().pop_front().unwrap());
        }
    }
    fn sleep_wait_write(&self){
        get_running().lock_irq().unwrap().set_status(TaskSleeping);
        scheduler(Some(&self.wait_write))
    }
    fn sleep_wait_read(&self){
        get_running().lock_irq().unwrap().set_status(TaskSleeping);
        scheduler(Some(&self.wait_read))
    }
    pub fn read_exact(&self,buf:&mut [u8])->Result<usize,()> {
        let mut buf_pos = 0usize;
        let need_read = buf.len();
        while  buf_pos<need_read {
            let mut ring_buffer = self.buffer.lock_irq().unwrap();
            let read_once = ring_buffer.read(buf);
            if read_once == 0{
                if self.__have_writer(){
                    // sleep之前需要释放锁
                    drop(ring_buffer);
                    self.sleep_wait_read();
                    // re lock
                    ring_buffer = self.buffer.lock_irq().unwrap();
                } else {
                    // read at all writer close
                    return Ok(buf_pos);
                }
            }
            buf_pos+=read_once;
        }
        assert_eq!(buf_pos,need_read);
        self.wake_up_write();
        Ok(buf_pos)
    }
    pub fn write_exact(&self,buf:&[u8])->Result<usize,()> {
        let mut buf_pos = 0usize;
        let need_write = buf.len();
        while  buf_pos< need_write {
            let mut ring_buffer = self.buffer.lock_irq().unwrap();
            let write_once = ring_buffer.write(buf);
            if write_once == 0{
                drop(ring_buffer);
                self.sleep_wait_write();
                // re lock
                ring_buffer = self.buffer.lock_irq().unwrap();
            }
            buf_pos+= write_once;
        }
        assert_eq!(buf_pos, need_write);
        self.wake_up_read();
        Ok(buf_pos)
    }
}

const RING_BUFFER_SIZE: usize = 1024;

#[derive(Copy, Clone, PartialEq)]
enum RingBufferStatus {
    FULL,
    EMPTY,
    NORMAL,
}

pub struct PipeRingBuffer {
    arr: [u8; RING_BUFFER_SIZE],
    head: usize,
    tail: usize,
    status: RingBufferStatus,
    count:usize,
}

impl PipeRingBuffer {
    pub fn new() -> Self {
        Self {
            arr: [0; RING_BUFFER_SIZE],
            head: 0,
            tail: 0,
            status: RingBufferStatus::EMPTY,
            count:0,
        }
    }
    pub fn write_byte(&mut self, byte: u8) {
        self.status = RingBufferStatus::NORMAL;
        self.arr[self.tail] = byte;
        self.tail = (self.tail + 1) % RING_BUFFER_SIZE;
        if self.tail == self.head {
            self.status = RingBufferStatus::FULL;
        }
    }
    pub fn read_byte(&mut self) -> u8 {
        self.status = RingBufferStatus::NORMAL;
        let c = self.arr[self.head];
        self.head = (self.head + 1) % RING_BUFFER_SIZE;
        if self.head == self.tail {
            self.status = RingBufferStatus::EMPTY;
        }
        c
    }
    pub fn available_read(&self) -> usize {
        if self.status == RingBufferStatus::EMPTY {
            0
        } else {
            if self.tail > self.head {
                self.tail - self.head
            } else {
                self.tail + RING_BUFFER_SIZE - self.head
            }
        }
    }
    pub fn available_write(&self) -> usize {
        if self.status == RingBufferStatus::FULL {
            0
        } else {
            RING_BUFFER_SIZE - self.available_read()
        }
    }
    fn read(&mut self, buf :&mut [u8]) ->usize {
        let mut can_read = self.available_read();
        if can_read == 0 {
            return 0;
        }
        can_read = min(can_read,buf.len());
        for i in 0..can_read {
            buf[i] = self.read_byte();
        }
        return can_read;
    }
    fn write(&mut self, buf :&[u8]) ->usize {
        let mut can_write = self.available_write();
        if can_write == 0 {
            return 0;
        }
        can_write = min(can_write,buf.len());
        for i in 0..can_write {
            self.write_byte(buf[i]);
        }
        return can_write;
    }
}
