mod stack;
mod pid;
mod elf_loader;
mod processor;

use lazy_static::lazy_static;
use core::arch::global_asm;
use spin::Mutex;
use spin::MutexGuard;
use alloc::vec::Vec;
// use alloc::collections::BinaryHeap;
use crate::mm::*;
use crate::config::*;
use alloc::sync::Arc;
use alloc::sync::Weak;
use pid::{ Pid, pid_alloc };
pub use processor::PROCESSOR;

pub use stack::KernelStack;
use crate::sbi;
use crate::println;
use crate::trap::__restore;
use crate::time;
use crate::syscall::MAX_SYSCALL_NUM;
use crate::mm::address_space::AddressSpace;
use elf_loader::get_app_data;


// global_asm!(include_str!("link_app.S"));
// extern "C" {
//     static _num_app: usize;
// }

const STRIDE: u64 = 11234567;

global_asm!(include_str!("task/switch.S"));
extern "C" {
    fn __switch(current_cx: *mut TaskContext, next_cx: *const TaskContext);
}


lazy_static! {
    pub static ref TASK_MANAGER: Mutex<TaskManager> = Mutex::new(unsafe { TaskManager::new() });
    pub static ref INITPROC: Arc<TaskControlBlock> = {
        let initproc_elf = get_app_data("initproc").expect("missing initproc");
        TaskControlBlock::load_from_elf(initproc_elf, None)
    };
}

#[repr(C)]
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum TaskStatus {
    #[default]
    UnInit = 0,
    Ready = 1,
    Running = 2,
    // Exited = 3,
    Zombie = 3,
}

#[derive(Debug, Clone, Default)]
#[repr(C)]
pub struct TaskContext {
    ra: usize,
    sp: usize,
    satp: usize,
    s0_11: [usize; 12],
}

#[derive(Debug, Clone)]
pub struct TaskStat {
    pub cpu_clocks: usize,
    pub first_scheduled: Option<usize>,
    pub last_scheduled: Option<usize>,
    pub syscall_times: [u32; MAX_SYSCALL_NUM],
}

impl TaskStat {
    pub fn record_schedule_begin(&mut self) {
        if self.last_scheduled.is_none() {
            self.first_scheduled = Some(time::get_time());
            self.last_scheduled = self.first_scheduled;
        } else {
            self.last_scheduled = Some(time::get_time());
        }
    }

    pub fn record_schedule_end(&mut self) {
        if let Some(last_scheduled) = self.last_scheduled {
            self.cpu_clocks += time::get_time().checked_sub(last_scheduled).expect("time goes backward");
        }
    }

    pub fn record_syscall(&mut self, syscall: usize) {
        self.syscall_times[syscall] += 1;
    }

    pub fn real_time(&self) -> usize {
        if let Some(first_scheduled) = self.first_scheduled {
            time::get_time().checked_sub(first_scheduled).expect("time goes backward")
        } else {
            0
        }
    }
}

impl Default for TaskStat {
    fn default() -> Self {
        Self {
            cpu_clocks: 0, 
            first_scheduled: None,
            last_scheduled: None,
            syscall_times: [0; MAX_SYSCALL_NUM],
        }
    }
}

#[derive(Debug)]
pub struct TaskControlBlockInner {
    pub status: TaskStatus,
    cx: TaskContext,
    // TODO: no pub
    pub addr_space: AddressSpace,
    pub stats: TaskStat,

    children: Vec<Arc<TaskControlBlock>>,
    parent: Option<Weak<TaskControlBlock>>,
    exit_code: i32,

    // Stride scheduling
    priority: u64,
    pass: u64,
}

impl TaskControlBlockInner {
    fn schedule_begin(&mut self) -> *const TaskContext {
        assert_eq!(self.status, TaskStatus::Ready);
        self.status = TaskStatus::Running;
        self.stats.record_schedule_begin();

        &self.cx as *const TaskContext
    }

    fn schedule_end(&mut self) -> *mut TaskContext {
        assert!(matches!(self.status, TaskStatus::Running | TaskStatus::Zombie));
        self.stats.record_schedule_end();
        if self.status == TaskStatus::Running {
            self.status = TaskStatus::Ready;
        }
        self.pass += STRIDE / self.priority;

        &mut self.cx as *mut TaskContext
    }
}

#[derive(Debug)]
pub struct TaskControlBlock {
    pid: Pid,
    inner: Mutex<TaskControlBlockInner>,
}

