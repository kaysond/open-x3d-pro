//! Regenerates `core/testdata/vectors.json`: `cargo run -p x3d-core --bin gen-vectors`.

use std::path::Path;
use std::process::ExitCode;

fn main() -> ExitCode {
    let path = Path::new(env!("CARGO_MANIFEST_DIR")).join(x3d_core::vectors::PATH);
    let result = x3d_core::vectors::generate().and_then(|json| {
        std::fs::create_dir_all(path.parent().unwrap_or(Path::new(".")))?;
        std::fs::write(&path, json)?;
        Ok(())
    });
    match result {
        Ok(()) => {
            println!("wrote {}", path.display());
            ExitCode::SUCCESS
        }
        Err(e) => {
            eprintln!("gen-vectors: {e}");
            ExitCode::FAILURE
        }
    }
}
