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

const QEMU_MEMORY_START: usize = 0x80000000;

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
    println!("loader: 0x{:x} - 0x{:x}", sloader as usize, eloader as usize);
    println!("kernel: 0x{:x} - 0x{:x}", spacked_kernel as usize, epacked_kernel as usize);

    // Build identity mapping for physical memory using 1GiB huge page.
    let memory_va = VirtAddr::new(QEMU_MEMORY_START);
    let memory_pa = PhysAddr::new(QEMU_MEMORY_START);
    let mut memory_ppn = memory_pa.ppn();
    memory_ppn.set_level(0, 0);
    memory_ppn.set_level(1, 0);
    let memory_pte = PageTableEntry::leaf(
        memory_ppn, 
        PteFlags::kernel_leaf()
    );

    unsafe {
        KERNEL_ROOT_PAGE_TABLE.set_entry(memory_va.vpn().level(2), memory_pte);
    }

    let kernel_pa = PhysAddr::new(spacked_kernel as usize);
    let kernel_size = (epacked_kernel as usize).checked_sub(spacked_kernel as usize)
        .expect("invalid epacked_kernel")
        // This is for .bss which isn't included in spacked_kernel..epacked_kernel.
        + 6 * 1024 * 1024;
    build_sub_page_mapping(
        unsafe { &mut KERNEL_SUB_PAGE_TABLE },
        KERNEL_BASE_ADDRESS,
        kernel_pa,
        kernel_size,
        PteFlags::kernel_leaf()
    );
    unsafe {
        let kernel_vpn = KERNEL_BASE_ADDRESS.vpn();
        let sub_table_ppn = KERNEL_SUB_PAGE_TABLE.pa().ppn();
        let pte = PageTableEntry::parent(sub_table_ppn, PteFlags::kernel_inner());
        KERNEL_ROOT_PAGE_TABLE.set_entry(kernel_vpn.level(2), pte);
    }

    unsafe {
        satp::set(Mode::Sv39, 0, KERNEL_ROOT_PAGE_TABLE.pa().ppn().as_usize());
        // Is it necessary?
        riscv::asm::sfence_vma_all();
        // Jump to kernel.
        asm!(
            // This line below will cause lld error undefined symbol `s5`, why?
            // "j {}",
            "mv a0, {}",
            "mv a1, {}",
            "mv ra, {}",
            "ret",
            in(reg) kernel_pa.0,
            in(reg) kernel_size,
            in(reg) KERNEL_BASE_ADDRESS.0
        );
    }

    unreachable!("shouldn't be here");

    // sbi::shutdown();
}

fn build_sub_page_mapping(
    page_table: &mut PageTable,
    vbase: VirtAddr, pbase: PhysAddr, size: usize, flags: PteFlags,
) {
    let mut v = vbase;
    let mut p = pbase;

    let mut mapped_size = 0;

    while mapped_size < size {
        let vpn = v.vpn();
        let mut ppn = p.ppn();
        // Huge page alignment
        ppn.set_level(0, 0);

        let leaf_pte = PageTableEntry::leaf(ppn, flags);
        unsafe {
            page_table.set_entry(vpn.level(1), leaf_pte);
        }

        const TWO_MIB: usize = 2 * 1024 * 1024;
        v.0 += TWO_MIB;
        p.0 += TWO_MIB;
        mapped_size += TWO_MIB
    }
}
