#![no_std]
#![no_main]
#![feature(format_args_nl)]
#![feature(sync_unsafe_cell)]
#![feature(naked_functions)]
#![feature(alloc_error_handler)]
#![feature(core_c_str)]
#![feature(core_ffi_c)]
#![feature(const_option)]

extern crate alloc;

pub mod console;
pub mod lang_items;
pub mod sbi;
pub mod syscall;
pub mod trap;
pub mod task;
pub mod time;
pub mod mm;
pub mod utils;
pub mod config;