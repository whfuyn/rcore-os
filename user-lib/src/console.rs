use core::fmt;
use crate::syscall::sys_write;

struct Stdout;

impl fmt::Write for Stdout {
    fn write_str(&mut self, s: &str) -> fmt::Result {
        sys_write(1, s.as_bytes());
        Ok(())
    }
}

pub fn print(args: fmt::Arguments) {
    use fmt::Write;
    Stdout.write_fmt(args).unwrap();
}

#[macro_export]
macro_rules! print {
    ($fmt: literal $(, $($arg: tt)+)?) => {
        $crate::console::print(::core::format_args!($fmt $(, $($arg)+)?))
    }
}

#[macro_export]
macro_rules! println {
    () => {
        println!("")
    };
    ($fmt: literal $(, $($arg: tt)+)?) => {
        $crate::console::print(::core::format_args_nl!($fmt $(, $($arg)+)?))
    };
}
