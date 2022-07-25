OUTPUT_ARCH(riscv)
ENTRY(_loader_start)
BASE_ADDRESS = 0x80200000;

SECTIONS
{
    . = BASE_ADDRESS;
    sloader = .;

    sloader_text = .;
    .text : {
        *(.text.entry)
        *(.text .text.*)
    }
    . = ALIGN(4K);
    eloader_text = .;

    sloader_rodata = .;
    .rodata : {
        *(.rodata .rodata.*)
        *(.srodata .srodata.*)
    }
    . = ALIGN(4K);
    eloader_rodata = .;

    sloader_data = .;
    .data : {
        *(.data .data.*)
        *(.sdata .sdata.*)
    }
    . = ALIGN(4K);
    eloader_data = .;

    .bss : {
        *(.bss.stack)
        sloader_bss = .;
        *(.bss .bss.*)
        *(.sbss .sbss.*)
    }
    . = ALIGN(4K);
    eloader_bss = .;
    
    eloader = .;

    /DISCARD/ : {
        *(.eh_frame)
    }
}