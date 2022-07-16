use lazy_static::lazy_static;
use spin::Mutex;
use crate::println;

const APP_BASE_ADDR: *const u8 = 0x80400000 as *const u8;
const MAX_APP_NUM: usize = 32;

lazy_static!{
    pub static ref APP_MANAGER: Mutex<AppManager> = unsafe {
        Mutex::new(AppManager::new())
    };
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
        let num_app = core::cmp::min(ptr.read_volatile(), MAX_APP_NUM);

        let mut app_start = [0; MAX_APP_NUM + 1];
        // The last one is app_end
        for i in 0..=num_app {
            app_start[i] = ptr.offset((1 + i) as isize).read_volatile();
        }
        Self { num_app, current_app: 0, app_start }
    }

    pub fn load_app(&mut self, app_id: usize) {
        if app_id >= self.num_app {
            panic!("app_id exceeds the num of app");
        }

        let app_start = self.app_start[app_id];
        let app_size = self.app_start[app_id + 1].saturating_sub(app_start);

        unsafe {
            core::ptr::copy_nonoverlapping(
                app_start as *const u8, APP_BASE_ADDR as *mut u8, app_size
            );
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
}
