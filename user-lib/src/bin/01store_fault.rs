#![no_std]
#![no_main]
#![feature(format_args_nl)]

use user_lib::println;

#[no_mangle]
fn main() -> i32 {
    println!("Into Test store_fault, we will insert an invalid store operation...");
    println!("Kernel should kill this application!");
    unsafe {
        core::ptr::null_mut::<u8>().write_volatile(0);
    }
    0
}
