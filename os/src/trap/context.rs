use riscv::register::sstatus::{self, Sstatus, SPP};
use crate::config::KERNEL_STACK_VA;
use crate::config::PAGE_SIZE;
use crate::mm::VirtAddr;

pub const TRAP_CX_VA: VirtAddr = KERNEL_STACK_VA.add(PAGE_SIZE - core::mem::size_of::<TrapContext>());

#[repr(C)]
pub struct TrapContext {
    pub x: [usize; 32],
    pub sstatus: Sstatus,
    pub sepc: usize,
    // pub satp: usize,
    // pub kernel_stack: usize,
    // pub brk: VirtAddr,
}

impl TrapContext {
    pub fn set_sp(&mut self, sp: usize) {
        self.x[2] = sp;
    }

    pub fn app_init_context(entry: usize, sp: usize) -> Self {
        let mut sstatus = sstatus::read();
        sstatus.set_spp(SPP::User);
        sstatus.set_spie(true);

        let mut cx = Self {
            x: [0; 32],
            sstatus,
            sepc: entry,
        };
        cx.set_sp(sp);

        cx
    }
}
