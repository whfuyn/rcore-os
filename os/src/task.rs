mod stack;

use lazy_static::lazy_static;
use core::arch::global_asm;
use spin::Mutex;
use alloc::vec::Vec;
use crate::mm::*;

pub use stack::KernelStack;
use crate::sbi;
use crate::println;
use crate::trap::__restore;
use crate::time;
use crate::syscall::MAX_SYSCALL_NUM;
use crate::mm::address_space::AddressSpace;


global_asm!(include_str!("link_app.S"));
extern "C" {
    static _app_info_table: usize;
}

global_asm!(include_str!("task/switch.S"));
extern "C" {
    fn __switch(current_cx: *mut TaskContext, next_cx: *mut TaskContext);
}


lazy_static! {
    pub static ref TASK_MANAGER: Mutex<TaskManager> = Mutex::new(unsafe { TaskManager::new() });
}

#[repr(C)]
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum TaskStatus {
    #[default]
    UnInit = 0,
    Ready = 1,
    Running = 2,
    Exited = 3,
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

#[derive(Debug, Clone, Default)]
pub struct TaskControlBlock {
    pub status: TaskStatus,
    cx: TaskContext,
}

pub struct TaskManager {
    app_starts: &'static [usize],
    num_app: usize,
    current_task: usize,
    tcbs: Vec<TaskControlBlock>,
    pub addr_spaces: Vec<AddressSpace>,
    stats: Vec<TaskStat>,
}

impl TaskManager {
    pub unsafe fn new() -> Self {
        let ptr = &_app_info_table as *const usize;
        let num_app = *ptr;
        let app_starts = {
            let table = ptr.add(1);
            // The last one is a marker for the end.
            core::slice::from_raw_parts(table, num_app + 1)
        };

        let mut task_mgr = Self {
            app_starts,
            num_app,
            current_task: 0,
            tcbs: Vec::new(),
            addr_spaces: Vec::new(),
            stats: Vec::new(),
        };

        for i in 0..num_app {
            task_mgr.load_task(i);
        }

        task_mgr
    }

    pub unsafe fn load_task(&mut self, task_id: usize) {
        let task_start = self.app_starts[task_id];
        let task_end = self.app_starts[task_id + 1];
        let task_size = task_end.saturating_sub(task_start);
        let task_data = core::slice::from_raw_parts(task_start as *const u8, task_size);

        let (addr_space, ksp) = AddressSpace::from_elf(task_data, task_id + 2);

        let mut tcb = TaskControlBlock::default();
        tcb.cx.sp = ksp;
        tcb.cx.ra = __restore as usize;
        tcb.cx.satp = addr_space.satp();

        self.tcbs.push(tcb);
        self.addr_spaces.push(addr_space);
        self.stats.push(TaskStat::default());

        self.tcbs[task_id].status = TaskStatus::Ready;
    }

    /// Return current task cx and next task cx
    pub unsafe fn move_to_next_task(&mut self, next_task: usize) -> (*mut TaskContext, *mut TaskContext) {
        let current_task = self.current_task;
        let current_tcb = &mut self.tcbs[current_task];
        let current_task_cx = &mut current_tcb.cx as *mut TaskContext;
        if current_tcb.status == TaskStatus::Running {
            current_tcb.status = TaskStatus::Ready;
        }
        self.stats[current_task].record_schedule_end();

        let next_tcb = &mut self.tcbs[next_task];
        let next_task_cx = &mut next_tcb.cx as *mut TaskContext;
        assert!(next_tcb.status == TaskStatus::Ready);
        next_tcb.status = TaskStatus::Running;
        self.stats[next_task].record_schedule_begin();

        self.current_task = next_task;

        (current_task_cx, next_task_cx)
    }

    pub fn find_next_task(&self) -> Option<usize> {
        let mut idx = (self.current_task + 1) % self.num_app;
        for _ in 0..self.num_app {
            if self.tcbs[idx].status == TaskStatus::Ready {
                return Some(idx);
            }
            idx = (idx + 1) % self.num_app;
        }
        if self.tcbs[self.current_task].status == TaskStatus::Running {
            return Some(self.current_task);
        }
        None
    }

    pub fn find_next_task_or_exit(&self) -> usize {
        self.find_next_task().unwrap_or_else(|| finish())
    }

    pub fn current_task(&self) -> usize {
        self.current_task
    }

    pub fn current_stat(&self) -> &TaskStat {
        &self.stats[self.current_task]
    }

    pub fn current_tcb(&self) -> &TaskControlBlock {
        &self.tcbs[self.current_task]
    }

    // pub fn current_stat(&mut self) -> &mut TaskStat {
    //     &mut self.stats[self.current_task]
    // }

    // pub fn current_tcb(&mut self) -> &mut TaskControlBlock {
    //     &mut self.tcbs[self.current_task]
    // }

    // pub fn mut_current_stat(&mut self) -> &mut TaskStat {
    //     &mut self.stats[self.current_task]
    // }

    // pub fn mut_current_tcb(&mut self) -> &mut TaskControlBlock {
    //     &mut self.tcbs[self.current_task]
    // }
}

pub fn exit_and_run_next() {
    let mut task_mgr = TASK_MANAGER.lock();

    let current_task = task_mgr.current_task;
    let current_tcb = &mut task_mgr.tcbs[current_task];
    current_tcb.status = TaskStatus::Exited;
    drop(task_mgr);
    run_next_task();
}

pub fn run_first_task() {
    let mut task_mgr = TASK_MANAGER.lock();

    let first_task = if task_mgr.num_app > 0 { 0 } else { finish() };
    let (_, first_task_cx) = unsafe { task_mgr.move_to_next_task(first_task) };

    drop(task_mgr);

    set_next_trigger();
    let mut unused = TaskContext::default();
    unsafe {
        __switch(&mut unused, first_task_cx);
    }
}

pub fn run_next_task() {
    let mut task_mgr = TASK_MANAGER.lock();
    let next_task = task_mgr.find_next_task_or_exit();
    let (current_task_cx, next_task_cx) = unsafe { task_mgr.move_to_next_task(next_task) };
    drop(task_mgr);

    set_next_trigger();
    unsafe {
        __switch(current_task_cx, next_task_cx);
    }
}

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
    let mut task_mgr = TASK_MANAGER.lock();
    let curent_task = task_mgr.current_task;
    task_mgr.stats[curent_task].record_syscall(syscall);
}
