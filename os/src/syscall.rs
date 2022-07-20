use crate::print;
use crate::task::run_next_task;
use crate::task::exit_task_and_run_next;

const STDOUT: usize = 1;
const SYSCALL_EXIT: usize = 93;
const SYSCALL_WRITE: usize = 64;
const SYSCALL_YIELD: usize = 124;

pub fn syscall(id: usize, args: [usize; 3]) -> isize {
    match id {
        SYSCALL_EXIT => {
            exit_task_and_run_next();
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
        unknown => panic!("unknown syscall `{}`", unknown),
    }
}
