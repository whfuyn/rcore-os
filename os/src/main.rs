#![no_std]
#![no_main]
#![feature(format_args_nl)]

use os::*;

core::arch::global_asm!(include_str!("entry.S"));
extern "C" {
    fn sbss();
    fn ebss();
}

pub fn clear_bss() {
    (sbss as usize..ebss as usize).for_each(|addr| unsafe { (addr as *mut u8).write_volatile(0) })
}

pub fn init() {
    clear_bss();
    trap::init();

    unsafe {
        // Avoid timer interrupt during the init.
        riscv::register::sstatus::clear_sie();
        riscv::register::sie::set_stimer();
    }
}

#[no_mangle]
pub fn rust_main() {
    println!("hello");
    init();
    task::run_first_task();
}
