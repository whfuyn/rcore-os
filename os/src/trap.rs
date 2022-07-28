mod context;

use crate::task::{
    run_next_task, exit_and_run_next, set_next_trigger,
};
use crate::println;
use crate::syscall::syscall;
pub use context::TrapContext;
use core::arch::global_asm;
use riscv::register::{
    scause::{self, Exception, Interrupt, Trap},
    stval, stvec, sepc
};

global_asm!(include_str!("trap/trap.S"));
extern "C" {
    fn __all_traps();
    pub fn __restore(cx: usize) -> !;
}

pub fn init() {
    unsafe {
        stvec::write(__all_traps as usize, stvec::TrapMode::Direct);
    }
}

#[no_mangle]
pub extern "C" fn trap_handler(cx: &mut TrapContext) -> &mut TrapContext {
    // println!("into trap handler");
    let scause = scause::read();
    let stval = stval::read();

    match scause.cause() {
        Trap::Interrupt(Interrupt::SupervisorTimer) => {
            // println!("\nscheduling");
            // set_next_trigger();
            run_next_task();
        }
        Trap::Exception(Exception::UserEnvCall) => {
            cx.sepc += 4;
            let id = cx.x[17];
            let args = [cx.x[10], cx.x[11], cx.x[12]];
            cx.x[10] = syscall(id, args) as usize;
        }
        Trap::Exception(Exception::StoreFault | Exception::StorePageFault) => {
            println!("[kernel] PageFault in application, kernel killed it.");
            println!("[kernel] stval: 0x{:x}, sepc: 0x{:x}", stval, sepc::read());

            // unsafe {
            //     let task_mgr = crate::task::TASK_MANAGER.lock();
            //     let translated = (*task_mgr.addr_spaces[1].page_table.as_page_table()).translate(crate::mm::VirtAddr::new(stval)).unwrap().0;
            //     crate::println!("buffer translate 0x{:x}", translated);
            //     println!("decoded: {}", *(translated as *const u8));

            // }
            exit_and_run_next();
        }
        Trap::Exception(Exception::IllegalInstruction) => {
            println!("[kernel] IllegalInstruction in application, kernel killed it.");
            println!("[kernel] stval: 0x{:x}, sepc: 0x{:x}", stval, sepc::read());
            exit_and_run_next();
        }
        unknown => {
            println!("[kernel] stval: 0x{:x}, sepc: 0x{:x}", stval, sepc::read());
            panic!("Unsupported trap {:?}", unknown);
            // panic!("Unsupported trap {:?}, stval = {:#x}!", unknown, stval);
        }
    }
    cx
}
