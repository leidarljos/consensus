fn main() {
    // Linking Seldon's sources is a local option. A normal install execs
    // `seldon` when it is on PATH and does not look for a checkout.
    println!("cargo:rerun-if-env-changed=SELDON_SRC");
    let Ok(src) = std::env::var("SELDON_SRC") else {
        return;
    };
    let root = std::path::Path::new(&src);
    let capi = root.join("src/capi.cpp");
    let inc = root.join("include");
    if capi.is_file() {
        println!("cargo:rerun-if-changed={}", capi.display());
        cc::Build::new()
            .cpp(true)
            .flag_if_supported("-std=c++20")
            .include(&inc)
            .file(&capi)
            .compile("seldon_capi");
        println!("cargo:rustc-cfg=seldon_capi");
    } else {
        println!(
            "cargo:warning=SELDON_SRC={src} has no src/capi.cpp; the binary execs seldon instead"
        );
    }
}
