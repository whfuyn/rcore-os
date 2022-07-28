#![no_std]
#![no_main]
#![feature(format_args_nl)]
// #![feature(default_alloc_error_&handler)]

use os::*;
use mm::*;
use riscv::register::satp;

const KERNEL_BASE_ADDRESS: VirtAddr = unsafe {
    VirtAddr::new_unchecked(0xffffffffc0000000)
};

const PAGE_SIZE: usize = 4096;
const QEMU_MEMORY_END: usize = 0x88000000;

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
    mm::heap_allocator::init();
    let kernel_pa_end = PhysAddr::new(kernel_pa.0 + kernel_size + PAGE_SIZE - 1).ppn();
    let memory_pa_end = PhysAddr::new(QEMU_MEMORY_END).ppn();
    mm::frame_allocator::init(kernel_pa_end, memory_pa_end);

    let kernel_va_end = VirtAddr::new(KERNEL_BASE_ADDRESS.0 + kernel_size + PAGE_SIZE - 1).vpn();
    let kernel_avail_va = VPN(kernel_va_end.0 + 1);
    mm::address_space::init(PPN(satp::read().ppn()), kernel_avail_va);

    println!("hello from os");
    println!("kernel pa: 0x{:x} 0x{:x}", kernel_pa.0, kernel_size);
    println!("satp: 0x{:x}", riscv::register::satp::read().bits());
    task::run_first_task();

    sbi::shutdown();
}
