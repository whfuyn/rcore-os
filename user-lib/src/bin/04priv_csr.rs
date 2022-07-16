#![no_std]
#![no_main]
#![feature(format_args_nl)]

use core::arch::asm;
use riscv::register::sstatus::{self, SPP};
use user_lib::println;

#[no_mangle]
fn main() -> i32 {
    println!("Try to access privileged CSR in U Mode");
    println!("Kernel should kill this application!");
    unsafe {
        sstatus::set_spp(SPP::User);
    }
    0
}

