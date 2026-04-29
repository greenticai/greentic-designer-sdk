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
    greentic_ext_contract::schema::validate_describe_json(&value)
        .map_err(|e| anyhow::anyhow!("{e}"))?;
    let _: greentic_ext_contract::DescribeJson = serde_json::from_value(value)?;
    println!("✓ {} valid", describe_path.display());
    Ok(())
}
