#![no_std]
#![no_main]
#![feature(format_args_nl)]

pub mod lang_items;
pub mod console;
pub mod sbi;


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
