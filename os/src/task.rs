use lazy_static::lazy_static;
use riscv::register::sstatus::Sstatus;
use core::cell::SyncUnsafeCell;
use crate::trap::TrapContext;
use spin::Mutex;


const MAX_TASK_NUM: usize = 32;

const KERNEL_STACK_SIZE: usize = 4096 * 2;
const USER_STACK_SIZE: usize = 4096 * 2;

core::arch::global_asm!(include_str!("task/switch.S"));
extern "C" {
    fn __switch(current_cx: &mut TaskContext, next_cx: &mut TaskContext);
}

#[repr(align(4096))]
pub struct KernelStack(SyncUnsafeCell<[u8; KERNEL_STACK_SIZE]>);

#[repr(align(4096))]
pub struct UserStack(SyncUnsafeCell<[u8; USER_STACK_SIZE]>);

static KERNEL_STACK: KernelStack = KernelStack(SyncUnsafeCell::new([0; KERNEL_STACK_SIZE]));

impl KernelStack {
    pub fn get_sp(&self) -> *mut u8 {
        unsafe {
            let stack = self.0.get();
            let len = (*stack).len() as isize;
            (stack as *mut u8).offset(len)
        }
    }

    pub fn push_context(&self, cx: TrapContext) -> *mut u8 {
        unsafe {
            let sp = self
                .get_sp()
                .offset(-(core::mem::size_of::<TrapContext>() as isize));
            (sp as *mut TrapContext).write(cx);
            sp
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
    pub static ref TASK_MANAGER: Mutex<TaskManager> = {
        todo!()
    };

}

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum TaskStatus {
    Uninit,
    Ready,
    Running,
    Exited,
}


#[derive(Clone)]
#[repr(C)]
pub struct TaskContext {
    ra: usize,
    sp: usize,
    s0_11: [usize; 12],
}

pub struct TaskControlBlock {
    pub status: TaskStatus,
    pub cx: TaskContext,
}

pub struct TaskManager {
    num_task: usize,
    current_task: usize,
    tasks: [TaskControlBlock; MAX_TASK_NUM],
    stacks: [UserStack; MAX_TASK_NUM],
}

impl TaskManager {
    pub const fn new() -> Self {
        todo!()
    }

    pub fn find_next_task(&self) -> Option<usize> {
        todo!()
    }
}


pub fn run_next_task() {
    let mut task_mgr = TASK_MANAGER.lock();
    let current_task = task_mgr.current_task;
    let current_task_cx = &mut task_mgr.tasks[current_task];

}
