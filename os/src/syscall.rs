use crate::batch::run_next_app;
use crate::print;

const STDOUT: usize = 1;
const SYSCALL_EXIT: usize = 93;
const SYSCALL_WRITE: usize = 64;

pub fn syscall(id: usize, args: [usize; 3]) -> isize {
    match id {
        SYSCALL_EXIT => {
            run_next_app();
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
        unknown => panic!("unknown syscall `{}`", unknown),
    }
}
