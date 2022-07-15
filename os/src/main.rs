#![no_std]
#![no_main]
#![feature(format_args_nl)]

use os::*;

#[no_mangle]
pub fn rust_main() {
    clear_bss();
    println!("Hello, world!");
    sbi::shutdown();
}
