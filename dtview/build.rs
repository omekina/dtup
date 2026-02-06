use std::{env, io::Read, path::PathBuf, process::Stdio};

fn main() {
    println!("cargo::rerun-if-changed=frontend");
    let mut bundle = String::new();
    let mut command = std::process::Command::new("bun")
        .current_dir(env!("CARGO_MANIFEST_DIR"))
        .args(["build", "frontend/main.ts", "--minify"])
        .stdout(Stdio::piped())
        .spawn()
        .unwrap();
    command
        .stdout
        .take()
        .unwrap()
        .read_to_string(&mut bundle)
        .unwrap();
    std::fs::write(
        &PathBuf::from(env::var("OUT_DIR").unwrap()).join("build.ts"),
        bundle,
    )
    .unwrap();
}
