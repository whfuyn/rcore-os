.PHONY: build run clean debug gdb

OS := os
OS_OUT_DIR := $(OS)/target/riscv64gc-unknown-none-elf/release

RUSTSBI_QEMU := rustsbi-qemu
RUSTSBI_QEMU_OUT_DIR := $(RUSTSBI_QEMU)/target/riscv64imac-unknown-none-elf/release

STRIP := rust-objcopy \
		--binary-architecture=riscv64 \
		--strip-all \
		-O binary

$(RUSTSBI_QEMU).bin:
	cd $(RUSTSBI_QEMU) && cargo make

build:
	cd $(OS) && cargo build --release
	$(STRIP) \
		$(OS_OUT_DIR)/$(OS) \
		$(OS_OUT_DIR)/$(OS).bin

run: build
	@qemu-system-riscv64 \
		-machine virt \
		-nographic \
		-bios rustsbi-qemu-orig.bin \
		-device loader,file=$(OS_OUT_DIR)/$(OS).bin,addr=0x80200000

run-self-built: $(RUSTSBI_QEMU).bin build
	@qemu-system-riscv64 \
		-machine virt \
		-nographic \
		-bios $(RUSTSBI_QEMU_OUT_DIR)/$(RUSTSBI_QEMU).bin \
		-device loader,file=$(OS_OUT_DIR)/$(OS).bin,addr=0x80200000

debug: $(RUSTSBI_QEMU).bin build
	@qemu-system-riscv64 \
		-machine virt \
		-nographic \
		-bios $(RUSTSBI_QEMU_OUT_DIR)/$(RUSTSBI_QEMU).bin \
		-device loader,file=$(OS_OUT_DIR)/$(OS).bin,addr=0x80200000 \
		-s -S

gdb: 
	riscv64-unknown-elf-gdb \
		-ex 'file $(OS_OUT_DIR)/$(OS)' \
		-ex 'set arch riscv:rv64' \
		-ex 'target remote localhost:1234'

clean:
	@cd os && cargo clean && rm -f src/link_app.S
	@cd user-lib && cargo clean && rm -f src/linker.ld

clean-all:
	@cd $(RUSTSBI_QEMU) && cargo clean
	@cd os && cargo clean && rm -f src/link_app.S
	@cd user-lib && cargo clean && rm -f src/linker.ld