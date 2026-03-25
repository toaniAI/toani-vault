use std::env;
use std::fs;
use std::path::PathBuf;

fn main() {
    println!("cargo:rerun-if-changed=Enclave.edl");
    println!("cargo:rerun-if-changed=src/lib.rs");
    println!("cargo:rerun-if-changed=src/ecalls.rs");

    let out_dir = PathBuf::from(env::var("OUT_DIR").expect("OUT_DIR must exist"));
    let metadata = out_dir.join("phase-a-enclave-build.txt");
    let content = "\
Phase A SGX enclave skeleton build.
The real EDL codegen, signing, and trusted runtime build happen in Linux SGX CI or TEE Docker.
";
    fs::write(metadata, content).expect("write SGX build metadata");
}
