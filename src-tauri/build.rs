fn main() {
    // IsSecureEventInputEnabled lives in Carbon (see inject.rs).
    #[cfg(target_os = "macos")]
    println!("cargo:rustc-link-lib=framework=Carbon");
    tauri_build::build()
}
