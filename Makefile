LOADER := loader
LOADER_OUT_DIR := $(LOADER)/target/riscv64gc-unknown-none-elf/release

OS := os
OS_OUT_DIR := $(OS)/target/riscv64gc-unknown-none-elf/release

RUSTSBI_QEMU := rustsbi-qemu
RUSTSBI_QEMU_OUT_DIR := $(RUSTSBI_QEMU)/target/riscv64imac-unknown-none-elf/release

STRIP := rust-objcopy \
		--binary-architecture=riscv64 \
		--strip-all \
		-O binary

build-loader:
	cd $(LOADER) && cargo build --release
	$(STRIP) \
		$(LOADER_OUT_DIR)/$(LOADER) \
		$(LOADER_OUT_DIR)/$(LOADER).bin

build-os:
	cd $(OS) && cargo build --release
	$(STRIP) \
		$(OS_OUT_DIR)/$(OS) \
		$(OS_OUT_DIR)/$(OS).bin

build-sbi:
	cd $(RUSTSBI_QEMU) && cargo make

run: build-os build-loader
	@qemu-system-riscv64 \
		-machine virt \
		-nographic \
		-bios rustsbi-qemu-orig.bin \
		-device loader,file=$(LOADER_OUT_DIR)/$(LOADER).bin,addr=0x80200000

run-self-built-sbi: build-os build-loader build-sbi
	@qemu-system-riscv64 \
		-machine virt \
		-nographic \
		-bios rustsbi-qemu-orig.bin \
	 	-bios $(RUSTSBI_QEMU_OUT_DIR)/$(RUSTSBI_QEMU).bin \
		-device loader,file=$(LOADER_OUT_DIR)/$(LOADER).bin,addr=0x80200000

debug: build-os build-loader
	@qemu-system-riscv64 \
		-machine virt \
		-nographic \
		-bios rustsbi-qemu-orig.bin \
		-device loader,file=$(LOADER_OUT_DIR)/$(LOADER).bin,addr=0x80200000 \
		-s -S

gdb: 
	riscv64-unknown-elf-gdb \
		-ex 'file $(OS_OUT_DIR)/$(OS)' \
		-ex 'set arch riscv:rv64' \
		-ex 'target remote localhost:1234'

clean:
	@cd loader && cargo clean
	@cd os && cargo clean && rm -f src/link_app.S
	@cd user-lib && cargo clean && rm -f src/linker.ld

clean-all:
	@cd $(RUSTSBI_QEMU) && cargo clean
	@cd loader && cargo clean
	@cd os && cargo clean && rm -f src/link_app.S
	@cd user-lib && cargo clean && rm -f src/linker.ld

.PHONY: run debug gdb clean clean-all build-loader build-os build-sbi
