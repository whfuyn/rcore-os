use buddy_system_allocator::LockedHeap;

const INIT_HEAP_SPACE_SIZE: usize = 2 * 1024 * 1024;

#[global_allocator]
pub static KERNEL_HEAP_ALLOCATOR: LockedHeap<32> = LockedHeap::empty();

static mut INIT_HEAP_SPACE: [u8; INIT_HEAP_SPACE_SIZE] = [0; INIT_HEAP_SPACE_SIZE];

#[alloc_error_handler]
/// panic when heap allocation error occurs
pub fn handle_alloc_error(layout: core::alloc::Layout) -> ! {
    panic!("Heap allocation error, layout = {:?}", layout);
}

pub fn init() {
    unsafe {
        let heap_start = &mut INIT_HEAP_SPACE  as *mut _ as usize;
        KERNEL_HEAP_ALLOCATOR.lock()
            .init(heap_start, INIT_HEAP_SPACE_SIZE);
    }
}
