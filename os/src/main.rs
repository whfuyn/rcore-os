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

#[no_mangle]
pub extern "C" fn what_the_fuck() {
    println!("what the fuck");
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

// #[no_mangle]
// pub fn rust_main() {
//     println!("hello");
//     init();
//     task::run_first_task();
// }



#[no_mangle]
pub extern "C" fn rust_main(kernel_pa: PhysAddr, kernel_size: usize) {
    init();
    mm::heap_allocator::init();
    let kernel_pa_end = PhysAddr::new(kernel_pa.0 + kernel_size + PAGE_SIZE - 1).ppn();
    let memory_pa_end = PhysAddr::new(QEMU_MEMORY_END).ppn();
    println!("kernel_pa_end: 0x{:x}", kernel_pa_end.0);
    mm::frame_allocator::init(kernel_pa_end, memory_pa_end);

    println!("va: \n0x{:x}\n0b{:b}", KERNEL_BASE_ADDRESS.0, KERNEL_BASE_ADDRESS.0);
    println!("va: \n0x{:x}\n0b{:b}", KERNEL_BASE_ADDRESS.0 + kernel_size, KERNEL_BASE_ADDRESS.0 + kernel_size);
    println!("kernel base addr: {:x}", KERNEL_BASE_ADDRESS.0);
    println!("kernel size: {:x}", kernel_size);
    println!("kernel va end: {:x}", KERNEL_BASE_ADDRESS.0 + kernel_size + PAGE_SIZE - 1);
    let kernel_va_end = VirtAddr::new(KERNEL_BASE_ADDRESS.0 + kernel_size + PAGE_SIZE - 1).vpn();
    // let kernel_avail_va = VPN(kernel_va_end.0 + 1);
    let kernel_avail_va = VPN(kernel_va_end.0 + 20);
    let kbtb = PPN(satp::read().ppn());
    println!("kbtb: 0x{:x}", kbtb.0);
    println!("kernel_va_end: 0x{:x}", kernel_va_end.0);
    println!("kernel_avail_va: 0x{:x}", kernel_avail_va.0);
    mm::address_space::init(PPN(satp::read().ppn()), kernel_avail_va);

    let mut pc = 0usize;
    unsafe {
        core::arch::asm!(
            "auipc {}, 0",
            out(reg) pc,
            options(nomem, nostack, preserves_flags)
        );
    }

    println!("pc: 0x{:x}", pc);

    println!("hello from os");
    println!("kernel pa: 0x{:x} 0x{:x}", kernel_pa.0, kernel_size);
    println!("satp: 0x{:x}", riscv::register::satp::read().bits());
    task::run_first_task();

    sbi::shutdown();
}
