use std::fs;

const LINKER_SCRIPT: &str =
"OUTPUT_ARCH(riscv)
ENTRY(_start)
BASE_ADDRESS = {BASE_ADDRESS};

SECTIONS
{
    . = BASE_ADDRESS;

    .text : {
        *(.text.entry)
        *(.text .text.*)
    }
    . = ALIGN(4K);

    .rodata : {
        *(.rodata .rodata.*)
        *(.srodata .srodata.*)
    }
    . = ALIGN(4K);

    .data : {
        *(.data .data.*)
        *(.sdata .sdata.*)
    }
    . = ALIGN(4K);

    .bss : {
        *(.bss.stack)
        sbss = .;
        *(.bss .bss.*)
        *(.sbss .sbss.*)
        ebss = .;
    }
    
    /DISCARD/ : {
        *(.eh_frame)
        *(.debug*)
    }
}
";

fn main() {
    // let base_addr = std::env::var("BASE_ADDRESS").unwrap_or("0x80400000".into());
    let base_addr = "0x10000";
    let linker_script = LINKER_SCRIPT.replace("{BASE_ADDRESS}", &base_addr);
    fs::write("src/linker.ld", linker_script).expect("cannot write linker script");
}