use goose::config::schema::GooseConfigSchema;
use schemars::schema_for;
use std::{env, fs, path::PathBuf};

fn main() {
    let schema = schema_for!(GooseConfigSchema);
    let json = serde_json::to_string_pretty(&schema).expect("failed to serialize schema");

    let dir = env::var("CARGO_MANIFEST_DIR").unwrap();
    let path = PathBuf::from(&dir).join("config.schema.json");
    fs::write(&path, format!("{json}\n")).expect("failed to write schema file");
    eprintln!("Generated config schema at {}", path.display());
}
