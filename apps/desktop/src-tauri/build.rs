use std::env;
use std::path::PathBuf;

fn main() {
    if let Ok(manifest_dir) = env::var("CARGO_MANIFEST_DIR") {
        let bin_dir = PathBuf::from(manifest_dir).join("bin");
        if bin_dir.exists() {
            if let Ok(path) = env::var("PATH") {
                let new_path = format!("{};{}", bin_dir.display(), path);
                env::set_var("PATH", new_path);
            }
        }
    }

    tauri_build::build();
}
