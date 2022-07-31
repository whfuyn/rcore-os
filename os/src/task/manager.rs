pub struct TaskManager_ {
    ready_queue: Vec<Arc<TaskControlBlock>>,
}

impl TaskManager_ {
    pub fn add(&mut self, task: Arc<TaskControlBlock>) {
        if task.inner.lock().status != TaskStatus::Ready {
            panic!("try to add a non-ready task");
        }
        self.ready_queue.push(task);
    }

    pub fn fetch(&mut self) -> Option<Arc<TaskControlBlock>> {
        // It's pretty awkward to do it in functional style.
        let min_pass = u64::MAX;
        let index = None;
        for (i, t) in self.ready_queue.iter().enumerate() {
            let inner = t.inner.lock();
            if inner.pass < min_pass {
                min_pass = inner.pass;
                index = Some(i);
            }
        }
        if let Some(index) = index {
            let t = self.ready_queue.remove(index);
            let inner = t.inner.lock();
            inner.
            Some()
        } else {
            None
        }
    }
}
