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
// use crate::mm::*;
// use crate::config::*;
use alloc::sync::Arc;
use alloc::sync::Weak;
use pid::{ Pid, pid_alloc };
pub use processor::PROCESSOR;
use crate::trap::TRAP_CX_VA;
use crate::trap::TrapContext;
// use crate::config::*;

pub use stack::KernelStack;
use crate::sbi;
// use crate::println;
use crate::trap::__restore;
use crate::time;
use crate::syscall::MAX_SYSCALL_NUM;
use crate::mm::address_space::AddressSpace;
pub use elf_loader::get_app_data;


// global_asm!(include_str!("link_app.S"));
// extern "C" {
//     static _num_app: usize;
// }

const STRIDE: u64 = 10007;

global_asm!(include_str!("task/switch.S"));
extern "C" {
    fn __switch(current_cx: *mut TaskContext, next_cx: *const TaskContext);
}


lazy_static! {
    pub static ref TASK_MANAGER: Mutex<TaskManager> = Mutex::new(TaskManager::new());
    pub static ref INITPROC: Arc<TaskControlBlock> = {
        let initproc_elf = get_app_data("ch5b_initproc").expect("missing initproc");
        TaskControlBlock::load_from_elf(initproc_elf, None)
    };
}

#[repr(C)]
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum TaskStatus {
    #[default]
    Ready = 0,
    Running = 1,
    // Exited = 3,
    Zombie = 2,
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

    pub children: Vec<Arc<TaskControlBlock>>,
    pub parent: Option<Weak<TaskControlBlock>>,
    pub exit_code: i32,

    // Stride scheduling
    pub priority: u64,
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
    pub pid: Pid,
    inner: Mutex<TaskControlBlockInner>,
}

impl TaskControlBlock {
    pub fn load_from_elf(elf_data: &[u8], parent: Option<Weak<TaskControlBlock>>) -> Arc<Self> {
        let pid = pid_alloc();
        let addr_space = AddressSpace::from_elf(elf_data, pid.0);
        let mut cx = TaskContext::default();
        cx.sp = TRAP_CX_VA.0;
        cx.ra = __restore as usize;
        cx.satp = addr_space.satp();
        // crate::println!("satp: 0x{:x}", cx.satp);

        let inner = TaskControlBlockInner {
            status: TaskStatus::Ready,
            cx,
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

    pub fn is_zombie(&self) -> bool {
        self.lock().status == TaskStatus::Zombie
    }

    pub fn is_ready(&self) -> bool {
        self.lock().status == TaskStatus::Ready
    }

    pub fn exec(self: Arc<Self>, elf_data: &[u8]) {
        let mut inner = self.lock();
        inner.schedule_end();

        let addr_space = AddressSpace::from_elf(elf_data, self.pid.0);
        inner.cx.sp = TRAP_CX_VA.0;
        inner.cx.ra = __restore as usize;
        inner.cx.satp = addr_space.satp();
        inner.addr_space = addr_space;

        let cx = inner.schedule_begin();

        drop(inner);
        drop(self);
        let mut unused = TaskContext::default();
        unsafe {
            __switch(&mut unused, cx);
        }
    }

    pub fn fork(self: &Arc<Self>) -> usize {
        let child_pid = pid_alloc();
        let mut parent_inner = self.lock();

        let child_addr_space = parent_inner.addr_space.dup(child_pid.0);
        let mut child_cx = TaskContext::default();
        child_cx.sp = TRAP_CX_VA.0;
        child_cx.ra = __restore as usize;
        child_cx.satp = child_addr_space.satp();

        let child_trap_cx = child_addr_space.translate(TRAP_CX_VA).expect("failed to translate trap cx va").0 as *mut TrapContext;
        unsafe {
            // return 0 for syscall fork
            (*child_trap_cx).x[10] = 0;
        }

        let child_inner = TaskControlBlockInner {
            status: TaskStatus::Ready,
            cx: child_cx,
            addr_space: child_addr_space,
            parent: Some(Arc::downgrade(self)),
            children: Vec::new(),
            exit_code: 0,
            stats: TaskStat::default(),
            priority: 16,
            pass: 0,
        };
        let ret = child_pid.0;
        let child = Arc::new(TaskControlBlock {
            pid: child_pid,
            inner: Mutex::new(child_inner)
        });

        parent_inner.children.push(Arc::clone(&child));
        TASK_MANAGER.lock().add(child);

        ret
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

// fn finish() -> ! {
//     println!("[kernel] All apps have completed.");
//     sbi::shutdown();
// }

pub fn set_next_trigger() {
    const TICKS_PER_SEC: usize = 100;
    let current_time = time::get_time();
    let delta = time::CLOCK_FREQ / TICKS_PER_SEC;
    // crate::println!("set timer to {}", current_time + delta);
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
    let processor = PROCESSOR.lock();
    let current_task = processor.current().expect("missing current task");
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
    drop(initproc);
    drop(processor);

    run_next_task();
}

pub fn run_next_task() {
    // crate::println!("run next");
    let mut processor = PROCESSOR.lock();
    let current_task = processor.take_current().expect("missing current task");
    let current_cx = current_task.inner.lock().schedule_end();

    let mut task_mgr = TASK_MANAGER.lock();
    if current_task.is_ready() {
        task_mgr.add(current_task);
    } else {
        drop(current_task);
    }
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
