#![no_std]
#![no_main]
#![feature(format_args_nl)]

use os::mm;

core::arch::global_asm!(include_str!("kernel.S"));

extern "C" {
    fn skernel();
    fn ekernel();
}

fn main() {
    
}
