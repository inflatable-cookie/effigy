#[path = "../qa/ratchet.rs"]
mod ratchet;
use std::path::Path;
fn main() -> Result<(), Box<dyn std::error::Error>> {
    let root = Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .unwrap()
        .parent()
        .unwrap();
    let current = ratchet::inventory(root)?;
    if std::env::args().any(|arg| arg == "--inventory") {
        std::fs::write(
            root.join("config/kernel-ratchet.json"),
            format!("{}\n", serde_json::to_string_pretty(&current)?),
        )?;
        println!("wrote config/kernel-ratchet.json");
        return Ok(());
    }
    let baseline: ratchet::Inventory =
        serde_json::from_slice(&std::fs::read(root.join("config/kernel-ratchet.json"))?)?;
    current.compare(&baseline)?;
    println!(
        "kernel ratchet: production counts match baseline; {} test files reported separately",
        current.tests.len()
    );
    Ok(())
}
