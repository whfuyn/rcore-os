mod stack;

use lazy_static::lazy_static;
use core::arch::global_asm;
use core::arch::asm;
use core::mem::MaybeUninit;
use spin::Mutex;
use alloc::vec::Vec;
use crate::mm::*;

use stack::{ KernelStack, UserStack };
use crate::trap::TrapContext;
use crate::sbi;
use crate::println;
use crate::trap::__restore;
use crate::time;
use crate::syscall::MAX_SYSCALL_NUM;
use crate::mm::address_space::AddressSpace;

const MAX_TASK_NUM: usize = 32;

// const APP_BASE_ADDR: *mut u8 = 0x80400000 as *mut u8;
const APP_BASE_ADDR: *mut u8 = 0x10000 as *mut u8;
const MAX_APP_SIZE: usize = 0x20000;

const PAGE_SIZE: usize = 4096;

global_asm!(include_str!("link_app.S"));
extern "C" {
    static _app_info_table: usize;
}

global_asm!(include_str!("task/switch.S"));
extern "C" {
    fn __switch(current_cx: *mut TaskContext, next_cx: *mut TaskContext);
}


// static KERNEL_STACK: [KernelStack ; MAX_TASK_NUM]= {
//     const KERNEL_STACK: KernelStack = KernelStack::new();
//     [KERNEL_STACK; MAX_TASK_NUM]
// };
// static USER_STACK: [UserStack; MAX_TASK_NUM] = {
//     const USER_STACK: UserStack = UserStack::new();
//     [USER_STACK; MAX_TASK_NUM]
// };

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

        let mut tcbs: Vec<TaskControlBlock> = Vec::new();
        let mut addr_spaces: Vec<AddressSpace> = Vec::new();
        let mut stats: Vec<TaskStat> = Vec::new();

        // let mut dbg_ksp = 0;
        for i in 0..num_app {
            let mut addr_space = AddressSpace::new(i + 2);

            // TODO: put ustack in lower addr
            let (ustack_vpn, _) = addr_space.alloc_page();
            let usp = ustack_vpn.as_va().0 + PAGE_SIZE;

            let (kstack_vpn, kstack_ppn) = addr_space.alloc_kernel_page();
            let kstack =  &mut *(kstack_ppn.as_pa().0 as *mut KernelStack);
            let task_init_trap_cx = TrapContext::app_init_context(
                APP_BASE_ADDR as usize, usp
                // start_task as usize, &SPACE as *const _ as usize
            );
            kstack.push_context(task_init_trap_cx);
            let ksp = kstack_vpn.as_va().0 + PAGE_SIZE - core::mem::size_of::<TrapContext>();
            // dbg_ksp = ksp;
            println!("ksp: 0x{:x}", ksp);
            // let ksp = &SPACE as *const _ as usize;

            let mut tcb = TaskControlBlock::default();
            tcb.cx.sp = ksp;
            // tcb.cx.sp = ;
            tcb.cx.ra = __restore as usize;
            // tcb.cx.ra = start_task as usize;
            tcb.cx.satp = addr_space.satp();


            tcbs.push(tcb);
            addr_spaces.push(addr_space);
            stats.push(TaskStat::default());
        }

        // println!("asp0 translate to 0x{:x}", addr_spaces[0].page_table.as_page_table().as_ref().unwrap().translate(VirtAddr::new(dbg_ksp)).unwrap().0);
        // println!("asp1 translate to 0x{:x}", addr_spaces[1].page_table.as_page_table().as_ref().unwrap().translate(VirtAddr::new(dbg_ksp)).unwrap().0);


        // println!("tcb[0]: {:p}", &(tcbs[0]));
        // println!("tcb[1]: {:p}", &(tcbs[1]));

        let mut task_mgr = Self {
            app_starts,
            num_app,
            current_task: 0,
            tcbs,
            addr_spaces,
            stats,
        };

        for i in 0..num_app {
            task_mgr.load_task(i);
        }
        unsafe {
            println!("jump to 0x{:x}", (*task_mgr.addr_spaces[0].page_table.as_page_table()).translate(VirtAddr::new(APP_BASE_ADDR as usize)).unwrap().0);
        }

        task_mgr
    }

    pub unsafe fn load_task(&mut self, task_id: usize) {
        let task_start = self.app_starts[task_id];
        let task_end = self.app_starts[task_id + 1];
        let task_size = task_end.saturating_sub(task_start);

        const PAGE_SIZE: usize = 4096;
        let mut loaded_size = 0usize;
        let mut load_to = VirtAddr::new(APP_BASE_ADDR as usize);

        let addr_space = &mut self.addr_spaces[task_id];
        while loaded_size < task_size {
            let (_vpn, ppn) = addr_space.alloc_page_at(load_to.vpn());
            let load_to_pa = ppn.as_pa().0 as *mut u8;
            // TODO: fix size
            // core::ptr::copy_nonoverlapping(task_start as *const u8, load_to_pa, PAGE_SIZE);
            core::ptr::copy((task_start + loaded_size) as *const u8, load_to_pa, PAGE_SIZE);
            println!("task `{task_id}` loaded at va `0x{:x}` pa `0x{:x}`", load_to.0, ppn.as_pa().0);
            loaded_size += PAGE_SIZE;
            load_to.0 += PAGE_SIZE;
        }
        // TODO
        addr_space.alloc_page_at(load_to.vpn());
        // addr_space.alloc_page_at(load_to.vpn());
        // addr_space.alloc_page_at(load_to.vpn());
        // addr_space.alloc_page_at(load_to.vpn());

        self.tcbs[task_id].status = TaskStatus::Ready;
    }

    /// Return current task cx and next task cx
    pub unsafe fn move_to_next_task(&mut self, next_task: usize) -> (*mut TaskContext, *mut TaskContext) {
        let current_task = self.current_task;
        // println!("curr task: {}", current_task);
        // println!("next task: {}", next_task);
        // let current_tcb = &self.tcbs[current_task];
        // let next_tcb = &self.tcbs[next_task];
        // println!("current tcb: {:x}", current_tcb as *const _ as usize);
        // println!("next tcb: {:x}", next_tcb as *const _ as usize);

        let current_tcb = &mut self.tcbs[current_task];
        // println!("current tcb: {:x}", current_tcb as *const _ as usize);
        let current_task_cx = &mut current_tcb.cx as *mut TaskContext;
        if current_tcb.status == TaskStatus::Running {
            current_tcb.status = TaskStatus::Ready;
        }
        self.stats[current_task].record_schedule_end();

        let next_tcb = &mut self.tcbs[next_task];
        // println!("next tcb: {:x}", next_tcb as *const _ as usize);
        let next_task_cx = &mut next_tcb.cx as *mut TaskContext;
        assert!(next_tcb.status == TaskStatus::Ready);
        next_tcb.status = TaskStatus::Running;
        self.stats[next_task].record_schedule_begin();

        self.current_task = next_task;
        // println!("current cx {:p}", current_task_cx);
        // println!("next cx {:p}", next_task_cx);

        (current_task_cx, next_task_cx)
    }

    pub fn find_next_task(&self) -> Option<usize> {
        let mut idx = (self.current_task + 1) % self.num_app;
        for _ in 0..self.num_app {
            if self.tcbs[idx].status == TaskStatus::Ready {
                // println!("found task {}", idx);
                return Some(idx);
            }
            idx = (idx + 1) % self.num_app;
        }
        if self.tcbs[self.current_task].status == TaskStatus::Running {
            // println!("found current {}", idx);
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

pub unsafe extern "C" fn start_task() {
    println!("start task");
    sbi::shutdown();
}
//     let task_mgr = TASK_MANAGER.lock();

//     let current_task = task_mgr.current_task;
//     let task_entry = get_task_base(current_task);
//     drop(task_mgr);

//     let mut task_init_trap_cx = TrapContext::app_init_context(
//         task_entry as usize, USER_STACK[current_task].get_sp() as usize
//     );

//     // We are already in our kernel stack. Don't need to push context to kernel stack.
//     __restore(
//         &mut task_init_trap_cx as *mut TrapContext as usize
//     );
// }

pub fn exit_and_run_next() {
    let mut task_mgr = TASK_MANAGER.lock();

    let current_task = task_mgr.current_task;
    // println!("task `{current_task}` exited");
    let current_tcb = &mut task_mgr.tcbs[current_task];
    current_tcb.status = TaskStatus::Exited;
    drop(task_mgr);
    run_next_task();
}

pub fn run_first_task() {
    println!("access task mgr");
    let mut task_mgr = TASK_MANAGER.lock();
    println!("access task mgr done");

    let first_task = if task_mgr.num_app > 0 { 0 } else { finish() };
    let (_, first_task_cx) = unsafe { task_mgr.move_to_next_task(first_task) };

    drop(task_mgr);

    println!("switch to first.. 1111");
    println!("debug cx: {:?}", unsafe {&*first_task_cx });
    // set_next_trigger();
    let mut unused = TaskContext::default();
    println!("__switch: 0x{:x}", __switch as usize);
    // println!("start_task: 0x{:x}", start_task as usize);
    // extern {
    //     fn what_the_fuck();
    // }
    // println!("wtf: 0x{:x}", what_the_fuck as usize);
    unsafe {
        __switch(&mut unused, first_task_cx);
    }
}

pub fn run_next_task() {
    let mut task_mgr = TASK_MANAGER.lock();
    let next_task = task_mgr.find_next_task_or_exit();
    let (current_task_cx, next_task_cx) = unsafe { task_mgr.move_to_next_task(next_task) };
    drop(task_mgr);

    // println!("switch to next");
    // println!("curr sepc: 0x{:x}", unsafe{ (*(((*current_task_cx).sp - 0) as *const TrapContext)).sepc });
    // println!("next sepc: 0x{:x}", unsafe{ (*(((*next_task_cx).sp - 0) as *const TrapContext)).sepc });
    // unsafe {
    //     (*(((*next_task_cx).sp - 0) as *mut TrapContext)).sepc = 0x10000;
    // }
    set_next_trigger();
    unsafe {
        __switch(current_task_cx, next_task_cx);
    }
}

// fn get_task_base(task_id: usize) -> *mut u8 {
//     unsafe {
//         APP_BASE_ADDR.add(task_id * MAX_APP_SIZE)
//     }
// }

fn finish() -> ! {
    println!("[kernel] All apps have completed.");
    sbi::shutdown();
}

pub fn set_next_trigger() {
    const TICKS_PER_SEC: usize = 1000;
    let current_time = time::get_time();
    let delta = time::CLOCK_FREQ / TICKS_PER_SEC;
    sbi::set_timer(current_time + delta);
}

pub fn record_syscall(syscall: usize) {
    let mut task_mgr = TASK_MANAGER.lock();
    let curent_task = task_mgr.current_task;
    task_mgr.stats[curent_task].record_syscall(syscall);
}
