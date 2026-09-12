fn main() {
    println!("cargo:rerun-if-changed=benches/c_nauty_helper.c");

    // The C reference implementation is only needed for the comparison
    // benchmarks; a plain `cargo build` must not require libnauty.
    if std::env::var_os("CARGO_FEATURE_C_NAUTY_BENCH").is_none() {
        return;
    }

    cc::Build::new()
        .file("benches/c_nauty_helper.c")
        .include("/usr/local/include")
        .opt_level(3)
        .compile("c_nauty_helper");

    println!("cargo:rustc-link-search=native=/usr/local/lib");
    println!("cargo:rustc-link-lib=dylib=nauty");
}
