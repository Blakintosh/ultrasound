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

    // The exe's icon, shown in Explorer and on the console window's title bar and taskbar button.
    if env::var("CARGO_CFG_TARGET_OS").as_deref() == Ok("windows") {
        println!("cargo:rerun-if-changed=assets/ultrasound.rc");
        println!("cargo:rerun-if-changed=assets/ultrasound.ico");
        embed_resource::compile("assets/ultrasound.rc", embed_resource::NONE)
            .manifest_required()
            .expect("Failed to embed the icon");
    }
}
