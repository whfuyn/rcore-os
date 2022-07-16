use serde::Deserialize;
use std::process::{ Command, Stdio };
use std::fs;

#[derive(Debug, Deserialize)]
struct Binary {
    name: String,
}

#[derive(Debug, Deserialize)]
struct UserLibToml {
    #[serde(default)]
    bin: Vec<Binary>,
}

fn main() {
    let build_user_lib_res = Command::new("cargo")
        .args([
            "build",
            "--release",
            "--bins",
            "--manifest-path", "../user-lib/Cargo.toml",
        ])
        .output();
    
    match build_user_lib_res {
        Ok(output) if output.status.success() => (),
        Err(e) => panic!("failed to build user lib: {e}"),
        // _ => panic!("failed to build user lib"),
        Ok(output) => {
            println!("stdout: {}", String::from_utf8_lossy(&output.stdout));
            panic!("stderr: {}", String::from_utf8_lossy(&output.stderr));
        }
    }

    let user_lib_toml = fs::read_to_string("../user-lib/Cargo.toml")
        .expect("cannot read user-lib Cargo.toml");
    let bins: Vec<String> = toml::from_str::<UserLibToml>(&user_lib_toml)
        .expect("cannot parse user-lib [[bin]]")
        .bin
        .into_iter()
        .map(|b| b.name)
        .collect();

    let user_lib_out_dir = "../user-lib/target/riscv64gc-unknown-none-elf/release";
    let stripped_bin_paths: Vec<String> = bins.iter().map(|bin| {
        let bin_path = format!("{user_lib_out_dir}/{bin}");
        let stripped_bin_path = format!("{bin_path}.bin");

        let strip_user_lib_res = Command::new("rust-objcopy")
            .args([
                "--binary-architecture=riscv64",
                "--strip-all",
                "-O", "binary",
                &bin_path,
                &stripped_bin_path,
            ])
            .output();

        match strip_user_lib_res {
            Ok(output) if output.status.success() => (),
            Err(e) => panic!("failed to run strip cmd for `{bin_path}`: {e}"),
            _ => panic!("failed to strip bin `{bin_path}`"),
        }
        stripped_bin_path
    }).collect();

    let link_app_asm = build_link_app_asm(bins, stripped_bin_paths);
    fs::write("src/link_app.S", link_app_asm).expect("cannot write link_app.S");

}

fn build_link_app_asm(bins: Vec<String>, stripped_bin_paths: Vec<String>) -> String {
    // Build start and end addr for each app.
    let mut link_app_asm = format!(
"    .align 3
    .section .data
    .global _num_app
_num_app:
    .quad {}\
",
        bins.len()
    );

    for i in 0..bins.len() {
        let entry = format!(
"
    .quad app_{i}_start\
"
        );
        link_app_asm.push_str(&entry);
    }
    let end_entry = format!(
"
    .quad app_{}_end
",
        bins.len()
    );
    link_app_asm.push_str(&end_entry);

    for (i, stripped_bin_path) in stripped_bin_paths.iter().enumerate() {
        let entry = format!(
"
    .section .data
    .global app_{i}_start
    .global app_{i}_end
app_{i}_start:
    .incbin \"{stripped_bin_path}\"
app_{i}_end:
"
        );
        link_app_asm.push_str(&entry);
    }

    link_app_asm
}
