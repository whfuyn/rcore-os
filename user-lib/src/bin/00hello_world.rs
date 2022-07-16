#![no_std]
#![no_main]
#![feature(format_args_nl)]

use user_lib::println;

#[no_mangle]
fn main() {
    println!("Hello, World!");
}
