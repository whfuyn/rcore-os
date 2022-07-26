use super::*;
// use lazy_static::lazy_static;
use spin::Mutex;

// lazy_static! {
//     pub static ref FRAME_ALLOCATOR: Mutex<FrameAllocator> = Mutex::new(FrameAllocator::new());
// }

pub static FRAME_ALLOCATOR: Mutex<FrameAllocator> = Mutex::new(FrameAllocator::new());


pub struct FrameAllocator {
    ppn_start: PPN,
    ppn_end: PPN,
}

impl FrameAllocator {
    pub const fn new() -> Self {
        Self { ppn_start: PPN(0), ppn_end: PPN(0) }
    }

    pub fn init(&mut self, ppn_start: PPN, ppn_end: PPN) {
        self.ppn_start = ppn_start;
        self.ppn_end = ppn_end;
    }

    pub fn alloc(&mut self) -> Option<PPN> {
        if self.ppn_start.0 < self.ppn_end.0 {
            let ppn = self.ppn_start;
            self.ppn_start.0 += 1;
            Some(ppn)
        } else {
            None
        }
    }

    pub fn free(&mut self) {
        // TODO
    }
}

pub fn frame_alloc() -> PPN {
    FRAME_ALLOCATOR.lock().alloc().expect("We run out of physical page frame. QAQ")
}

pub fn frame_free(_ppn: PPN) {
    // TODO
}
