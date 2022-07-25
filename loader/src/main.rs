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
static mut LOADER_SUB_PAGE_TABLE: PageTable = PageTable::empty();

#[no_mangle]
fn loader_main() {
    println!("hello from loader");
    println!("loader: 0x{:x} - 0x{:x}", sloader as usize, eloader as usize);
    println!("kernel: 0x{:x} - 0x{:x}", spacked_kernel as usize, epacked_kernel as usize);

    // let sloader_pa = PhysAddr::new(sloader as usize);
    // let eloader_pa = PhysAddr::new(eloader as usize);

    // Build identity mapping for loader. We assume the kernel won't be too big (e.g. > 1G).
    let loader_va = VirtAddr::new(sloader as usize);
    let loader_pa = PhysAddr::new(sloader as usize);
    let loader_size = (eloader as usize).checked_sub(sloader as usize).expect("invalid eloader");
    build_sub_page_mapping(
        unsafe { &mut LOADER_SUB_PAGE_TABLE },
        loader_va, 
        loader_pa,
        loader_size,
        PteFlags::VALID | PteFlags::READ | PteFlags::WRITE | PteFlags::EXECUTE,
    );
    unsafe {
        let loader_vpn = VirtAddr::new(sloader as usize).vpn();
        let sub_table_ppn = LOADER_SUB_PAGE_TABLE.pa().ppn();
        let pte = PageTableEntry::parent(sub_table_ppn, PteFlags::VALID);
        KERNEL_ROOT_PAGE_TABLE.set_entry(loader_vpn.level(2), pte);
    }

    let kernel_pa = PhysAddr::new(spacked_kernel as usize);
    let kernel_size = (epacked_kernel as usize).checked_sub(spacked_kernel as usize)
        .expect("invalid epacked_kernel");
    build_sub_page_mapping(
        unsafe { &mut KERNEL_SUB_PAGE_TABLE },
        KERNEL_BASE_ADDRESS,
        kernel_pa,
        // + bss for kernel
        kernel_size + 6 * 1024 * 1024,
        PteFlags::VALID
            | PteFlags::READ | PteFlags::WRITE | PteFlags::EXECUTE
            | PteFlags::GLOBAL | PteFlags::DIRTY | PteFlags::ACCESS,
    );
    unsafe {
        let kernel_vpn = KERNEL_BASE_ADDRESS.vpn();
        let sub_table_ppn = KERNEL_SUB_PAGE_TABLE.pa().ppn();
        let pte = PageTableEntry::parent(sub_table_ppn, PteFlags::VALID | PteFlags::GLOBAL);
        KERNEL_ROOT_PAGE_TABLE.set_entry(kernel_vpn.level(2), pte);
    }

    unsafe {
        satp::set(Mode::Sv39, 0, KERNEL_ROOT_PAGE_TABLE.pa().ppn().as_usize());
        riscv::asm::sfence_vma_all();
        asm!(
            "mv ra, {}",
            "ret",
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
