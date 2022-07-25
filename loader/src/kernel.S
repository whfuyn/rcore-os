    .p2align 21
    .section .data.kernel
    .global spacked_kernel
    .global epacked_kernel
spacked_kernel:
    .incbin "../os/target/riscv64gc-unknown-none-elf/release/os.bin"
    .p2align 21
epacked_kernel:
