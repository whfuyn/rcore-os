const MAX_SYSCALL_NUM: usize = 500;

const SYSCALL_EXIT: usize = 93;
const SYSCALL_WRITE: usize = 64;
const SYSCALL_YIELD: usize = 124;
const SYSCALL_GET_TIME: usize = 169;
const SYSCALL_TASK_INFO: usize = 410;

#[repr(C)]
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum TaskStatus {
    #[default]
    UnInit = 0,
    Ready = 1,
    Running = 2,
    Exited = 3,
}

#[repr(C)]
#[derive(Debug)]
pub struct TaskInfo {
    status: TaskStatus,
    syscall_times: [u32; MAX_SYSCALL_NUM],
    time: usize
}

#[repr(C)]
#[derive(Debug)]
pub struct TimeVal {
    pub sec: usize,
    pub usec: usize,
}

pub fn syscall(id: usize, args: [usize; 3]) -> isize {
    let mut ret;
    unsafe {
        core::arch::asm!(
            // TODO: should I clear x16 for syscall?
            // "li x16, 0",
            "ecall",
            inlateout("x10") args[0] => ret,
            in("x11") args[1],
            in("x12") args[2],
            in("x17") id,
        );
    }
    ret
}

pub fn sys_exit(xstate: i32) -> ! {
    syscall(SYSCALL_EXIT, [xstate as usize, 0, 0]);
    unreachable!("It should have exited")
}

pub fn sys_write(fd: usize, buffer: &[u8]) -> isize {
    syscall(SYSCALL_WRITE, [fd, buffer.as_ptr() as usize, buffer.len()])
}

pub fn sys_yield() -> isize {
    syscall(SYSCALL_YIELD, [0, 0, 0])
}

pub fn sys_task_info(ti: &mut TaskInfo) -> isize {
    syscall(SYSCALL_TASK_INFO, [ti as *mut TaskInfo as usize, 0, 0])
}

pub fn sys_get_time(time: &mut TimeVal, tz: usize) -> isize {
    syscall(SYSCALL_GET_TIME, [time as *mut TimeVal as usize, tz, 0])
}
