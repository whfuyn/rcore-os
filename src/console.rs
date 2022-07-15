use core::fmt;
// use crate::sys_write;
use crate::sbi::console_putchar;

struct Stdout;

impl fmt::Write for Stdout {
    fn write_str(&mut self, s: &str) -> fmt::Result {
        // The following code doesn't work on qemu machine mode
        // sys_write(1, s.as_bytes());         // 1 means stdout
        s.bytes().for_each(|c| console_putchar(c as usize));
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
