.altmacro
.macro SAVE_GP n
    sd x\n, \n*8(sp)
.endm
.macro LOAD_GP n
    ld x\n, \n*8(sp)
.endm

    .section .text
    .align 2
    .global __all_traps
__all_traps:
    csrrw sp, sscratch, sp
    addi sp, sp, -34*8
    SAVE_GP 1
    SAVE_GP 3
    .set n, 5
    .rept 27
        SAVE_GP %n
        .set n, n + 1
    .endr

    csrr t0, sstatus
    csrr t1, sepc
    sd t0, 32*8(sp)
    sd t1, 33*8(sp)

    csrr t2, sscratch
    sd t2, 2*8(sp)

    # We pass &mut TrapContext to the handler
    mv a0, sp
    call trap_handler

    .global __restore
__restore:
    # trap_handler returns the kernel sp
    # This also handle the app init case.
    mv sp, a0

    ld t0, 32*8(sp)
    ld t1, 33*8(sp)
    ld t2, 2*8(sp)
    csrw sstatus, t0
    csrw sepc, t1
    csrw sscratch, t2

    LOAD_GP 1
    LOAD_GP 3

    .set n, 5
    .rept 27
        LOAD_GP %n
        .set n, n+1
    .endr

    addi sp, sp, 34*8
    csrrw sp, sscratch, sp
    sret