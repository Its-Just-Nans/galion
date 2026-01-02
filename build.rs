use std::path::PathBuf;
use std::process::Command;
use std::{env, fs};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let lib_name = "librclone";
    let rclone_repo = format!("github.com/rclone/rclone/{}", lib_name);
    let target_triple = env::var("TARGET")?;
    let out_path = PathBuf::from(env::var("OUT_DIR")?).join(lib_name);

    // return early for docs.rs
    if env::var("DOCS_RS").is_ok() {
        std::fs::write(out_path.join("bindings.rs"), "")?;
        return Ok(());
    }

    fs::create_dir_all(&out_path)?;

    // Re-run build script if these files change
    println!(
        "cargo:rerun-if-changed={}",
        out_path.join("go.mod").display()
    );
    println!(
        "cargo:rerun-if-changed={}",
        out_path.join("go.sum").display()
    );

    if !out_path.join(format!("{lib_name}.go")).exists() {
        let mut gofile_buf = String::new();
        gofile_buf.push_str("package main\n\n");
        gofile_buf.push_str(&format!("import \"{}\"", rclone_repo));
        std::fs::write(out_path.join(format!("{lib_name}.go")), gofile_buf)?;
    }

    // Build the Go static library
    if !out_path.join("go.mod").exists() {
        Command::new("go")
            .current_dir(&out_path)
            .args(["mod", "init", "github.com/Its-Just-Nans/galion"])
            .status()?;
        Command::new("go")
            .current_dir(&out_path)
            .args(["get", &rclone_repo])
            .status()?;
        Command::new("go")
            .current_dir(&out_path)
            .args(["mod", "tidy", "-go=1.24.4"])
            .status()?;
    }
    if !out_path.join(format!("{lib_name}.a")).exists() {
        let status = Command::new("go")
            .current_dir(&out_path)
            .args(["build", "--buildmode=c-archive", "-o"])
            .arg(out_path.join(format!("{lib_name}.a")))
            .arg(rclone_repo)
            .status()?;

        if !status.success() {
            return Err("`go build` failed. Ensure Go is installed and up-to-date.".into());
        }
    }

    // Tell Rust where to find and link the library
    println!("cargo:rustc-link-search=native={}", out_path.display());
    println!("cargo:rustc-link-lib=static=rclone");

    // macOS-specific frameworks
    if target_triple.ends_with("darwin") {
        for framework in &["CoreFoundation", "IOKit", "Security"] {
            println!("cargo:rustc-link-lib=framework={}", framework);
        }
        println!("cargo:rustc-link-lib=resolv");
    }

    // Generate Rust bindings using bindgen
    bindgen::Builder::default()
        .header(out_path.join(format!("{}.h", lib_name)).to_string_lossy())
        .allowlist_function("RcloneRPC")
        .allowlist_function("RcloneInitialize")
        .allowlist_function("RcloneFinalize")
        .allowlist_function("RcloneFreeString")
        .parse_callbacks(Box::new(bindgen::CargoCallbacks::new()))
        .generate()?
        .write_to_file(out_path.join("bindings.rs"))?;

    Ok(())
}