impl TaskControlBlock {
    fn load_from_elf(elf_data: &[u8], parent: Option<Weak<TaskControlBlock>>) -> Arc<Self> {
        let pid = pid_alloc();
        // let elf_data = get_app_data(elf_name).expect("todo");
        let (addr_space, ksp) = AddressSpace::from_elf(elf_data, pid.0);
        let mut cx = TaskContext::default();
        cx.sp = ksp;
        cx.ra = __restore as usize;
        cx.satp = addr_space.satp();

        let inner = TaskControlBlockInner {
            status: TaskStatus::Ready,
            cx: TaskContext::default(),
            addr_space,
            stats: TaskStat::default(),
            children: Vec::new(),
            parent,
            exit_code: 0,
            priority: 16,
            pass: 0,
        };
        Arc::new(Self {
            pid,
            inner: Mutex::new(inner),
        })
    }

    pub fn lock<'a>(&'a self) -> MutexGuard<'a, TaskControlBlockInner> {
        self.inner.lock()
    }
}

pub struct TaskManager {
    ready_queue: Vec<Arc<TaskControlBlock>>,
}

impl TaskManager {
    pub const fn new() -> Self {
        Self { ready_queue: Vec::new() }
    }

    pub fn add(&mut self, task: Arc<TaskControlBlock>) {
        if task.inner.lock().status != TaskStatus::Ready {
            panic!("try to add a non-ready task");
        }
        self.ready_queue.push(task);
    }

    pub fn fetch(&mut self) -> Option<Arc<TaskControlBlock>> {
        // It's pretty awkward to do it in functional style.
        let mut min_pass = u64::MAX;
        let mut index = None;
        for (i, t) in self.ready_queue.iter().enumerate() {
            let inner = t.inner.lock();
            if inner.pass < min_pass {
                min_pass = inner.pass;
                index = Some(i);
            }
        }
        if let Some(index) = index {
            let task = self.ready_queue.remove(index);
            Some(task)
        } else {
            None
        }
    }
}

// pub struct TaskManager_ {
//     app_starts: &'static [usize],
//     num_app: usize,
//     current_task: usize,
//     tcbs: Vec<Arc<Mutex<TaskControlBlockInner>>>,
//     // pub addr_spaces: Vec<AddressSpace>,
//     // stats: Vec<TaskStat>,
// }

// impl TaskManager_ {
//     pub unsafe fn new() -> Self {
//         let ptr = &_num_app as *const usize;
//         let num_app = *ptr;
//         let app_starts = {
//             let table = ptr.add(1);
//             // The last one is a marker for the end.
//             core::slice::from_raw_parts(table, num_app + 1)
//         };

//         let mut task_mgr = Self {
//             app_starts,
//             num_app,
//             current_task: 0,
//             tcbs: Vec::new(),
//             // addr_spaces: Vec::new(),
//             // stats: Vec::new(),
//         };

//         for i in 0..num_app {
//             task_mgr.load_task(i);
//         }

//         task_mgr
//     }

//     pub unsafe fn load_task(&mut self, task_id: usize) {
//         let task_start = self.app_starts[task_id];
//         let task_end = self.app_starts[task_id + 1];
//         let task_size = task_end.saturating_sub(task_start);
//         let task_data = core::slice::from_raw_parts(task_start as *const u8, task_size);

//         let (addr_space, ksp) = AddressSpace::from_elf(task_data, task_id + 2);

//         let mut tcb = TaskControlBlockInner {
//             status: TaskStatus::Ready,
//             cx: TaskContext::default(),
//             addr_space,
//             stats: TaskStat::default(), 
//         };
//         tcb.cx.sp = ksp;
//         tcb.cx.ra = __restore as usize;
//         tcb.cx.satp = tcb.addr_space.satp();

//         self.tcbs.push(Arc::new(Mutex::new(tcb)));
//     }

//     /// Return current task cx and next task cx
//     pub unsafe fn move_to_next_task(&mut self, next_task: usize) -> (*mut TaskContext, *mut TaskContext) {
//         let current_task = self.current_task;
//         let current_tcb = &mut self.tcbs[current_task];
//         let current_task_cx = &mut current_tcb.cx as *mut TaskContext;
//         if current_tcb.status == TaskStatus::Running {
//             current_tcb.status = TaskStatus::Ready;
//         }
//         self.stats[current_task].record_schedule_end();

//         let next_tcb = &mut self.tcbs[next_task];
//         let next_task_cx = &mut next_tcb.cx as *mut TaskContext;
//         assert!(next_tcb.status == TaskStatus::Ready);
//         next_tcb.status = TaskStatus::Running;
//         self.stats[next_task].record_schedule_begin();

//         self.current_task = next_task;

//         (current_task_cx, next_task_cx)
//     }

//     pub fn find_next_task(&self) -> Option<usize> {
//         let mut idx = (self.current_task + 1) % self.num_app;
//         for _ in 0..self.num_app {
//             if self.tcbs[idx].status == TaskStatus::Ready {
//                 return Some(idx);
//             }
//             idx = (idx + 1) % self.num_app;
//         }
//         if self.tcbs[self.current_task].status == TaskStatus::Running {
//             return Some(self.current_task);
//         }
//         None
//     }

