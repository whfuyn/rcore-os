
fn main() {
    // Pass linker script here to avoid interferences with rustsbi-qemu
    println!("cargo:rustc-link-arg=-Tsrc/linker.ld");
}
