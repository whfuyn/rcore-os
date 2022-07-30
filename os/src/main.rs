#![no_std]
#![no_main]
#![feature(format_args_nl)]
// #![feature(default_alloc_error_&handler)]

use os::*;
use mm::*;
use riscv::register::satp;
use config::*;

core::arch::global_asm!(include_str!("entry.S"));
extern "C" {
    fn sbss();
    fn ebss();
}

pub fn clear_bss() {
    (sbss as usize..ebss as usize).for_each(|addr| unsafe { (addr as *mut u8).write_volatile(0) })
}

pub fn init() {
    clear_bss();
    trap::init();

    unsafe {
        // Avoid timer interrupt during the init.
        riscv::register::sstatus::clear_sie();
        riscv::register::sstatus::set_sum();
        riscv::register::sie::set_stimer();
    }
}

#[no_mangle]
pub extern "C" fn rust_main(kernel_pa: PhysAddr, kernel_size: usize) {
    init();
    let kernel_pa_end = PhysAddr::new(kernel_pa.0 + kernel_size + PAGE_SIZE - 1).ppn();
    let memory_pa_end = PhysAddr::new(QEMU_MEMORY_END).ppn();
    mm::init(kernel_pa_end, memory_pa_end);

    println!("hello from os");
    println!("kernel pa: 0x{:x} 0x{:x}", kernel_pa.0, kernel_size);
    println!("satp: 0x{:x}", riscv::register::satp::read().bits());
    task::run_first_task();

    sbi::shutdown();
}
