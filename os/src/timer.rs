use riscv::register::time;

pub const CLOCK_FREQ: usize = 12500000;

pub fn get_current_time() -> usize {
    time::read()
}
