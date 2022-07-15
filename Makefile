.PHONY: build run clean debug gdb

OS := os
STRIP := rust-objcopy \
		--binary-architecture=riscv64 \
		--strip-all \
		-O binary \
TARGET := target/riscv64gc-unknown-none-elf/release

rustsbi-qemu.bin: rustsbi-qemu
	cd rustsbi-qemu && \
	cargo build --release -Zbuild-std -p rustsbi-qemu && \
	$(STRIP) \
		$(TARGET)/rustsbi-qemu \
		$(TARGET)/rustsbi-qemu.bin

build:
	cd os && \
	cargo build --release && \
	$(STRIP) \
		$(TARGET)/$(OS) \
		$(TARGET)/$(OS).bin

run: rustsbi-qemu.bin build
	@qemu-system-riscv64 \
		-machine virt \
		-nographic \
		-bios rustsbi-qemu/$(TARGET)/rustsbi-qemu.bin
		-device loader,file=$(OS)/$(TARGET)/$(OS).bin,addr=0x80200000

debug: rustsbi-qemu.bin build
	@qemu-system-riscv64 \
		-machine virt \
		-nographic \
		-bios rustsbi-qemu/$(TARGET)/rustsbi-qemu.bin
		-device loader,file=$(OS)/$(TARGET)/$(OS).bin,addr=0x80200000 \
		-s -S

gdb: 
	riscv64-unknown-elf-gdb \
		-ex 'file $(OS)/$(TARGET)/$(OS)' \
		-ex 'set arch riscv:rv64' \
		-ex 'target remote localhost:1234'

clean:
	@cd rustsbi-qemu && cargo clean
	@rm -f rustsbi-qemu.bin
	@cargo clean
