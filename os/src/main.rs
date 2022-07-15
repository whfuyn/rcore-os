#![no_std]
#![no_main]
#![feature(format_args_nl)]

use core::arch::global_asm;
use os::*;

global_asm!(
    include_str!("entry.asm")
);

#[no_mangle]
pub fn rust_main() {
    clear_bss();
    println!("Hello, world!");
    sbi::shutdown();
}
