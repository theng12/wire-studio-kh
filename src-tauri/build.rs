fn main() {
    // Sparkle through the shared shim: a vendored framework plus a universal static library, linked
    // from cargo because this bundle is built by cargo, not Xcode. The rpath is not optional — without
    // it the app links and dies at launch with `Library not loaded: @rpath/Sparkle.framework/…`.
    let vendor = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("vendor/khsparkle");
    println!("cargo:rerun-if-changed={}", vendor.join("libkhsparkle.a").display());
    println!("cargo:rustc-link-search=native={}", vendor.display());
    println!("cargo:rustc-link-lib=static=khsparkle");
    println!("cargo:rustc-link-search=framework={}", vendor.display());
    println!("cargo:rustc-link-lib=framework=Sparkle");
    println!("cargo:rustc-link-lib=framework=Foundation");
    println!("cargo:rustc-link-arg=-Wl,-rpath,@executable_path/../Frameworks");
    // A dev or test binary sits in <target>/debug or <target>/release, so `@executable_path/../Frameworks`
    // resolves to <target>/Frameworks. Put the framework there: `/usr/bin/env` strips DYLD_* from the
    // inherited environment (measured 2026-11-23), so DYLD_FRAMEWORK_PATH alone cannot be relied on.
    if let Ok(out) = std::env::var("OUT_DIR") {
        if let Some(target) = std::path::Path::new(&out).ancestors().nth(4).and_then(|p| p.parent()) {
            let dest = target.join("Frameworks");
            let link = dest.join("Sparkle.framework");
            if std::fs::create_dir_all(&dest).is_ok() && !link.exists() {
                let _ = std::os::unix::fs::symlink(vendor.join("Sparkle.framework"), &link);
            }
        }
    }
    tauri_build::build()
}
