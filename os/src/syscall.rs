use crate::print;
// use crate::println;
use crate::task::run_next_task;
use crate::task::exit_and_run_next;
use crate::task::record_syscall;
use crate::task::TASK_MANAGER;
use crate::task::TaskStatus;
use crate::time;
use crate::mm::*;

pub const STDOUT: usize = 1;
pub const MAX_SYSCALL_NUM: usize = 500;

pub const SYSCALL_EXIT: usize = 93;
pub const SYSCALL_WRITE: usize = 64;
pub const SYSCALL_YIELD: usize = 124;
pub const SYSCALL_GET_TIME: usize = 169;
pub const SYSCALL_MUNMAP: usize = 215;
pub const SYSCALL_MMAP: usize = 222;
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
        SYSCALL_MMAP => {
            // TODO: check start
            let start = VirtAddr::new(args[0]);
            let len = args[1];
            let prot = args[2];

            if start.offset() != 0 {
                return -1;
            }
            if len == 0 {
                return 0;
            }
            if prot & !7 != 0 || prot & 7 == 0 {
                return -1;
            }

            let mut task_mgr = TASK_MANAGER.lock();
            let current_task = task_mgr.current_task();
            let addr_space = &mut task_mgr.addr_spaces[current_task];

            let mut checked_len = 0;
            while checked_len < len {
                let checked_va = VirtAddr::new(start.0 + checked_len);
                if addr_space.translate(checked_va).is_some() {
                    return -1;
                }
                checked_len += 4096;
            }

            let mut flags_at_level = [
                PteFlags::user_leaf(),
                PteFlags::user_inner(),
                PteFlags::user_inner(),
            ];
            if prot & 0b1 != 0 {
                flags_at_level[0] |= PteFlags::R;
            }
            if prot & 0b10 != 0 {
                // W imply R
                flags_at_level[0] |= PteFlags::R | PteFlags::W;
            }
            if prot & 0b100 != 0 {
                flags_at_level[0] |= PteFlags::X;
            }
            let mut mapped_len = 0;
            while mapped_len < len {
                let mapped_va = VirtAddr::new(start.0 + mapped_len);
                let frame = addr_space.alloc_frame();
                addr_space.build_mapping(mapped_va.vpn(), frame, flags_at_level);
                mapped_len += 4096;
            }

            unsafe {
                // use riscv::register::satp;
                // satp::set(satp::Mode::Sv39, addr_space.asid, addr_space.page_table.0);
                // riscv::register::sstatus::set_sum();
                riscv::asm::sfence_vma_all();
            }

            0
        }
        SYSCALL_MUNMAP => {
            let start = VirtAddr::new(args[0]);
            let len = args[1];
            if start.offset() != 0 {
                return -1;
            }

            let mut task_mgr = TASK_MANAGER.lock();
            let current_task = task_mgr.current_task();
            let addr_space = &mut task_mgr.addr_spaces[current_task];

            let mut checked_len = 0;
            while checked_len < len {
                let checked_va = VirtAddr::new(start.0 + checked_len);
                if addr_space.translate(checked_va).is_none() {
                    return -1;
                }
                checked_len += 4096;
            }
            let flags_at_level = [
                PteFlags::empty(),
                PteFlags::user_inner(),
                PteFlags::user_inner(),
            ];

            let mut unmapped_len = 0;
            while unmapped_len < len {
                let unmapped_va = VirtAddr::new(start.0 + unmapped_len);
                let frame = addr_space.translate(unmapped_va).unwrap().ppn();
                addr_space.build_mapping(unmapped_va.vpn(), frame, flags_at_level);
                unmapped_len += 4096;
            }

            unsafe {
                riscv::asm::sfence_vma_all();
            }
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
