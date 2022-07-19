use lazy_static::lazy_static;
use riscv::register::sstatus::Sstatus;
use core::cell::SyncUnsafeCell;
use crate::trap::TrapContext;
use crate::sbi;
use crate::println;
use crate::trap::__restore;
use core::arch::global_asm;
use core::arch::asm;

use spin::Mutex;


const MAX_TASK_NUM: usize = 32;

const KERNEL_STACK_SIZE: usize = 4096 * 2;
const USER_STACK_SIZE: usize = 4096 * 2;

const APP_BASE_ADDR: *mut u8 = 0x80400000 as *mut u8;
const MAX_APP_SIZE: usize = 0x20000;

global_asm!(include_str!("link_app.S"));
extern "C" {
    static _app_info_table: usize;
}

global_asm!(include_str!("task/switch.S"));
extern "C" {
    fn __switch(current_cx: *mut TaskContext, next_cx: *mut TaskContext);
}

#[repr(align(4096))]
pub struct KernelStack(SyncUnsafeCell<[u8; KERNEL_STACK_SIZE]>);

#[repr(align(4096))]
pub struct UserStack(SyncUnsafeCell<[u8; USER_STACK_SIZE]>);

static KERNEL_STACK: [KernelStack ; MAX_TASK_NUM]= {
    const KERNEL_STACK: KernelStack = KernelStack(SyncUnsafeCell::new([0; KERNEL_STACK_SIZE]));
    [KERNEL_STACK; MAX_TASK_NUM]
};
static USER_STACK: [UserStack; MAX_TASK_NUM] = {
    const USER_STACK: UserStack = UserStack(SyncUnsafeCell::new([0; USER_STACK_SIZE]));
    [USER_STACK; MAX_TASK_NUM]
};

impl KernelStack {
    pub fn get_sp(&self) -> usize {
        unsafe {
            let stack = self.0.get();
            let len = (*stack).len() as isize;
            (stack as *mut u8).offset(len) as usize
        }
    }

    pub fn push_context(&self, cx: TrapContext) -> usize {
        unsafe {
            let sp = (self.get_sp() as *mut u8)
                .offset(-(core::mem::size_of::<TrapContext>() as isize));
            (sp as *mut TrapContext).write(cx);
            sp as usize
        }
    }
}

impl UserStack {
    pub fn get_sp(&self) -> *mut u8 {
        unsafe {
            let stack = self.0.get();
            let len = (*stack).len() as isize;
            (stack as *mut u8).offset(len)
        }
    }
}

lazy_static! {
    pub static ref TASK_MANAGER: Mutex<TaskManager> = Mutex::new(unsafe { TaskManager::new() });
}

#[derive(Default, Debug, Clone, Copy, PartialEq, Eq)]
pub enum TaskStatus {
    #[default]
    Running,
    Exited,
}


#[derive(Debug, Clone, Default)]
#[repr(C)]
pub struct TaskContext {
    ra: usize,
    sp: usize,
    s0_11: [usize; 12],
}

#[derive(Debug, Clone, Default)]
pub struct TaskControlBlock {
    pub status: TaskStatus,
    pub cx: TaskContext,
}

pub struct TaskManager {
    app_starts: &'static [usize],
    num_app: usize,
    current_task: usize,
    tcbs: [TaskControlBlock; MAX_TASK_NUM],
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

        let mut tcbs: [TaskControlBlock; MAX_TASK_NUM] = Default::default();

        tcbs.iter_mut()
            .enumerate()
            .for_each(|(i, tcb)| {
                if i < num_app {
                    tcb.status = TaskStatus::Running;
                    tcb.cx.sp = KERNEL_STACK[i].get_sp() as usize;
                    tcb.cx.ra = start_task as usize;
                } else {
                    tcb.status = TaskStatus::Exited;
                }
            });

        Self {
            app_starts,
            num_app,
            current_task: 0,
            tcbs,
        }
    }

    pub unsafe fn load_task(&self, task_id: usize) {
        let task_start = self.app_starts[task_id];
        let task_end = self.app_starts[task_id + 1];
        let task_size = task_end.saturating_sub(task_start);

        let load_to = get_task_base(task_id);
        println!("task `{task_id}` loaded at `0x{:x}`", load_to as usize);
        core::ptr::copy_nonoverlapping(task_start as *const u8, load_to, task_size);

        asm!("fence.i");
    }

    pub fn find_next_task(&self) -> Option<usize> {
        let mut idx = (self.current_task + 1) % self.num_app;
        for _ in 0..self.num_app {
            if self.tcbs[idx].status == TaskStatus::Running {
                return Some(idx);
            }
            idx = (idx + 1) % self.num_app;
        }
        None
    }

    pub fn find_next_task_or_exit(&self) -> usize {
        self.find_next_task().unwrap_or_else(|| {
            println!("[kernel] All apps have completed.");
            sbi::shutdown();
        })
    }
}

pub unsafe extern "C" fn start_task() {
    println!("start task");
    let task_mgr = TASK_MANAGER.lock();
    let current_task = task_mgr.current_task;
    task_mgr.load_task(current_task);
    let task_entry = get_task_base(current_task);
    drop(task_mgr);

    let mut task_init_trap_cx = TrapContext::app_init_context(
        task_entry as usize, USER_STACK[current_task].get_sp() as usize
    );

    // We are already in our kernel stack. Don't need to push context to kernel stack.
    __restore(
        &mut task_init_trap_cx as *mut TrapContext as usize
    );
}

pub fn exit_task_and_run_next() {
    let mut task_mgr = TASK_MANAGER.lock();

    let current_task = task_mgr.current_task;
    println!("task `{current_task}` exited");
    let current_tcb = &mut task_mgr.tcbs[current_task];
    current_tcb.status = TaskStatus::Exited;
    drop(task_mgr);
    run_next_task();
}

pub fn start() {
    let mut task_mgr = TASK_MANAGER.lock();
    let first_task = task_mgr.find_next_task_or_exit();
    task_mgr.current_task = first_task;
    let first_task_cx = &mut task_mgr.tcbs[first_task].cx as *mut TaskContext;

    drop(task_mgr);
    let mut _unused = TaskContext::default();
    unsafe {
        __switch(&mut _unused, first_task_cx);
    }
}

pub fn run_next_task() {
    let mut task_mgr = TASK_MANAGER.lock();

    let current_task = task_mgr.current_task;
    let current_tcb = &mut task_mgr.tcbs[current_task];
    let current_task_cx = &mut current_tcb.cx as *mut TaskContext;

    let next_task = task_mgr.find_next_task_or_exit();
    let next_tcb = &mut task_mgr.tcbs[next_task];
    let next_task_cx = &mut next_tcb.cx as *mut TaskContext;

    task_mgr.current_task = next_task;

    drop(task_mgr);
    unsafe {
        __switch(current_task_cx, next_task_cx);
    }
}

fn get_task_base(task_id: usize) -> *mut u8 {
    unsafe {
        APP_BASE_ADDR.add(task_id * MAX_APP_SIZE)
    }
}
