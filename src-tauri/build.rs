fn main() {
    let target_os = std::env::var("CARGO_CFG_TARGET_OS").unwrap_or_default();
    let target_env = std::env::var("CARGO_CFG_TARGET_ENV").unwrap_or_default();
    if target_os == "windows" && target_env == "msvc" {
        windows_msvc_build();
    } else {
        tauri_build::build()
    }
}

/// tauri-build embeds its app manifest (Common Controls v6) only into the
/// `[[bin]]`: embed-resource links it with `rustc-link-arg-bins`. The `cargo test`
/// binaries then load comctl32 v5, which lacks `TaskDialogIndirect`, and exit
/// with STATUS_ENTRYPOINT_NOT_FOUND (0xc0000139). So we skip tauri-build's
/// manifest and embed the same file into every linked artifact (app, cdylib,
/// tests) — the workaround from https://github.com/tauri-apps/tauri/issues/13419
/// and tauri's own `crates/tauri/build.rs` (`embed_manifest_for_tests`).
/// `windows-app-manifest.xml` is a copy of tauri-build's default manifest.
fn windows_msvc_build() {
    let manifest = std::path::Path::new(&std::env::var("CARGO_MANIFEST_DIR").unwrap())
        .join("windows-app-manifest.xml");
    println!("cargo:rerun-if-changed={}", manifest.display());
    println!("cargo:rustc-link-arg=/MANIFEST:EMBED");
    println!("cargo:rustc-link-arg=/MANIFESTINPUT:{}", manifest.display());

    let attributes = tauri_build::Attributes::new()
        .windows_attributes(tauri_build::WindowsAttributes::new_without_app_manifest());
    if let Err(error) = tauri_build::try_build(attributes) {
        panic!("tauri-build failed: {error:#}");
    }
}
