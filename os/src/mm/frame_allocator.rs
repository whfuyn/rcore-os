use super::*;
use lazy_static::lazy_static;
use spin::Mutex;
use buddy_system_allocator::LockedFrameAllocator;

// lazy_static! {
//     pub static ref FRAME_ALLOCATOR: Mutex<FrameAllocator> = Mutex::new(FrameAllocator::new());
// }

// pub static FRAME_ALLOCATOR: Mutex<FrameAllocator> = Mutex::new(FrameAllocator::new());

lazy_static! {
    pub static ref FRAME_ALLOCATOR: LockedFrameAllocator = LockedFrameAllocator::new();
}

pub fn init(frame_start: PPN, frame_end: PPN) {
    FRAME_ALLOCATOR.lock()
        .add_frame(frame_start.0, frame_end.0);
}

// pub struct FrameAllocator {
//     ppn_start: PPN,
//     ppn_end: PPN,
// }

// impl FrameAllocator {
//     pub const fn new() -> Self {
//         Self { ppn_start: PPN(0), ppn_end: PPN(0) }
//     }

//     pub fn init(&mut self, ppn_start: PPN, ppn_end: PPN) {
//         self.ppn_start = ppn_start;
//         self.ppn_end = ppn_end;
//     }

//     pub fn alloc(&mut self) -> Option<PPN> {
//         if self.ppn_start.0 < self.ppn_end.0 {
//             let ppn = self.ppn_start;
//             self.ppn_start.0 += 1;
//             Some(ppn)
//         } else {
//             None
//         }
//     }

//     pub fn free(&mut self) {
//         // TODO
//     }
// }

pub fn frame_alloc() -> PPN {
    let frame = FRAME_ALLOCATOR.lock().alloc(1).expect("We run out of physical page frame. QAQ");
    PPN(frame)
}

pub fn frame_free(ppn: PPN) {
    FRAME_ALLOCATOR.lock().dealloc(ppn.0, 1);
}
