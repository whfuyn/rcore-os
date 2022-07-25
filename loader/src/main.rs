#![no_std]
#![no_main]
#![feature(format_args_nl)]

use os::*;
use os::mm::*;
use core::arch::asm;
use riscv::register::satp::{self, Mode};

core::arch::global_asm!(include_str!("entry.S"));
core::arch::global_asm!(include_str!("kernel.S"));

const KERNEL_BASE_ADDRESS: VirtAddr = unsafe {
    VirtAddr::new_unchecked(0xffffffffc0000000)
};

extern "C" {
    fn spacked_kernel();
    fn epacked_kernel();
    fn sloader();
    fn eloader();
}

static mut KERNEL_ROOT_PAGE_TABLE: PageTable = PageTable::empty();
static mut KERNEL_SUB_PAGE_TABLE: PageTable = PageTable::empty();

#[no_mangle]
fn loader_main() {
    println!("hello from loader");
    println!("kernel: 0x{:x} - 0x{:x}", spacked_kernel as usize, epacked_kernel as usize);

    // let sloader_pa = PhysAddr::new(sloader as usize);
    // let eloader_pa = PhysAddr::new(eloader as usize);
    // let loader_size = (eloader as usize).checked_sub(sloader as usize).expect("invalid eloader");

    // Build identity mapping for loader. We assume the kernel won't be too big (e.g. > 1G).
    let mut loader_va = VirtAddr::new(sloader as usize);
    let mut loader_pa = PhysAddr::new(sloader as usize);
    while loader_va.0 < eloader as usize {
        let loader_vpn = loader_va.vpn();
        let mut loader_ppn = loader_pa.ppn();
        loader_ppn.set_level(0, 0);

        let pte = PageTableEntry::leaf(
            loader_ppn, 
            PteFlags::VALID | PteFlags::READ | PteFlags::WRITE | PteFlags::EXECUTE,
        );
        
        unsafe {
            KERNEL_SUB_PAGE_TABLE.set_entry(loader_vpn.level(1), pte);
        }

        // 2 MiB
        loader_va.0 += 2 * 1024 * 1024;
        loader_pa.0 += 2 * 1024 * 1024;
    }
    unsafe {
        let loader_vpn = VirtAddr::new(sloader as usize).vpn();
        let sub_table_ppn = KERNEL_SUB_PAGE_TABLE.pa().ppn();
        let pte = PageTableEntry::parent(sub_table_ppn, PteFlags::VALID);
        KERNEL_ROOT_PAGE_TABLE.set_entry(loader_vpn.level(2), pte);
        satp::set(Mode::Sv39, 0, KERNEL_ROOT_PAGE_TABLE.pa().ppn().as_usize());
        riscv::asm::sfence_vma_all();
    }

    println!("still alive");

    sbi::shutdown();
    // unsafe {
    //     core::arch::asm!("ret");
    // }
}
