#![no_std]
#![no_main]
#![feature(format_args_nl)]
#![feature(sync_unsafe_cell)]

pub mod batch;
pub mod console;
pub mod lang_items;
pub mod sbi;
pub mod syscall;
pub mod trap;

core::arch::global_asm!(include_str!("entry.asm"));

pub fn clear_bss() {
    extern "C" {
        fn sbss();
        fn ebss();
    }
    (sbss as usize..ebss as usize).for_each(|addr| unsafe { (addr as *mut u8).write_volatile(0) })
}

pub fn init() {
    clear_bss();
    trap::init();
}
