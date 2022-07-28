#![no_std]
#![no_main]
#![feature(format_args_nl)]

use user_lib::println;
// use user_lib::sys_write;

#[no_mangle]
fn main() -> i32 {
    // sys_write(1, b"hello, world!");
    println!("Hello, World!");
    0
}
