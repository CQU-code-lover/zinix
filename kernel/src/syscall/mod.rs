use core::cmp::min;
use core::mem::size_of;
use core::ptr::{slice_from_raw_parts, slice_from_raw_parts_mut};
use fatfs::error;
use crate::{error_sync, println, trace_sync};
use crate::sbi::shutdown;
use crate::task::exit_self;
use crate::task::task::get_running;
use crate::trap::TrapFrame;

const SYSCALL_GETCWD: usize = 17;
const SYSCALL_DUP: usize = 23;
const SYSCALL_DUP3:usize = 24;
const SYSCALL_FCNTL:usize = 25;
const SYSCALL_IOCTL:usize = 29;
const SYSCALL_MKDIRAT: usize = 34;
const SYSCALL_UNLINKAT: usize = 35;
const SYSCALL_LINKAT: usize = 37;
const SYSCALL_UMOUNT2: usize = 39;
const SYSCALL_MOUNT: usize = 40;
const SYSCALL_FACCESSAT: usize = 48;
const SYSCALL_CHDIR: usize = 49;
const SYSCALL_OPENAT: usize = 56;
const SYSCALL_CLOSE: usize = 57;
const SYSCALL_PIPE: usize = 59;
const SYSCALL_GETDENTS64: usize = 61;
const SYSCALL_LSEEK: usize = 62;
const SYSCALL_READ: usize = 63;
const SYSCALL_WRITE: usize = 64;
const SYSCALL_WRITEV: usize = 66;
const SYSCALL_SENDFILE: usize = 71;
const SYSCALL_PSELECT6: usize = 72;
const SYSCALL_READLINKAT: usize = 78;
const SYSCALL_NEW_FSTATAT: usize = 79;
const SYSCALL_FSTAT:usize = 80;
const SYSCALL_FSYNC:usize = 82;
const SYSCALL_UTIMENSAT:usize = 88;
const SYSCALL_EXIT: usize = 93;
const SYSCALL_EXIT_GRUOP: usize = 94;
const SYSCALL_SET_TID_ADDRESS: usize = 96;
const SYSCALL_NANOSLEEP: usize = 101;
const SYSCALL_GETITIMER: usize = 102;
const SYSCALL_SETITIMER: usize = 103;
const SYSCALL_CLOCK_GETTIME: usize = 113;
const SYSCALL_YIELD: usize = 124;
const SYSCALL_KILL: usize = 129;
const SYSCALL_SIGACTION: usize = 134;
const SYSCALL_SIGRETURN: usize = 139;
const SYSCALL_TIMES: usize = 153;
const SYSCALL_UNAME: usize = 160;
const SYSCALL_GETRUSAGE: usize = 165;
const SYSCALL_GET_TIME_OF_DAY: usize = 169;
const SYSCALL_GETPID: usize = 172;
const SYSCALL_GETPPID: usize = 173;
const SYSCALL_GETUID: usize = 174;
const SYSCALL_GETEUID: usize = 175;
const SYSCALL_GETGID: usize = 176;
const SYSCALL_GETEGID: usize = 177;
const SYSCALL_GETTID: usize = 177;
const SYSCALL_SBRK: usize = 213;
const SYSCALL_BRK: usize = 214;
const SYSCALL_MUNMAP: usize = 215;
const SYSCALL_CLONE: usize = 220;
const SYSCALL_EXEC: usize = 221;
const SYSCALL_MMAP: usize = 222;
const SYSCALL_MPROTECT: usize = 226;
const SYSCALL_WAIT4: usize = 260;
const SYSCALL_PRLIMIT: usize = 261;
const SYSCALL_RENAMEAT2: usize = 276;

// Not standard POSIX sys_call
const SYSCALL_LS: usize = 500;
const SYSCALL_SHUTDOWN: usize = 501;
const SYSCALL_CLEAR: usize = 502;

pub unsafe fn syscall_entry(trap_frame:&mut TrapFrame){
    let syscall_id = trap_frame.x17;
    trace_sync!("[syscall:{}]",syscall_id);
    match syscall_id {
        SYSCALL_EXIT =>{
            println!("EXIT");
            trap_frame.ok();
            exit_self();
        }
        SYSCALL_SET_TID_ADDRESS => {
            trap_frame.ok();
        }
        SYSCALL_WRITEV =>{
            #[repr(C)]
            #[derive(Copy,Clone)]
            struct IOVEC{
                iov_base:*mut u8,
                iov_len:usize
            }
            let fd = trap_frame.x10;
            let iov_array_base = trap_frame.x11;
            let mut len_res = trap_frame.x12;
            let len_need_read = len_res;
            let file = get_running().lock_irq().unwrap().get_opened(fd);
            for i in 0..usize::MAX {
                if len_res <=0{
                    break;
                }
                let iov = *((iov_array_base + i*size_of::<IOVEC>()) as *mut IOVEC);
                let real_write_len = min(iov.iov_len, len_res);
                let write_buf= slice_from_raw_parts(iov.iov_base, real_write_len).as_ref().unwrap();
                assert_eq!(file.inner.lock_irq().unwrap().write(write_buf), real_write_len);
                len_res -= real_write_len;
            }
            trap_frame.ret(len_need_read);
        }
        SYSCALL_EXIT_GRUOP =>{
            trap_frame.ok();
        }
        SYSCALL_IOCTL=>{
            trap_frame.ok();
        }
        _ => {
            error_sync!("syscall not register");
        }
    }
}