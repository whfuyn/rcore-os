mod context;

use crate::task::{
    run_next_task, exit_task_and_run_next, set_next_trigger,
};
use crate::println;
use crate::syscall::syscall;
pub use context::TrapContext;
use core::arch::global_asm;
use riscv::register::{
    scause::{self, Exception, Interrupt, Trap},
    stval, stvec,
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
    let scause = scause::read();
    let stval = stval::read();

    match scause.cause() {
        Trap::Interrupt(Interrupt::SupervisorTimer) => {
            // println!("schedule---------");
            set_next_trigger();
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
            exit_task_and_run_next();
        }
        Trap::Exception(Exception::IllegalInstruction) => {
            println!("[kernel] IllegalInstruction in application, kernel killed it.");
            exit_task_and_run_next();
        }
        unknown => {
            panic!("Unsupported trap {:?}, stval = {:#x}!", unknown, stval);
        }
    }
    cx
}
