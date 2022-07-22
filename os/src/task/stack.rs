use core::cell::SyncUnsafeCell;

const KERNEL_STACK_SIZE: usize = 4096 * 2;
const USER_STACK_SIZE: usize = 4096 * 2;

#[repr(align(4096))]
pub struct KernelStack(SyncUnsafeCell<[u8; KERNEL_STACK_SIZE]>);

#[repr(align(4096))]
pub struct UserStack(SyncUnsafeCell<[u8; USER_STACK_SIZE]>);


impl KernelStack {
    pub fn new() -> Self {
        SyncUnsafeCell::new([0; KERNEL_STACK_SIZE])
    }

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
    pub fn new() -> Self {
        SyncUnsafeCell::new([0; KERNEL_STACK_SIZE])
    }

    pub fn get_sp(&self) -> *mut u8 {
        unsafe {
            let stack = self.0.get();
            let len = (*stack).len() as isize;
            (stack as *mut u8).offset(len)
        }
    }
}
