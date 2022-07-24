use crate::mm::addr::Addr;
use crate::println;
use crate::sbi::shutdown;

pub fn do_test(){
    let a = Addr(1);
    let b = Addr(100);
    let c = a..b ;
    for i in c {
        println!("{:?}",i);
    }
    shutdown();
}