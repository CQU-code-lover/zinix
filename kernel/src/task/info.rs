

bitflags!{
    pub struct CloneFlags: usize{
        const SIGCHLD = 17;
        const CLONE_VM = 0x00000100;
        const CLONE_FS      =  0x00000200;
        const CLONE_FILES   =  0x00000400;
        const CLONE_SIGHAND =  0x00000800;
        const CLONE_PID    =  0x00001000;
        const CLONE_PTRACE  =  0x00002000;
        const CLONE_VFORK  = 0x00004000;
        const CLONE_PARENT =  0x00008000;
        const CLONE_THREAD  = 0x00010000;
        const CLONE_NEWNS =  0x00020000;
        const CLONE_CHILD_CLEARTID = 0x00200000;
        const CLONE_CHILD_SETTID = 0x01000000;
    }
}

/* fcntl */
/* cmd */
pub const F_DUPFD: u32 = 0; /*  dup the fd using the lowest-numbered
                            available file descriptor greater than or equal to arg.
                            on success, return new fd*/

pub const F_GETFD: u32 = 1; /* fd flag */
pub const F_SETFD: u32 = 2;
pub const F_GETFL: u32 = 3;

pub const F_DUPFD_CLOEXEC: u32 = 1030;  /* Duplicate file descriptor with close-on-exit set.*/

/* arg */
pub const FD_CLOEXEC: u32 = 1;


pub const S_IFMT    :u32 = 0o170000;   //bit mask for the file type bit field
pub const S_IFSOCK  :u32 = 0o140000;   //socket
pub const S_IFLNK   :u32 = 0o120000;   //symbolic link
pub const S_IFREG   :u32 = 0o100000;   //regular file
pub const S_IFBLK   :u32 = 0o060000;   //block device
pub const S_IFDIR   :u32 = 0o040000;   //directory
pub const S_IFCHR   :u32 = 0o020000;   //character device
pub const S_IFIFO   :u32 = 0o010000;   //FIFO

pub const S_ISUID:u32 = 0o4000;   //set-user-ID bit (see execve(2))
pub const S_ISGID:u32 = 0o2000;   //set-group-ID bit (see below)
pub const S_ISVTX:u32 = 0o1000;   //sticky bit (see below)

pub const S_IRWXU:u32 = 0o0700;   //owner has read, write, and execute permission
pub const S_IRUSR:u32 = 0o0400;   //owner has read permission
pub const S_IWUSR:u32 = 0o0200;   //owner has write permission
pub const S_IXUSR:u32 = 0o0100;   //owner has execute permission

pub const S_IRWXG:u32 = 0o0070;   //group has read, write, and execute permission
pub const S_IRGRP:u32 = 0o0040;   //group has read permission
pub const S_IWGRP:u32 = 0o0020;   //group has write permission
pub const S_IXGRP:u32 = 0o0010;   //group has execute permission

pub const S_IRWXO:u32 = 0o0007;   //others (not in group) have read, write,and execute permission
pub const S_IROTH:u32 = 0o0004;   //others have read permission
pub const S_IWOTH:u32 = 0o0002;   //others have write permission
pub const S_IXOTH:u32 = 0o0001;   //others have execute permission

pub struct Utsname {
    sysname: [u8; 65],
    nodename: [u8; 65],
    release: [u8; 65],
    version: [u8; 65],
    machine: [u8; 65],
    domainname: [u8; 65],
}


impl Utsname {
    pub fn new() -> Self{
        Self{
            //sysname: utsname::str2u8("UltraOS"),
            //nodename: utsname::str2u8("UltraOS"),
            //release: utsname::str2u8("5.10.0-7-riscv64"),
            //version: utsname::str2u8("1.1"),
            //machine: utsname::str2u8("RISC-V64"),
            //domainname: utsname::str2u8("UltraTEAM/UltraOS"),
            sysname: Utsname::str2u8("Linux"),
            nodename: Utsname::str2u8("debian"),
            release: Utsname::str2u8("5.10.0-7-riscv64"),
            version: Utsname::str2u8("#1 SMP Debian 5.10.40-1 (2021-05-28)"),
            machine: Utsname::str2u8("riscv64"),
            domainname: Utsname::str2u8(""),
        }
    }

    fn str2u8(str: &str) -> [u8;65]{
        let mut arr:[u8;65] = [0;65];
        let str_bytes = str.as_bytes();
        let len = str.len();
        for i in 0..len{
            arr[i] = str_bytes[i];
        }
        arr
    }

