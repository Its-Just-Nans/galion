use std::env;
use std::fs;
use std::path::PathBuf;
use std::process::Command;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let target_triple = env::var("TARGET")?;
    let out_dir = env::var("OUT_DIR")?;
    let out_path = PathBuf::from(&out_dir);

    // docs.rs blocks network builds, so generate empty bindings for documentation
    if env::var("DOCS_RS").is_ok() {
        fs::write(out_path.join("bindings.rs"), "")?;
        return Ok(());
    }

    // Re-run build script if these files change
    println!("cargo:rerun-if-changed=go.mod");
    println!("cargo:rerun-if-changed=go.sum");

    // Build the Go static library
    let lib_path = out_path.join("librclone.a");
    let status = Command::new("go")
        .args(["build", "--buildmode=c-archive", "-o"])
        .arg(&lib_path)
        .arg("github.com/rclone/rclone/librclone")
        .status()?;

    if !status.success() {
        return Err("`go build` failed. Ensure Go is installed and up-to-date.".into());
    }

    // Tell Rust where to find and link the library
    println!("cargo:rustc-link-search=native={}", out_dir);
    println!("cargo:rustc-link-lib=static=rclone");

    // macOS-specific frameworks
    if target_triple.ends_with("darwin") {
        for framework in &["CoreFoundation", "IOKit", "Security"] {
            println!("cargo:rustc-link-lib=framework={}", framework);
        }
        println!("cargo:rustc-link-lib=resolv");
    }

    // Generate Rust bindings using bindgen
    let bindings = bindgen::Builder::default()
        .header(out_path.join("librclone.h").to_string_lossy())
        .allowlist_function("RcloneRPC")
        .allowlist_function("RcloneInitialize")
        .allowlist_function("RcloneFinalize")
        .allowlist_function("RcloneFreeString")
        .parse_callbacks(Box::new(bindgen::CargoCallbacks::new()))
        .generate()?;

    bindings.write_to_file(out_path.join("bindings.rs"))?;

    Ok(())
}
