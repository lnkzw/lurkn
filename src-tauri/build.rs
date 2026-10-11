fn main() {
    // Tauri embeds its Windows manifest — the one that asks for Common
    // Controls v6 — into the app binary only. Test binaries link the same code
    // without it and die at start with STATUS_ENTRYPOINT_NOT_FOUND. So on MSVC
    // the manifest goes in through the linker, for every binary the package
    // builds, tests included, and Tauri's own copy is switched off to avoid a
    // second one. The GNU linker has no such switch; Wine, the only place that
    // target runs, does not need it.
    let windows = std::env::var("CARGO_CFG_TARGET_OS").as_deref() == Ok("windows");
    let msvc = std::env::var("CARGO_CFG_TARGET_ENV").as_deref() == Ok("msvc");

    if windows && msvc {
        let manifest =
            std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("windows-app-manifest.xml");
        println!("cargo:rerun-if-changed={}", manifest.display());
        println!("cargo:rustc-link-arg=/MANIFEST:EMBED");
        println!("cargo:rustc-link-arg=/MANIFESTINPUT:{}", manifest.display());

        let attributes = tauri_build::Attributes::new()
            .windows_attributes(tauri_build::WindowsAttributes::new_without_app_manifest());
        if let Err(error) = tauri_build::try_build(attributes) {
            eprintln!("the Tauri build script failed: {error:#}");
            std::process::exit(1);
        }
    } else {
        tauri_build::build();
    }
}
