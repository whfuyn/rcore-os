    .p2align 12
    .section .data.kernel
    .global spacked_kernel
    .global epacked_kernel
spacked_kernel:
    .incbin "../os/target/riscv64gc-unknown-none-elf/release/os.bin"
    .p2align 12
epacked_kernel:
