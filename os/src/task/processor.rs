use super::TaskControlBlock;
use spin::Mutex;
use core::option::Option;
use alloc::sync::Arc;

pub static PROCESSOR: Mutex<Processor> = Mutex::new(Processor{ current: None });

pub struct Processor {
    current: Option<Arc<TaskControlBlock>>,
}

impl Processor {
    pub fn take_current(&mut self) -> Option<Arc<TaskControlBlock>> {
        // crate::println!("current taken");
        self.current.take()
    }

    pub fn current(&self) -> Option<Arc<TaskControlBlock>> {
        self.current.as_ref().map(Arc::clone)
    }

    pub fn set_current(&mut self, task: Arc<TaskControlBlock>) -> Option<Arc<TaskControlBlock>> {
        // crate::println!("current set");
        self.current.replace(task)
    }
}