use std::io;

/// Export the shared Rust schema manifest for the web frontend generator.
fn main() -> Result<(), Box<dyn std::error::Error>> {
    let schemas = agentics_contracts::validation::schemas::export_web_schemas()?;
    serde_json::to_writer_pretty(io::stdout().lock(), &schemas)?;
    println!();
    Ok(())
}
