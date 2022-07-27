use crate::mm::*;
use buddy_system_allocator::LockedHeap;
use spin::Mutex;

const INIT_HEAP_SPACE_SIZE: usize = 4096 * 10;

#[global_allocator]
pub static KERNEL_HEAP_ALLOCATOR: LockedHeap<32> = LockedHeap::empty();

static mut INIT_HEAP_SPACE: [u8; INIT_HEAP_SPACE_SIZE] = [0; INIT_HEAP_SPACE_SIZE];

// pub static KERNEL_BRK: Mutex<VirtAddr> = Mutex::new(unsafe { VirtAddr::new_unchecked(0) });

pub fn init() {
    unsafe {
        let heap_start = &mut INIT_HEAP_SPACE  as *mut _ as usize;
        KERNEL_HEAP_ALLOCATOR.lock()
            .init(heap_start, INIT_HEAP_SPACE_SIZE);
    }
    // *KERNEL_BRK.lock() = init_brk;
}
