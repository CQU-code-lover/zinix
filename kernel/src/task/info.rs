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