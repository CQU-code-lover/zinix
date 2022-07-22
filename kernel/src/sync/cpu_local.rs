use crate::asm::{r_tp, w_tp};
#[macro_export]
macro_rules! cpu_local{
    () => {};
}

pub fn get_core_id() -> usize{
    r_tp()
}

pub fn set_core_id(core_id:usize){
    w_tp(core_id)
}
