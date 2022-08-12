

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
    pub fn readwriteable(&self)->bool{
        self.contains(Self::O_RDWR)
    }
    pub fn readable(&self)->bool{
        if self.readwriteable(){
            true
        } else {
            !self.contains(Self::O_WRONLY)
        }
    }
    pub fn writeable(&self)->bool{
        if self.readwriteable(){
            true
        } else {
            self.contains(Self::O_WRONLY)
        }
    }
}

bitflags! {
    pub struct OpenMode: u32 {
    }
}