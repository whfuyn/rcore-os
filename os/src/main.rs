#![no_std]
#![no_main]
#![feature(format_args_nl)]

use core::arch::global_asm;
use os::*;

global_asm!(
    include_str!("link_app.S")
);

#[no_mangle]
pub fn rust_main() {
    clear_bss();
    let mut app_manager = batch::APP_MANAGER.lock();
    app_manager.print_app_info();
    app_manager.load_app(0);
    println!("Hello, world!");
    sbi::shutdown();
}
