#![no_std]
#![feature(linkage)]
#![feature(format_args_nl)]

pub mod console;
pub mod lang_items;
pub mod syscall;

pub use syscall::*;

// const MICRO_PER_SEC: usize = 1_000_000;
pub const CLOCK_FREQ: usize = 12500000;


#[no_mangle]
#[link_section = ".text.entry"]
pub extern "C" fn _start() -> ! {
    clear_bss();
    // let HELLO: &[u8] = b"hello, world!";
    // sys_write(1, &[b'h']);
    // sys_write(1, HELLO);
    // println!("_start");
    let xstate = main();
    syscall::sys_exit(xstate);
}

#[linkage = "weak"]
#[no_mangle]
pub fn main() -> i32 {
    panic!("Cannot find main!");
}

pub fn clear_bss() {
    extern "C" {
        fn sbss();
        fn ebss();
    }
    (sbss as usize..ebss as usize).for_each(|addr| unsafe { (addr as *mut u8).write_volatile(0) })
}

pub fn yield_() {
    syscall::sys_yield();
}

pub fn get_time() -> isize {
    let mut time = TimeVal::new();
    match sys_get_time(&mut time, 0) {
        0 => ((time.sec & 0xffff) * 1000 + time.usec / 1000) as isize,
        _ => -1,
    }
}


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
    pub status: TaskStatus,
    pub syscall_times: [u32; syscall::MAX_SYSCALL_NUM],
    pub time: usize
}

impl TaskInfo {
    pub fn new() -> Self {
        TaskInfo {
            status: TaskStatus::UnInit,
            syscall_times: [0; MAX_SYSCALL_NUM],
            time: 0,
        }
    }
}

#[repr(C)]
#[derive(Debug, Default)]
pub struct TimeVal {
    pub sec: usize,
    pub usec: usize,
}

impl TimeVal {
    pub fn new() -> Self {
        Self::default()
    }
}

pub fn sleep(period_ms: usize) {
    let start = get_time();
    while get_time() < start + period_ms as isize {
        sys_yield();
    }
}

pub fn task_info(info: &mut TaskInfo) -> isize {
    sys_task_info(info)
}

pub fn write(fd: usize, buf: &[u8]) -> isize {
    sys_write(fd, buf)
}