use riscv::register::time;

pub const CLOCK_FREQ: usize = 12500000;
pub const MILLI_PER_SEC: usize = 1000;

pub const CLOCKS_PER_SEC: usize = CLOCK_FREQ / 1;
pub const CLOCKS_PER_MILLI_SEC: usize = CLOCKS_PER_SEC / MILLI_PER_SEC;


pub fn get_time() -> usize {
    time::read()
}
