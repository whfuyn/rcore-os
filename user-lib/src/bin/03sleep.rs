#![no_std]
#![no_main]
#![feature(format_args_nl)]

use user_lib::*;

pub const CLOCK_FREQ: usize = 12500000;

#[no_mangle]
fn main() -> i32 {
    println!("start sleeping..");
    let current_time = get_time();
    let wait_for = current_time + 3000;
    while get_time() < wait_for {
        yield_();
    }
    println!("Test sleep OK!");
    0
}
