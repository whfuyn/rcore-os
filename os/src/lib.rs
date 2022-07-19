#![no_std]
#![no_main]
#![feature(format_args_nl)]
#![feature(sync_unsafe_cell)]
#![feature(naked_functions)]

// pub mod batch;
pub mod console;
pub mod lang_items;
pub mod sbi;
pub mod syscall;
pub mod trap;
pub mod task;

use core::arch::global_asm;

global_asm!(include_str!("entry.S"));
extern "C" {
    fn sbss();
    fn ebss();
    fn _num_app();
}

pub fn clear_bss() {
    (sbss as usize..ebss as usize).for_each(|addr| unsafe { (addr as *mut u8).write_volatile(0) })
}

pub fn init() {
    clear_bss();
    trap::init();
}
