use crate::println;
use crate::sbi::shutdown;
use core::panic::PanicInfo;

#[panic_handler]
fn panic_handler(info: &PanicInfo) -> ! {
    if let Some(location) = info.location() {
        println!(
            "panic in file `{}` at line {}: {}",
            location.file(),
            location.line(),
            info
        );
    } else {
        println!("panic: {}", info)
    }
    shutdown();
}
