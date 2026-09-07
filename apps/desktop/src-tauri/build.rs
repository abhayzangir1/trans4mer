use std::env;
use std::path::PathBuf;

fn main() {
    // Prepend local bin folder containing windres shim to PATH to prevent GNU windres
    // from crashing on paths with spaces during Windows resource embedding.
    let bin_dir = PathBuf::from(env::var("CARGO_MANIFEST_DIR").unwrap()).join("bin");
    if bin_dir.exists() {
        if let Ok(path) = env::var("PATH") {
            let new_path = format!("{};{}", bin_dir.display(), path);
            env::set_var("PATH", new_path);
        }
    }

    tauri_build::build();
}
