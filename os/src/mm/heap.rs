
use spin::Mutex;

pub static KERNEL_HEAP: Mutex<Heap> = Mutex::new(Heap::new());

pub struct Heap {
    brk: usize,
}

impl Heap {
    const fn new() -> Self {
        Self { brk: 0 }
    }

    pub fn init(&mut self, brk: usize) {

    }
}

