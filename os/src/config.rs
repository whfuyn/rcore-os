use crate::mm::*;

pub const KERNEL_BASE_ADDRESS: VirtAddr = unsafe {
    VirtAddr::new_unchecked(0xffffffffc0000000)
};

pub const PAGE_SIZE: usize = 4096;
pub const QEMU_MEMORY_START: usize = 0x80000000;
pub const QEMU_MEMORY_END: usize = 0x88000000;

