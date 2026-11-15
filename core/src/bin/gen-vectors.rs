//! Regenerates `core/testdata/vectors.json`: `cargo run -p x3d-core --bin gen-vectors`.

use std::path::Path;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let path = Path::new(env!("CARGO_MANIFEST_DIR")).join(x3d_core::vectors::PATH);
    std::fs::write(&path, x3d_core::vectors::generate()?)?;
    println!("wrote {}", path.display());
    Ok(())
}
