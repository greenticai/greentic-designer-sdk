use std::path::Path;

use anyhow::Context as _;
use clap::Args as ClapArgs;

#[derive(ClapArgs, Debug)]
pub struct Args {
    /// Path to an extension source directory containing describe.json
    #[arg(default_value = ".")]
    pub path: String,
}

pub fn run(args: &Args, _home: &Path) -> anyhow::Result<()> {
    let describe_path = Path::new(&args.path).join("describe.json");
    // Previously a bare `?`, which printed "No such file or directory (os
    // error 2)" without naming the file this command exists to read.
    let bytes = std::fs::read(&describe_path).with_context(|| {
        format!(
            "no describe.json at {}. Run `gtdx validate <dir>` from an extension \
             project, or scaffold one with `gtdx new`",
            describe_path.display()
        )
    })?;
    let value: serde_json::Value = serde_json::from_slice(&bytes)
        .with_context(|| format!("{} is not valid JSON", describe_path.display()))?;
    greentic_extension_sdk_contract::schema::validate_describe_json(&value)
        .map_err(|e| anyhow::anyhow!("{e}"))?;
    let _: greentic_extension_sdk_contract::DescribeJson = serde_json::from_value(value)?;
    println!("✓ {} valid", describe_path.display());
    Ok(())
}
