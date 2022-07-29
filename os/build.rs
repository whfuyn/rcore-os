use serde::Deserialize;
use std::fs;
use std::process::Command;

const APP_BASE_ADDR: *mut u8 = 0x80400000 as *mut u8;
const MAX_APP_SIZE: usize = 0x20000;

#[derive(Debug, Deserialize)]
struct Binary {
    name: String,
}

#[derive(Debug, Deserialize)]
struct UserLibToml {
    #[serde(default)]
    bin: Vec<Binary>,
}

fn build_user_bin(name: &str, base_addr: usize) {
    let build_user_bin_res = Command::new("cargo")
        .env("BASE_ADDRESS", format!("0x{:X}", base_addr))
        .args([
            "build",
            "--release",
            "--bin",
            name,
            "--manifest-path",
            "../user-lib/Cargo.toml",
        ])
        .output();

    match build_user_bin_res {
        Ok(output) if output.status.success() => (),
        Ok(output) => {
            panic!("stderr: {}", String::from_utf8_lossy(&output.stderr));
        }
        Err(e) => panic!("failed to build user lib: {e}"),
    }
}

fn main() {
    println!("cargo:rerun-if-changed=../user-lib/src");
    println!("cargo:rerun-if-changed=../user-lib/Cargo.toml");

    let user_lib_toml =
        fs::read_to_string("../user-lib/Cargo.toml").expect("cannot read user-lib Cargo.toml");
    let bins: Vec<String> = toml::from_str::<UserLibToml>(&user_lib_toml)
        .expect("cannot parse user-lib [[bin]]")
        .bin
        .into_iter()
        .map(|b| b.name)
        .collect();
    
    bins.iter().enumerate().for_each(|(i, bin)|{
        let base_addr = unsafe {
            APP_BASE_ADDR.add(i * MAX_APP_SIZE) as usize
        };
        build_user_bin(bin, base_addr)
    });

    let user_lib_out_dir = "../user-lib/target/riscv64gc-unknown-none-elf/release";
    // let mut stripped_bin_paths: Vec<String> = bins
    //     .iter()
    //     .map(|bin| {
    //         let bin_path = format!("{user_lib_out_dir}/{bin}");
    //         let stripped_bin_path = format!("{bin_path}.bin");

    //         let strip_user_lib_res = Command::new("rust-objcopy")
    //             .args([
    //                 "--binary-architecture=riscv64",
    //                 "--strip-all",
    //                 "-O",
    //                 "binary",
    //                 &bin_path,
    //                 &stripped_bin_path,
    //             ])
    //             .output();

    //         match strip_user_lib_res {
    //             Ok(output) if output.status.success() => (),
    //             Ok(output) => {
    //                 panic!("stderr: {}", String::from_utf8_lossy(&output.stderr));
    //             }
    //             Err(e) => panic!("failed to run strip cmd for `{bin_path}`: {e}"),
    //         }
    //         stripped_bin_path
    //     })
    //     .collect();

    // bins.push("ch2b_hello_world".into());
    // stripped_bin_paths.push("../ch2b_hello_world.bin".into());
    let elf_paths = bins.iter()
        .map(|b| format!("{user_lib_out_dir}/{b}"))
        .collect();
    let link_app_asm = build_link_app_asm(bins, elf_paths);
    fs::write("src/link_app.S", link_app_asm).expect("cannot write link_app.S");
}

fn build_link_app_asm(bins: Vec<String>, elf_paths: Vec<String>) -> String {
    // Build start and end addr for each app.
    let mut link_app_asm = format!(
"    .p2align 3
    .section .data
    .global _app_info_table
_app_info_table:
    .quad {}\
",
        bins.len()
    );

    for i in 0..bins.len() {
        let start_entry = format!(
"
    .quad app_{i}_start\
"
        );
        link_app_asm.push_str(&start_entry);
        if i + 1 == bins.len() {
            let end_entry = format!(
"
    .quad app_{i}_end
",
            );
            link_app_asm.push_str(&end_entry);
        }
    }

    for (i, elf_path) in elf_paths.iter().enumerate() {
        let entry = format!(
"
    .section .data
    .global app_{i}_start
    .global app_{i}_end
app_{i}_start:
    .incbin \"{elf_path}\"
app_{i}_end:
"
        );
        link_app_asm.push_str(&entry);
    }

    link_app_asm
}
