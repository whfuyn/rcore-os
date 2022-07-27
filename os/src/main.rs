#![no_std]
#![no_main]
#![feature(format_args_nl)]

use os::*;
use mm::*;
use riscv::register::satp;

const KERNEL_BASE_ADDRESS: VirtAddr = unsafe {
    VirtAddr::new_unchecked(0xffffffffc0000000)
};


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
        riscv::register::sie::set_stimer();
    }
}

// #[no_mangle]
// pub fn rust_main() {
//     println!("hello");
//     init();
//     task::run_first_task();
// }



#[no_mangle]
pub extern "C" fn rust_main(kernel_pa: PhysAddr, kernel_size: usize) {
    init();
    println!("hello from os");
    println!("kernel pa: 0x{:x} 0x{:x}", kernel_pa.0, kernel_size);
    let kernel_end = VirtAddr::new(KERNEL_BASE_ADDRESS.0 + kernel_size).vpn();
    let kernel_avail_va = VPN(kernel_end.0 + 1);
    mm::address_space::init(PPN(satp::read().bits()), kernel_avail_va);

    println!("satp: 0x{:x}", riscv::register::satp::read().bits());
    task::run_first_task();

    sbi::shutdown();
}
