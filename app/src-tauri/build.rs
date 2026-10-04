fn main() {
    // tauri-build is a Windows-only build dependency (see Cargo.toml).
    #[cfg(windows)]
    tauri_build::build();
}
