
#[derive(Clone,Debug)]
#[repr(C)]
pub struct PollFd{
    fd:i32,
    events:u16,
    revents:u16,
}