#![no_std]
#![no_main]
#![feature(format_args_nl)]

use user_lib::*;

pub const CLOCK_FREQ: usize = 12500000;

#[no_mangle]
fn main() -> i32 {
    println!("start sleeping..");
    let current_timer = get_time();
    let wait_for = current_timer + 3 * CLOCK_FREQ;
    while get_time() < wait_for {
        yield_();
    }
    println!("Test sleep OK!");
    0
}
