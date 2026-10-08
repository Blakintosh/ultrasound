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
        println!("cargo:rerun-if-changed=build.rs");
        println!("cargo:rerun-if-changed=assets/ultrasound.ico");
        println!("cargo:rerun-if-changed=Cargo.toml");
        // The icon plus version info. The installer and the self-update recognise an Ultrasound build by its
        // product name, and read its version, so they're generated from Cargo.toml rather than kept by hand.
        let version = env::var("CARGO_PKG_VERSION").unwrap();
        let mut parts: Vec<u16> = version
            .split(['.', '-', '+'])
            .take(3)
            .map(|p| p.parse().unwrap_or(0))
            .collect();
        parts.resize(4, 0);
        let numeric = parts.iter().map(u16::to_string).collect::<Vec<_>>().join(",");
        let icon = Path::new(manifest_dir).join("assets/ultrasound.ico");
        let rc = Path::new(&out_dir).join("ultrasound.rc");
        fs::write(
            &rc,
            format!(
                r#"1 ICON "{icon}"
1 VERSIONINFO
FILEVERSION {numeric}
PRODUCTVERSION {numeric}
FILEOS 0x40004
FILETYPE 0x1
BEGIN
  BLOCK "StringFileInfo"
  BEGIN
    BLOCK "040904B0"
    BEGIN
      VALUE "CompanyName", "Blakintosh"
      VALUE "FileDescription", "Ultrasound"
      VALUE "FileVersion", "{version}"
      VALUE "InternalName", "snd_convert"
      VALUE "OriginalFilename", "snd_convert.exe"
      VALUE "ProductName", "Ultrasound"
      VALUE "ProductVersion", "{version}"
    END
  END
  BLOCK "VarFileInfo"
  BEGIN
    VALUE "Translation", 0x409, 1200
  END
END
"#,
                icon = icon.display().to_string().replace('\\', "/"),
            ),
        )
        .expect("Failed to write the resource script");
        embed_resource::compile(&rc, embed_resource::NONE)
            .manifest_required()
            .expect("Failed to embed the icon and version info");
    }
}