    pub fn as_bytes(&self) -> &[u8] {
        let size = core::mem::size_of::<Self>();
        unsafe {
            core::slice::from_raw_parts(
                self as *const _ as usize as *const u8,
                size,
            )
        }
    }
}

#[derive(Debug)]
#[repr(C)]
pub struct NewStat{

    /* the edition that can pass bw_test */
    st_dev  :u64,   /* ID of device containing file */
    //__pad1  :u32,
    st_ino  :u64,   /* Inode number */
    st_mode :u32,   /* File type and mode */
    st_nlink:u32,   /* Number of hard links */
    st_uid  :u32,
    st_gid  :u32,
    //st_rdev :u64,   /* Device ID (if special file) */
    //__pad2  :u32,
    st_blksize   :u64,    /* Block size for filesystem I/O */
    st_blocks    :u64,    /* Number of 512B blocks allocated */
    pub st_size  :u64,         /* Total size, in bytes */ //????????????
    st_atime_sec :i64,
    st_atime_nsec:i64,
    st_mtime_sec :i64,
    st_mtime_nsec:i64,
    st_ctime_sec :i64,
    st_ctime_nsec:i64,

    //st_dev  :u64,   /* ID of device containing file */
    ////__pad1  :u32,
    //st_ino  :u64,   /* Inode number */
    //st_mode :u32,   /* File type and mode */
    //st_nlink:u64,   /* Number of hard links */
    //st_uid  :u32,
    //st_gid  :u32,
    ////st_rdev :u64,   /* Device ID (if special file) */
    ////__pad2  :u32,
    //st_blksize   :u64,    /* Block size for filesystem I/O */
    //st_blocks    :u64,    /* Number of 512B blocks allocated */
    //pub st_size  :u64,         /* Total size, in bytes */ //????????????
    //st_atime_sec :i64,
    //st_atime_nsec:i64,
    //st_mtime_sec :i64,
    //st_mtime_nsec:i64,
    //st_ctime_sec :i64,
    //st_ctime_nsec:i64,


}

impl NewStat {
    pub fn empty()->Self{
        Self{
            st_dev  :0,
            //__pad1  :0,
            st_ino  :0,
            st_mode :0,
            st_nlink:0,
            st_uid  :0,
            st_gid  :0,
            //st_rdev :0,
            //__pad2  :0,
            st_size :0,
            st_blksize   :512,
            st_blocks    :0,
            st_atime_sec :0,
            st_atime_nsec:0,
            st_mtime_sec :0,
            st_mtime_nsec:0,
            st_ctime_sec :0,
            st_ctime_nsec:0,
        }
    }

    pub fn fill_info(&mut self,
                     st_dev  :u64,
                     st_ino  :u64,
                     st_mode :u32,
                     st_nlink:u64,
                     //st_uid  :u32,
                     //st_gid  :u32,
                     //st_rdev :u64,
                     st_size :i64,
                     //st_blksize   :u32,
                     //st_blocks    :u64,
                     st_atime_sec :i64,
                     //st_atime_nsec:i64,
                     st_mtime_sec :i64,
                     //st_mtime_nsec:i64,
                     st_ctime_sec :i64,
                     //st_ctime_nsec:i64,
    ) {
        let st_blocks = ( st_size as u64 + self.st_blksize as u64 - 1)
            / self.st_blksize as u64;

        *self = Self {
            st_dev,
            //__pad1  :0,
            st_ino ,
            st_mode,
            st_nlink:st_nlink as u32,
            //st_nlink,
            st_uid  :0,
            st_gid  :0,
            //st_rdev :0,
            //__pad2  :0,
            st_size : st_size as u64,
            st_blksize :self.st_blksize, //TODO:real blksize
            st_blocks ,
            st_atime_sec,
            st_atime_nsec:0,
            st_mtime_sec ,
            st_mtime_nsec:0,
            st_ctime_sec ,
            st_ctime_nsec:0,
        };
    }

    pub fn as_bytes(&self) -> &[u8] {
        let size = core::mem::size_of::<Self>();
        unsafe {
            core::slice::from_raw_parts(
                self as *const _ as usize as *const u8,
                size,
            )
        }
    }

    pub fn as_bytes_mut(&mut self) -> &mut [u8] {
        let size = core::mem::size_of::<Self>();
        unsafe {
            core::slice::from_raw_parts_mut(
                self as *mut _ as usize as *mut u8,
                size,
            )
        }
    }
}