//     pub fn find_next_task_or_exit(&self) -> usize {
//         self.find_next_task().unwrap_or_else(|| finish())
//     }

//     pub fn current_task(&self) -> usize {
//         self.current_task
//     }

//     pub fn current_stat(&self) -> &TaskStat {
//         &self.stats[self.current_task]
//     }

//     pub fn current_tcb(&self) -> &TaskControlBlockInner {
//         &self.tcbs[self.current_task]
//     }

//     // pub fn current_stat(&mut self) -> &mut TaskStat {
//     //     &mut self.stats[self.current_task]
//     // }

//     // pub fn current_tcb(&mut self) -> &mut TaskControlBlock {
//     //     &mut self.tcbs[self.current_task]
//     // }

//     // pub fn mut_current_stat(&mut self) -> &mut TaskStat {
//     //     &mut self.stats[self.current_task]
//     // }

//     // pub fn mut_current_tcb(&mut self) -> &mut TaskControlBlock {
//     //     &mut self.tcbs[self.current_task]
//     // }
// }

// pub fn exit_and_run_next() {
//     let mut task_mgr = TASK_MANAGER.lock();

//     let current_task = task_mgr.current_task;
//     let current_tcb = &mut task_mgr.tcbs[current_task];
//     current_tcb.status = TaskStatus::Exited;
//     drop(task_mgr);
//     run_next_task();
// }

// pub fn run_first_task() {
//     let mut task_mgr = TASK_MANAGER.lock();

//     let first_task = if task_mgr.num_app > 0 { 0 } else { finish() };
//     let (_, first_task_cx) = unsafe { task_mgr.move_to_next_task(first_task) };

//     drop(task_mgr);

//     set_next_trigger();
//     let mut unused = TaskContext::default();
//     unsafe {
//         __switch(&mut unused, first_task_cx);
//     }
// }

// pub fn run_next_task() {
//     let mut task_mgr = TASK_MANAGER.lock();
//     let next_task = task_mgr.find_next_task_or_exit();
//     let (current_task_cx, next_task_cx) = unsafe { task_mgr.move_to_next_task(next_task) };
//     drop(task_mgr);

//     set_next_trigger();
//     unsafe {
//         __switch(current_task_cx, next_task_cx);
//     }
// }

fn finish() -> ! {
    println!("[kernel] All apps have completed.");
    sbi::shutdown();
}

pub fn set_next_trigger() {
    const TICKS_PER_SEC: usize = 100;
    let current_time = time::get_time();
    let delta = time::CLOCK_FREQ / TICKS_PER_SEC;
    sbi::set_timer(current_time + delta);
}

pub fn record_syscall(syscall: usize) {
    let processor = PROCESSOR.lock();
    let current_task = processor.current().expect("missing current");
    current_task.inner.lock().stats.record_syscall(syscall);
}

pub fn run_initproc() {
    let initproc = Arc::clone(&*INITPROC);
    let initproc_cx = initproc.inner.lock().schedule_begin();

    assert!(PROCESSOR.lock().set_current(initproc).is_none());

    set_next_trigger();
    let mut unused = TaskContext::default();
    unsafe {
        __switch(&mut unused, initproc_cx);
    }
}

pub fn exit_and_run_next(exit_code: i32) {
    let mut processor = PROCESSOR.lock();
    let current_task = processor.take_current().expect("missing current task");
    let mut inner = current_task.lock();
    inner.status = TaskStatus::Zombie;
    inner.exit_code = exit_code;
    inner.schedule_end();

    // reparent to initproc
    let initproc = Arc::clone(&*INITPROC);
    let mut initproc_inner = initproc.inner.lock();
    for ch in inner.children.drain(..) {
        ch.inner.lock().parent.replace(Arc::downgrade(&initproc));
        initproc_inner.children.push(ch);
    }

    drop(inner);
    drop(initproc_inner);
    drop(processor);

    run_next_task();
}

pub fn run_next_task() {
    let mut processor = PROCESSOR.lock();
    let current_task = processor.take_current().expect("missing current task");
    let current_cx = current_task.inner.lock().schedule_end();

    let mut task_mgr = TASK_MANAGER.lock();
    task_mgr.add(current_task);
    let next_task = task_mgr.fetch().unwrap();
    let next_cx = next_task.inner.lock().schedule_begin();

    processor.set_current(next_task);

    drop(task_mgr);
    drop(processor);

    set_next_trigger();
    unsafe {
        __switch(current_cx, next_cx);
    }
}
