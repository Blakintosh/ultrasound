use std::env;
use std::fs;
use std::path::Path;

fn main() {
    let manifest_dir = env!("CARGO_MANIFEST_DIR");
    println!("cargo:rustc-link-lib=dylib=libFLAC");
    println!("cargo:rustc-link-search=native={}/lib", manifest_dir);

    // Copy libFLAC.dll to the output directory
    let out_dir = env::var("OUT_DIR").unwrap();
    // OUT_DIR is something like target/debug/build/ultrasound-xxx/out
    // The exe lands in target/debug/, so go up 3 levels
    let target_dir = Path::new(&out_dir).ancestors().nth(3).unwrap();

    let src = Path::new(manifest_dir).join("lib/libFLAC.dll");
    let dst = target_dir.join("libFLAC.dll");
    fs::copy(&src, &dst).expect("Failed to copy libFLAC.dll");
}
