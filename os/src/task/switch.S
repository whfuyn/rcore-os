.altmacro

.macro SAVE_SN n
    sd s\n, (\n+3)*8(a0)
.endm
.macro LOAD_SN n
    ld s\n, (\n+3)*8(a1)
.endm

    .section .text
    .global __switch
    .p2align 2
__switch:
    sd ra, 0*8(a0)
    sd sp, 1*8(a0)
    csrr t0, satp
    sd t0, 2*8(a0)

    .set n, 0
    .rept 12
        SAVE_SN %n
        .set n, n+1
    .endr

    ld ra, 0*8(a1)
    ld sp, 1*8(a1)

    .set n, 0
    .rept 12
        LOAD_SN %n
        .set n, n+1
    .endr

    ld t1, 2*8(a1)
    csrw satp, t1
    sfence.vma

    jalr x0, ra
