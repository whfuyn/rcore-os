    .section .text.entry
    .global _loader_start
_loader_start:
    csrw sie, zero
    la sp, loader_stack_top
    call loader_main

    .section .bss.stack
    .global loader_stack
loader_stack:
    .space 4096 * 10
    .global loader_stack_top
loader_stack_top:
