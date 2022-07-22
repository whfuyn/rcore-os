use crate::print;
use crate::task::run_next_task;
use crate::task::exit_and_run_next;
use crate::task::record_syscall;
use crate::task::TASK_MANAGER;
use crate::task::TaskStatus;
use crate::time;

pub const STDOUT: usize = 1;
pub const MAX_SYSCALL_NUM: usize = 500;

pub const SYSCALL_EXIT: usize = 93;
pub const SYSCALL_WRITE: usize = 64;
pub const SYSCALL_YIELD: usize = 124;
pub const SYSCALL_GET_TIME: usize = 169;
pub const SYSCALL_TASK_INFO: usize = 410;

#[repr(C)]
#[derive(Debug)]
struct TimeVal {
    pub sec: usize,
    pub usec: usize,
}

#[allow(dead_code)]
#[repr(C)]
#[derive(Debug)]
struct TaskInfo {
    pub status: TaskStatus,
    pub syscall_times: [u32; MAX_SYSCALL_NUM],
    pub time: usize
}

pub fn syscall(id: usize, args: [usize; 3]) -> isize {
    record_syscall(id);

    match id {
        SYSCALL_EXIT => {
            exit_and_run_next();
            0
        }
        SYSCALL_WRITE => {
            let fd = args[0];
            if fd != STDOUT {
                panic!("unsupported fd in syscall write");
            }

            let buffer_ptr = args[1];
            let buffer_size = args[2];
            let buffer = unsafe { core::slice::from_raw_parts(buffer_ptr as *const u8, args[2]) };

            print!(
                "{}",
                core::str::from_utf8(buffer).expect("try to print non-utf8 str")
            );
            buffer_size as isize
        }
        SYSCALL_YIELD => {
            // crate::println!("\nyield..");
            run_next_task();
            0
        }
        SYSCALL_GET_TIME => {
            let t = time::get_time();
            let time_val = unsafe { &mut *(args[0] as *mut TimeVal)};
            time_val.sec = t / time::CLOCKS_PER_SEC;
            // This funny formula is to work around percision issue of the CLOCKS_PER_MICRO_SEC,
            // which should be 12.5 instead of 12.
            time_val.usec = t % time::CLOCKS_PER_SEC / time::CLOCKS_PER_MILLI_SEC * 1000;

            0
        }
        SYSCALL_TASK_INFO => {
            let task_info = unsafe { &mut *(args[0] as *mut TaskInfo) };

            let task_mgr = TASK_MANAGER.lock();
            let status = task_mgr.current_tcb().status;
            let stat = task_mgr.current_stat();

            task_info.status = status;
            task_info.syscall_times = stat.syscall_times;
            task_info.time = stat.real_time() / time::CLOCKS_PER_MILLI_SEC;
            0
        }
        unknown => panic!("unknown syscall `{}`", unknown),
    }
}
