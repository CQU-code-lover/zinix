

pub const AT_FDCWD:isize = -100;

bitflags! {
    pub struct OpenFlags: u32 {
        const O_RDONLY = 0;
        const O_WRONLY = 1 << 0;
        const O_RDWR = 1 << 1;
        const O_CREATE = 1 << 6;
        const O_TRUNC = 1 << 10;
        const O_DIRECTROY = 0200000;
        const O_LARGEFILE  = 0100000;
        const O_CLOEXEC = 02000000;
    }
}

impl OpenFlags {
    pub fn check_main_flag(&self)->bool{
        let (f0,f1,f2) = (self.contains(Self::O_RDONLY),self.contains(Self::O_RDWR),self.contains(Self::O_WRONLY));
        if f0 {
            !f1 && !f2
        } else if f1{
            !f0 && !f2
        } else if f2{
            !f0 && !f1
        } else {
            false
        }
    }
    pub fn from_bits_checked(v:u32) ->Option<Self> {
        let f =Self::from_bits(v).unwrap();
        if f.check_main_flag() {
            Some(f)
        } else {
            None
        }
    }
}

bitflags! {
    pub struct OpenMode: u32 {
    }
}