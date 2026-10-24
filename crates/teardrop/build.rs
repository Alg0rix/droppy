fn main() {
    println!("cargo:rerun-if-changed=res/teardrop.rc");
    println!("cargo:rerun-if-changed=res/teardrop.manifest");
    println!("cargo:rerun-if-changed=res/teardrop.ico");
    // Only Windows targets get the icon, manifest (Per-Monitor-V2 DPI) and version info.
    if std::env::var("CARGO_CFG_TARGET_OS").as_deref() == Ok("windows") {
        embed_resource::compile("res/teardrop.rc", embed_resource::NONE);
        if std::env::var("CARGO_CFG_TARGET_ENV").as_deref() == Ok("msvc") {
            println!("cargo:rustc-link-arg-bins=/DEPENDENTLOADFLAG:0x800");
        }
    }
}
