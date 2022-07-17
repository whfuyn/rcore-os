use core::arch::global_asm;
use riscv::register::{
    stvec,
    scause::{
        self,
        Trap, Interrupt, Exception,
    },
};
use crate::syscall::syscall;
use crate::batch::APP_MANAGER;

global_asm!(include_str!("trap/trap.S"));


pub fn init() {
    extern "C" {
        fn __all_trap();
    }
    unsafe {
        stvec::write(__all_trap as usize, stvec::TrapMode::Direct);
    }
}


#[repr(C)]
pub struct TrapContext {
    pub x: [usize; 32],
    pub sstatus: usize,
    pub sepc: usize,
}

#[no_mangle]
pub extern "C" fn trap_handler(cx: &mut TrapContext) {
    let scause = scause::read();

    match scause.cause() {
        Trap::Exception(Exception::UserEnvCall) => {
            cx.sepc += 4;
            let id = cx.x[17];
            let args = [cx.x[10], cx.x[11], cx.x[12]];
            cx.x[10] = syscall(id, args) as usize;
        }
        Trap::Exception(Exception::StoreFault | Exception::StorePageFault) => {
            todo!()
        }
        Trap::Exception(Exception::IllegalInstruction) => {
            todo!()
        }
    }
}
