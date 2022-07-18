#![no_std]
#![no_main]
#![feature(format_args_nl)]

use core::arch::global_asm;
use os::*;

global_asm!(include_str!("link_app.S"));

#[no_mangle]
pub fn rust_main() {
    init();
    let app_mgr = batch::APP_MANAGER.lock();
    app_mgr.print_app_info();
    drop(app_mgr);
    println!("Running apps..");
    batch::run_next_app();
}
