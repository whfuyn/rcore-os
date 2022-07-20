#![no_std]
#![feature(linkage)]
#![feature(format_args_nl)]

pub mod console;
pub mod lang_items;
pub mod syscall;

#[no_mangle]
#[link_section = ".text.entry"]
pub extern "C" fn _start() -> ! {
    clear_bss();
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

use riscv::register::time;

// const MICRO_PER_SEC: usize = 1_000_000;
pub const CLOCK_FREQ: usize = 12500000;


pub fn get_time() -> usize {
    time::read()
}

