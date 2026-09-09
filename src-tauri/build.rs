fn main() {
    // Track additions/removals as well as the individual resource files Tauri tracks.
    println!("cargo:rerun-if-changed=resources/cvc5");
    println!("cargo:rerun-if-changed=cvc5-package.json");
    tauri_build::build();
}
