use alloc::sync::Arc;

use crate::print;
// use crate::println;
use crate::task::run_next_task;
use crate::task::exit_and_run_next;
use crate::task::record_syscall;
use crate::task::TaskStatus;
use crate::task::TaskControlBlock;
use crate::time;
use crate::mm::*;
use crate::task::PROCESSOR;
use crate::sbi::console_getchar;
use core::ffi::CStr;
use crate::task::get_app_data;
// use crate::task::TaskControlBlock;
use crate::task::TASK_MANAGER;

pub const FD_STDIN: usize = 0;
pub const FD_STDOUT: usize = 1;
pub const MAX_SYSCALL_NUM: usize = 500;

pub const SYSCALL_READ: usize = 63;
pub const SYSCALL_WRITE: usize = 64;
pub const SYSCALL_EXIT: usize = 93;
pub const SYSCALL_YIELD: usize = 124;
pub const SYSCALL_GET_TIME: usize = 169;
pub const SYSCALL_FORK: usize = 220;
pub const SYSCALL_EXEC: usize = 221;
pub const SYSCALL_WAITPID: usize = 260;
pub const SYSCALL_SPAWN: usize = 400;
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
        SYSCALL_READ => {
            let fd = args[0];
            let buffer = args[1] as *mut u8;
            let len = args[2];

            if fd != FD_STDIN {
                panic!("unsupported fd in syscall read");
            }
            assert_eq!(len, 1, "Only support len = 1 in sys_read!");
            let mut c: usize;
            loop {
                c = console_getchar();
                if c == 0 {
                    run_next_task();
                    continue;
                } else {
                    break;
                }
            }
            // We don't need to translate the buffer, since we didn't switch satp
            unsafe {
                buffer.write_volatile(c as u8);
            }

            1
        }
        SYSCALL_EXIT => {
            crate::println!("call sys exit");
            let exit_code = args[0] as i32;
            exit_and_run_next(exit_code);
            0
        }
        SYSCALL_WRITE => {
            let fd = args[0];
            if fd != FD_STDOUT {
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
        SYSCALL_EXEC => {
            let current_task = PROCESSOR.lock().current().expect("missing current").clone();

            let elf_name: &'static str = unsafe {
                CStr::from_ptr(args[0] as *const i8).to_str().expect("invalid app name")
            };
            let elf_data = match get_app_data(elf_name) {
                Some(elf_data) => elf_data,
                None => return -1,
            };
            current_task.exec(elf_data);

            unreachable!()
        }
        SYSCALL_FORK => {
            let current_task = PROCESSOR.lock().current().expect("missing current").clone();
            current_task.fork() as isize
        }
        SYSCALL_WAITPID => {
            let pid = args[0] as isize; 
            let exit_code_ptr = args[1] as *mut i32;

            let current_task = PROCESSOR.lock().current().expect("missing current").clone();
            let mut current_inner = current_task.lock();

            let mut ret = -1;
            let mut found: Option<usize> = None;
            for (idx, ch) in current_inner.children.iter().enumerate() {
                if pid == -1 || ch.pid.0 == pid as usize {
                    ret = -2;

                    if ch.is_zombie() {
                        ret = ch.pid.0 as isize;
                        found = Some(idx);
                        break;
                    }
                }
            }
            if let Some(found) = found {
                let exit_child = current_inner.children.remove(found);
                unsafe {
                    *exit_code_ptr = exit_child.lock().exit_code;
                }
            }

            ret
        }
        SYSCALL_SPAWN => {
            let current_task = PROCESSOR.lock().current().expect("missing current").clone();
            let mut current_inner = current_task.lock();

            let elf_name: &'static str = unsafe {
                CStr::from_ptr(args[0] as *const i8).to_str().expect("invalid app name")
            };
            let elf_data = match get_app_data(elf_name) {
                Some(elf_data) => elf_data,
                None => return -1,
            };

            let child_task = TaskControlBlock::load_from_elf(elf_data, Some(Arc::downgrade(&current_task)));
            current_inner.children.push(child_task.clone());

            let ret = child_task.pid.0 as isize;

            drop(current_inner);
            drop(current_task);
            TASK_MANAGER.lock().add(child_task);

            ret
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

            let processor = PROCESSOR.lock();
            let current_task = processor.current().expect("missing current");
            let mut current_inner = current_task.lock();
            let addr_space = &mut current_inner.addr_space;

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

            let processor = PROCESSOR.lock();
            let current_task = processor.current().expect("missing current");
            let mut current_inner = current_task.lock();
            let addr_space = &mut current_inner.addr_space;

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
                // TODO: free frame
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

            let processor = PROCESSOR.lock();
            let current_task = processor.current().expect("missing current");
            let current_inner = current_task.lock();
            let stat = &current_inner.stats;

            task_info.status = current_inner.status;
            task_info.syscall_times = stat.syscall_times;
            task_info.time = stat.real_time() / time::CLOCKS_PER_MILLI_SEC;

            0
        }
        unknown => panic!("unknown syscall `{}`", unknown),
    }
}
