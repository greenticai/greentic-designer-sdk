use std::path::Path;

use clap::Args as ClapArgs;

#[derive(ClapArgs, Debug)]
pub struct Args {
    /// Path to an extension source directory containing describe.json
    #[arg(default_value = ".")]
    pub path: String,
}

pub fn run(args: &Args, _home: &Path) -> anyhow::Result<()> {
    let describe_path = Path::new(&args.path).join("describe.json");
    let bytes = std::fs::read(&describe_path)?;
    let value: serde_json::Value = serde_json::from_slice(&bytes)?;
    greentic_extension_sdk_contract::schema::validate_describe_json(&value)
        .map_err(|e| anyhow::anyhow!("{e}"))?;
    let describe: greentic_extension_sdk_contract::DescribeJson = serde_json::from_value(value)?;
    let contributions = describe
        .typed_contributions()
        .map_err(|e| anyhow::anyhow!("{e}"))?;
    println!("✓ {} valid", describe_path.display());
    if !contributions.node_types.is_empty() {
        println!("  nodeTypes: {} valid", contributions.node_types.len());
    }
    Ok(())
}
