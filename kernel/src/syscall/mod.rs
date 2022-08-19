mod sys_fs;
mod sys_proc;
mod sys_dev;

use alloc::sync::Arc;
use core::cmp::min;
use core::mem::size_of;
use core::ptr::{slice_from_raw_parts, slice_from_raw_parts_mut};
use fatfs::error;
use crate::{error_sync, info_sync, println, trace_sync, warn_sync};
use crate::fs::poll::PollFd;
use crate::mm::addr::Vaddr;
use crate::sbi::shutdown;
use crate::syscall::sys_fs::syscall_fs_entry;
use crate::syscall::sys_proc::syscall_proc_entry;
use crate::task::{exit_self, sleep_self_in_sleeping_list};
use crate::task::task::get_running;
use crate::trap::TrapFrame;
use crate::utils::convert_cstr_from_vaddr;

pub const SYSCALL_GETCWD: usize = 17;
pub const SYSCALL_DUP: usize = 23;
pub const SYSCALL_DUP3:usize = 24;
pub const SYSCALL_FCNTL:usize = 25;
pub const SYSCALL_IOCTL:usize = 29;
pub const SYSCALL_MKDIRAT: usize = 34;
pub const SYSCALL_UNLINKAT: usize = 35;
pub const SYSCALL_LINKAT: usize = 37;
pub const SYSCALL_UMOUNT2: usize = 39;
pub const SYSCALL_MOUNT: usize = 40;
pub const SYSCALL_FACCESSAT: usize = 48;
pub const SYSCALL_CHDIR: usize = 49;
pub const SYSCALL_OPENAT: usize = 56;
pub const SYSCALL_CLOSE: usize = 57;
pub const SYSCALL_PIPE: usize = 59;
pub const SYSCALL_GETDENTS64: usize = 61;
pub const SYSCALL_LSEEK: usize = 62;
pub const SYSCALL_READ: usize = 63;
pub const SYSCALL_WRITE: usize = 64;
pub const SYSCALL_WRITEV: usize = 66;
pub const SYSCALL_SENDFILE: usize = 71;
pub const SYSCALL_PSELECT6: usize = 72;
pub const SYSCALL_PPOLL: usize = 73;
pub const SYSCALL_READLINKAT: usize = 78;
pub const SYSCALL_NEW_FSTATAT: usize = 79;
pub const SYSCALL_FSTAT:usize = 80;
pub const SYSCALL_FSYNC:usize = 82;
pub const SYSCALL_UTIMENSAT:usize = 88;
pub const SYSCALL_EXIT: usize = 93;
pub const SYSCALL_EXIT_GRUOP: usize = 94;
pub const SYSCALL_SET_TID_ADDRESS: usize = 96;
pub const SYSCALL_NANOSLEEP: usize = 101;
pub const SYSCALL_GETITIMER: usize = 102;
pub const SYSCALL_SETITIMER: usize = 103;
pub const SYSCALL_CLOCK_GETTIME: usize = 113;
pub const SYSCALL_YIELD: usize = 124;
pub const SYSCALL_KILL: usize = 129;
pub const SYSCALL_SIGACTION: usize = 134;
pub const SYSCALL_SIGPROCMASK: usize = 135;
pub const SYSCALL_SIGRETURN: usize = 139;
pub const SYSCALL_TIMES: usize = 153;
pub const SYSCALL_UNAME: usize = 160;
pub const SYSCALL_GETRUSAGE: usize = 165;
pub const SYSCALL_GET_TIME_OF_DAY: usize = 169;
pub const SYSCALL_GETPID: usize = 172;
pub const SYSCALL_GETPPID: usize = 173;
pub const SYSCALL_GETUID: usize = 174;
pub const SYSCALL_GETEUID: usize = 175;
pub const SYSCALL_GETGID: usize = 176;
pub const SYSCALL_GETEGID: usize = 177;
pub const SYSCALL_GETTID: usize = 178;
pub const SYSCALL_SBRK: usize = 213;
pub const SYSCALL_BRK: usize = 214;
pub const SYSCALL_MUNMAP: usize = 215;
pub const SYSCALL_CLONE: usize = 220;
pub const SYSCALL_EXECVE: usize = 221;
pub const SYSCALL_MMAP: usize = 222;
pub const SYSCALL_MPROTECT: usize = 226;
pub const SYSCALL_WAIT4: usize = 260;
pub const SYSCALL_PRLIMIT: usize = 261;
pub const SYSCALL_RENAMEAT2: usize = 276;

// Not standard POSIX sys_call
const SYSCALL_LS: usize = 500;
const SYSCALL_SHUTDOWN: usize = 501;
const SYSCALL_CLEAR: usize = 502;

pub unsafe fn syscall_entry(trap_frame:&mut TrapFrame){
    let syscall_id = trap_frame.x17;
    warn_sync!("[syscall:{}]",syscall_id);
    match syscall_id {
        // todo signal
        SYSCALL_PPOLL =>{
            let ptr = trap_frame.arg0();
            let e = (*(ptr as *const PollFd)).clone();
            println!("{:?},cnt:{}",e,trap_frame.arg1());
            loop{sleep_self_in_sleeping_list();}
            // error_sync!("ppoll");
            // trap_frame.ok()
        }
        SYSCALL_SIGACTION => {
            trap_frame.ok()
        }
        SYSCALL_SIGPROCMASK =>{
            trap_frame.ok()
        }
        SYSCALL_EXIT_GRUOP =>{
            trap_frame.ok();
        }
        // todo ioctl
        SYSCALL_IOCTL=>{
            info_sync!("syscall ioctl,trap frame:\n{:?}",trap_frame);
            trap_frame.ok();
        }
        SYSCALL_GETUID=>{
            trap_frame.ret(0);
        }
        SYSCALL_OPENAT|SYSCALL_SENDFILE|SYSCALL_WRITEV|SYSCALL_WRITE|SYSCALL_READ|
        SYSCALL_DUP|SYSCALL_DUP3|SYSCALL_CLOSE|SYSCALL_NEW_FSTATAT|SYSCALL_FCNTL|
        SYSCALL_PIPE=> {
            syscall_fs_entry(trap_frame,syscall_id);
        }
        SYSCALL_BRK|SYSCALL_MMAP|SYSCALL_GETPID|SYSCALL_GETPPID|SYSCALL_UNAME|SYSCALL_GETCWD|
        SYSCALL_CLONE|SYSCALL_SET_TID_ADDRESS|SYSCALL_WAIT4|SYSCALL_GETTID|SYSCALL_EXIT|SYSCALL_EXECVE=> {
            syscall_proc_entry(trap_frame,syscall_id);
        }
        _ => {
            error_sync!("syscall[{}] not register",syscall_id);
        }
    }
}