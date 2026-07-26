use std::path::Path;

fn watch_frontend(path: &Path) {
    println!("cargo:rerun-if-changed={}", path.display());
    if let Ok(entries) = std::fs::read_dir(path) {
        for entry in entries.flatten() {
            let child = entry.path();
            if child.is_dir() {
                watch_frontend(&child);
            } else {
                println!("cargo:rerun-if-changed={}", child.display());
            }
        }
    }
}

fn main() {
    watch_frontend(Path::new("../dist"));
    tauri_build::build()
}
