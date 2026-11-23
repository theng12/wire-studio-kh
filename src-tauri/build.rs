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
    tauri_build::build()
}
