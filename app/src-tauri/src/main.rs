// No console window in release builds.
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

fn main() {
    #[cfg(windows)]
    open_x3d_pro_lib::run();

    #[cfg(not(windows))]
    {
        eprintln!(
            "open-x3d-pro runs on Windows only; for UI work run `npm run dev` in app/ (mock transport)"
        );
        std::process::exit(1);
    }
}
