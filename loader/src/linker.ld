OUTPUT_ARCH(riscv)
ENTRY(_start)
BASE_ADDRESS = 0x80200000;

SECTIONS
{
    . = BASE_ADDRESS;
    sloader = .;

    stext = .;
    .text : {
        /* loader entrypoint */
        *(.text.entry)
        *(.text .text.*)

        /* kernel binary */
        . = ALIGN(1024M);
        skernel = .;
        *(.kernel)
        . = ALIGN(1024M);
        ekernel = .;
    }

    etext = .;
    srodata = .;
    .rodata : {
        *(.rodata .rodata.*)
        *(.srodata .srodata.*)
    }

    . = ALIGN(4K);
    erodata = .;

    sdata = .;
    .data : {
        *(.data .data.*)
        *(.sdata .sdata.*)
    }
    . = ALIGN(4K);
    edata = .;

    .bss : {
        *(.bss.stack)
        sbss = .;
        *(.bss .bss.*)
        *(.sbss .sbss.*)
    }
    . = ALIGN(4K);
    ebss = .;
    
    eloader = .;

    /DISCARD/ : {
        *(.eh_frame)
    }
}