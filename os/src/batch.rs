use crate::println;
use crate::sbi;
use crate::trap::TrapContext;
use core::cell::SyncUnsafeCell;
use lazy_static::lazy_static;
use riscv::register::sscratch;
use spin::Mutex;

const APP_BASE_ADDR: *const u8 = 0x80400000 as *const u8;
const MAX_APP_NUM: usize = 32;

const KERNEL_STACK_SIZE: usize = 4096 * 2;
const USER_STACK_SIZE: usize = 4096 * 2;

lazy_static! {
    pub static ref APP_MANAGER: Mutex<AppManager> = unsafe { Mutex::new(AppManager::new()) };
}

#[repr(align(4096))]
struct KernelStack(SyncUnsafeCell<[u8; KERNEL_STACK_SIZE]>);

#[repr(align(4096))]
struct UserStack(SyncUnsafeCell<[u8; USER_STACK_SIZE]>);

static KERNEL_STACK: KernelStack = KernelStack(SyncUnsafeCell::new([0; KERNEL_STACK_SIZE]));
static USER_STACK: UserStack = UserStack(SyncUnsafeCell::new([0; USER_STACK_SIZE]));

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

pub struct AppManager {
    num_app: usize,
    current_app: usize,
    app_start: [usize; MAX_APP_NUM + 1],
}

impl AppManager {
    pub unsafe fn new() -> Self {
        extern "C" {
            fn _num_app();
        }
        let ptr = _num_app as *const usize;
        // TODO: is volatile op necessary?
        let num_app = core::cmp::min(ptr.read_volatile(), MAX_APP_NUM);

        let mut app_start = [0; MAX_APP_NUM + 1];
        // The last one is app_end
        for i in 0..=num_app {
            app_start[i] = ptr.add(1 + i).read_volatile();
        }
        Self {
            num_app,
            current_app: 0,
            app_start,
        }
    }

    pub fn print_app_info(&self) {
        println!("App num: {}", self.num_app);
        for i in 0..self.num_app {
            println!(
                "App {}: [0x{:x}..0x{:x}]",
                i,
                self.app_start[i],
                self.app_start[i + 1],
            );
        }
        println!("Current app: {}", self.current_app);
    }

    pub unsafe fn load_app(&mut self, app_id: usize) {
        if app_id >= self.num_app {
            panic!("app_id exceeds the num of app");
        }

        let app_start = self.app_start[app_id];
        let app_size = self.app_start[app_id + 1].saturating_sub(app_start);

        core::ptr::copy_nonoverlapping(app_start as *const u8, APP_BASE_ADDR as *mut u8, app_size);
        core::arch::asm!("fence.i");
    }

    pub fn move_to_next_app(&mut self) -> usize {
        let current_app = self.current_app;
        self.current_app += 1;
        current_app
    }
}

pub fn init() {
    sscratch::write(KERNEL_STACK.get_sp() as usize);
}

pub fn run_next_app() -> ! {
    println!("run next app");
    let mut app_mgr = APP_MANAGER.lock();

    let current_app = app_mgr.move_to_next_app();
    if current_app >= app_mgr.num_app {
        println!("[kernel] All apps have completed.");
        sbi::shutdown();
    }

    let app_entry = app_mgr.app_start[current_app];
    unsafe {
        app_mgr.load_app(current_app);
    }
    println!("app loaded");

    let app_init_cx = TrapContext::app_init_context(app_entry, USER_STACK.get_sp() as usize);

    drop(app_mgr);
    unsafe {
        extern "C" {
            fn __restore(cx: usize);
        }
        __restore(KERNEL_STACK.push_context(app_init_cx) as usize);
    }

    unreachable!("it should have been running the app");
}
