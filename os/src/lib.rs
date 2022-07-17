#![no_std]
#![no_main]
#![feature(format_args_nl)]

pub mod lang_items;
pub mod console;
pub mod sbi;
pub mod batch;
pub mod trap;
pub mod syscall;

core::arch::global_asm!(
    include_str!("entry.asm")
);

pub fn clear_bss() {
    extern "C" {
        fn sbss();
        fn ebss();
    }
    (sbss as usize .. ebss as usize).for_each(|addr| {
        unsafe {
            (addr as *mut u8).write_volatile(0)
        }
    })
}

pub fn init() {
    clear_bss();
    trap::init();

}
