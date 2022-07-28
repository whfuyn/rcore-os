use super::*;
use lazy_static::lazy_static;
use buddy_system_allocator::LockedFrameAllocator;

lazy_static! {
    pub static ref FRAME_ALLOCATOR: LockedFrameAllocator = LockedFrameAllocator::new();
}

pub fn init(frame_start: PPN, frame_end: PPN) {
    FRAME_ALLOCATOR.lock()
        .add_frame(frame_start.0, frame_end.0);
}

pub fn frame_alloc() -> PPN {
    let frame = FRAME_ALLOCATOR.lock().alloc(1).expect("We run out of physical page frame. QAQ");
    PPN(frame)
}

pub fn frame_free(ppn: PPN) {
    FRAME_ALLOCATOR.lock().dealloc(ppn.0, 1);
}
