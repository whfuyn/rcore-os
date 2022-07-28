use crate::*;

pub const MAX_SYSCALL_NUM: usize = 500;

pub const SYSCALL_EXIT: usize = 93;
pub const SYSCALL_WRITE: usize = 64;
pub const SYSCALL_YIELD: usize = 124;
pub const SYSCALL_GET_TIME: usize = 169;
pub const SYSCALL_GETTIMEOFDAY: usize = SYSCALL_GET_TIME;
pub const SYSCALL_MUNMAP: usize = 215;
pub const SYSCALL_MMAP: usize = 222;
pub const SYSCALL_TASK_INFO: usize = 410;

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

pub fn sys_mmap(start: usize, len: usize, prot: usize) -> isize {
    syscall(SYSCALL_MMAP, [start, len, prot])
}

pub fn sys_munmap(start: usize, len: usize) -> isize {
    syscall(SYSCALL_MUNMAP, [start, len, 0])
}
