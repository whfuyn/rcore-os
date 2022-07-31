use alloc::vec::Vec;
use spin::Mutex;

static PID_ALLOCATOR: Mutex<PidAllocator> = Mutex::new(PidAllocator::new());

pub fn pid_alloc() -> Pid {
    PID_ALLOCATOR.lock().alloc()
}

#[derive(Debug)]
pub struct Pid(pub usize);

impl Drop for Pid {
    fn drop(&mut self) {
        PID_ALLOCATOR.lock().free(self.0)
    }
}

pub struct PidAllocator {
    current: usize,
    recycled: Vec<usize>,
}

impl PidAllocator {
    const fn new() -> Self {
        Self { current: 1, recycled: Vec::new() }
    }

    fn alloc(&mut self) -> Pid {
        if let Some(pid) = self.recycled.pop() {
            Pid(pid)
        } else {
            let pid = self.current;
            self.current += 1;
            Pid(pid)
        }
    }

    fn free(&mut self, pid: usize) {
        self.recycled.push(pid);
    }
}
