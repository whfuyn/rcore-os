#![no_std]
#![no_main]
#![feature(format_args_nl)]

use os::*;

#[no_mangle]
pub fn rust_main() {
    init();
    // let task_mgr = task::TASK_MANAGER.lock();
    // task_mgr.print_app_info();
    // drop(app_mgr);
    // println!("Running apps..");
    task::run_first_task();
}
