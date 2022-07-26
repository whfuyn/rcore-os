build:
	make -C .. build
	cargo build --release
	rust-objcopy \
		--binary-architecture=riscv64 \
		--strip-all \
		-O binary \
		target/riscv64gc-unknown-none-elf/release/loader \
		loader.bin

run: build
	qemu-system-riscv64 \
		-machine virt \
		-nographic \
		-bios ../rustsbi-qemu-orig.bin \
		-device loader,file=loader.bin,addr=0x80200000

debug: build
	qemu-system-riscv64 \
		-machine virt \
		-nographic \
		-bios ../rustsbi-qemu-orig.bin \
		-device loader,file=loader.bin,addr=0x80200000 \
		-S -s

gdb: 
	riscv64-unknown-elf-gdb \
		-ex 'file target/riscv64gc-unknown-none-elf/release/loader' \
		-ex 'set arch riscv:rv64' \
		-ex 'target remote localhost:1234'

.PHONY: build run
