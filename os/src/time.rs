use riscv::register::time;

pub const CLOCK_FREQ: usize = 12500000;
const MICRO_PER_SEC: usize = 1_000_000;

pub fn get_time() -> usize {
    time::read()
}

pub fn get_time_us() -> usize {
    time::read() / (CLOCK_FREQ / MICRO_PER_SEC)
}
