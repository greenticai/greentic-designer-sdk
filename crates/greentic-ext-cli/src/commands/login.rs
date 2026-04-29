use std::path::Path;

use clap::Args as ClapArgs;
use greentic_ext_registry::credentials::Credentials;

#[derive(ClapArgs, Debug)]
pub struct Args {
    #[arg(long)]
    pub registry: Option<String>,
}

#[derive(ClapArgs, Debug)]
pub struct LogoutArgs {
    #[arg(long)]
    pub registry: Option<String>,
}

pub fn run_login(args: &Args, home: &Path) -> anyhow::Result<()> {
    let cfg = super::load_config(home)?;
    let reg_name = args.registry.as_deref().unwrap_or(&cfg.default.registry);
    let token = dialoguer::Password::new()
        .with_prompt(format!("Token for {reg_name}"))
        .interact()?;
    let creds_path = home.join("credentials.toml");
    let mut creds = Credentials::load(&creds_path).unwrap_or_default();
    creds.set(reg_name, &token);
    creds
        .save(&creds_path)
        .map_err(|e| anyhow::anyhow!("{e}"))?;
    println!("✓ logged in to {reg_name}");
    Ok(())
}

pub fn run_logout(args: &LogoutArgs, home: &Path) -> anyhow::Result<()> {
    let cfg = super::load_config(home)?;
    let reg_name = args.registry.as_deref().unwrap_or(&cfg.default.registry);
    let creds_path = home.join("credentials.toml");
    let mut creds = Credentials::load(&creds_path).unwrap_or_default();
    if creds.remove(reg_name).is_some() {
        creds
            .save(&creds_path)
            .map_err(|e| anyhow::anyhow!("{e}"))?;
        println!("✓ logged out of {reg_name}");
    } else {
        println!("no credentials for {reg_name}");
    }
    Ok(())
}
