use crate::mm::*;

pub const KERNEL_BASE_ADDRESS: VirtAddr = unsafe {
    // Last 1GB
    VirtAddr::new_unchecked(0xffffffffc0000000)
};

pub const PAGE_SIZE: usize = 4096;
pub const QEMU_MEMORY_START: usize = 0x80000000;
pub const QEMU_MEMORY_END: usize = 0x88000000;

pub const KERNEL_STACK_VA: VirtAddr = unsafe {
    VirtAddr::new_unchecked(0xffffffff80000000)
};

pub const KERNEL_BRK_VA: VirtAddr = unsafe {
    VirtAddr::new_unchecked(0xffffffff40000000)
};

pub const USER_STACK_VA: VirtAddr = unsafe {
    VirtAddr::new_unchecked(0x70000000)
};
