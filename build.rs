fn main() {
    let src = std::env::var("SELDON_SRC").unwrap_or_else(|_| {
        let home = std::env::var("HOME").unwrap_or_else(|_| ".".into());
        format!("{home}/Git/Github/seldon")
    });
    let capi = std::path::Path::new(&src).join("src/capi.cpp");
    let inc = std::path::Path::new(&src).join("include");
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
        println!("cargo:warning=SELDON_SRC={src} has no src/capi.cpp; discrete settle only");
    }
}
