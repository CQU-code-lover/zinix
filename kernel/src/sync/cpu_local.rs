#[macro_export]
macro_rules! cpu_local{
    () => {};

}

pub fn get_core_id() -> usize{
    let mut tp:usize = 0;
    unsafe {
        asm!("mv {}, tp",out(reg) tp);
    }
    // tp
    tp
}

pub fn set_core_id(core_id:usize){
    unsafe {
        asm!("mv tp, {}",in(reg) core_id);
    }
}
