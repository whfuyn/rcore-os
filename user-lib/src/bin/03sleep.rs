#![no_std]
#![no_main]
#![feature(format_args_nl)]

use user_lib::*;

#[no_mangle]
fn main() -> i32 {
    let current_timer = get_time();
    let wait_for = current_timer + 30000;
    while get_time() < wait_for {
        yield_();
    }
    println!("Test sleep OK!");
    0
}
