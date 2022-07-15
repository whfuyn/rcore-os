.PHONY: build run clean debug gdb

OS := rcore-os
STRIP := rust-objcopy \
		--binary-architecture=riscv64 \
		--strip-all \
		-O binary \

rustsbi-qemu.bin: rustsbi-qemu
	cd rustsbi-qemu && \
	cargo build --release -Zbuild-std -p rustsbi-qemu && \
	$(STRIP) \
		target/riscv64gc-unknown-none-elf/release/rustsbi-qemu \
		../rustsbi-qemu.bin

build:
	cargo build --release
	$(STRIP) \
		target/riscv64gc-unknown-none-elf/release/$(OS) \
		target/riscv64gc-unknown-none-elf/release/$(OS).bin

run: rustsbi-qemu.bin build
	@qemu-system-riscv64 \
		-machine virt \
		-nographic \
		-bios rustsbi-qemu.bin \
		-device loader,file=target/riscv64gc-unknown-none-elf/release/$(OS).bin,addr=0x80200000

debug: rustsbi-qemu.bin build
	@qemu-system-riscv64 \
		-machine virt \
		-nographic \
		-bios rustsbi-qemu.bin \
		-device loader,file=target/riscv64gc-unknown-none-elf/release/$(OS).bin,addr=0x80200000 \
		-s -S

gdb: 
	riscv64-unknown-elf-gdb \
		-ex 'file target/riscv64gc-unknown-none-elf/release/$(OS)' \
		-ex 'set arch riscv:rv64' \
		-ex 'target remote localhost:1234'

clean:
	@cd rustsbi-qemu && cargo clean
	@rm -f rustsbi-qemu.bin
	@cargo clean
