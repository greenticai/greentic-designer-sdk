//! `gtdx component register` — register a component-tool by URL against the
//! greentic-designer-admin tenant write endpoint.
//!
//! "Path A": this command registers a *source URL*; it does not generate or
//! sign a wrapper. The Designer introspects the component's operations later,
//! so the request body deliberately omits any `operations` field.

use std::path::Path;

use clap::{Args as ClapArgs, Subcommand};

mod client;

use client::{AdminClient, ComponentResponse, RegisterRequest, RolesRequest};

/// Roles the endpoint accepts for a registered component-tool.
const VALID_ROLES: &[&str] = &["flow_editor", "agentic_worker"];

/// Credentials key used for the designer-admin service token in
/// `~/.greentic/credentials.toml`.
const ADMIN_CREDENTIALS_KEY: &str = "greentic-admin";

#[derive(ClapArgs, Debug)]
pub struct Args {
    #[command(subcommand)]
    pub op: Op,
}

#[derive(Subcommand, Debug)]
pub enum Op {
    /// Register a component-tool by URL against greentic-designer-admin
    Register(RegisterArgs),
}

#[derive(ClapArgs, Debug)]
pub struct RegisterArgs {
    /// Store/OCI/repo URL of the component to register
    #[arg(long)]
    pub url: String,

    /// Friendly name for the component-tool (unique per tenant on the server)
    #[arg(long)]
    pub name: String,

    /// Tenant slug — sent as the `X-Greentic-Tenant` header
    #[arg(long)]
    pub tenant: String,

    /// Acting user email — sent as `X-Greentic-User`; MUST be a tenant admin
    #[arg(long)]
    pub user: String,

    /// Base URL of greentic-designer-admin (else env `GREENTIC_ADMIN_URL`)
    #[arg(long)]
    pub admin_url: Option<String>,

    /// `gts_` service key (else env `GREENTIC_ADMIN_TOKEN`, else
    /// credentials.toml key `greentic-admin`)
    #[arg(long)]
    pub admin_token: Option<String>,

    /// Optional component reference identifier
    #[arg(long)]
    pub component_ref: Option<String>,

    /// Optional component version
    #[arg(long)]
    pub component_version: Option<String>,

    /// Optional component digest
    #[arg(long)]
    pub component_digest: Option<String>,

    /// Restrict the registration to these operations (repeatable or
    /// comma-separated). Omitted = all operations allowed.
    #[arg(long, value_delimiter = ',')]
    pub allowed_ops: Vec<String>,

    /// Grant the component-tool to these roles (repeatable). Valid values:
    /// `flow_editor`, `agentic_worker`.
    #[arg(long)]
    pub role: Vec<String>,
}

/// Resolve the designer-admin base URL, reading env at the call site. Real
/// resolution precedence lives in [`pick_admin_url`] so it can be unit-tested
/// without mutating process-global env.
fn resolve_admin_url(flag: Option<&str>) -> anyhow::Result<String> {
    pick_admin_url(flag, non_empty_env("GREENTIC_ADMIN_URL").as_deref())
}

/// Pure precedence: CLI flag wins, else env value, else error.
fn pick_admin_url(flag: Option<&str>, env: Option<&str>) -> anyhow::Result<String> {
    if let Some(url) = flag.filter(|s| !s.is_empty()) {
        return Ok(url.to_string());
    }
    if let Some(url) = env.filter(|s| !s.is_empty()) {
        return Ok(url.to_string());
    }
    anyhow::bail!("no designer-admin URL: pass --admin-url <URL> or set GREENTIC_ADMIN_URL")
}

/// Resolve the `gts_` service token with precedence: CLI flag → env var →
/// stored credentials (`~/.greentic/credentials.toml` key `greentic-admin`).
/// Reads env + credentials at the call site; the precedence rule lives in the
/// pure [`pick_admin_token`] for unit testing.
fn resolve_admin_token(flag: Option<&str>, home: &Path) -> anyhow::Result<String> {
    pick_admin_token(
        flag,
        non_empty_env("GREENTIC_ADMIN_TOKEN").as_deref(),
        stored_admin_token(home).as_deref(),
        home,
    )
}

/// Pure precedence: flag → env → stored credentials, else error.
fn pick_admin_token(
    flag: Option<&str>,
    env: Option<&str>,
    stored: Option<&str>,
    home: &Path,
) -> anyhow::Result<String> {
    if let Some(token) = flag.filter(|s| !s.is_empty()) {
        return Ok(token.to_string());
    }
    if let Some(token) = env.filter(|s| !s.is_empty()) {
        return Ok(token.to_string());
    }
    if let Some(token) = stored.filter(|s| !s.is_empty()) {
        return Ok(token.to_string());
    }
    anyhow::bail!(
        "no designer-admin token: pass --admin-token <gts_…>, set GREENTIC_ADMIN_TOKEN, \
         or store one under key '{ADMIN_CREDENTIALS_KEY}' in {}",
        home.join("credentials.toml").display()
    )
}

fn stored_admin_token(home: &Path) -> Option<String> {
    use greentic_extension_sdk_registry::credentials::Credentials;
    let creds = Credentials::load(&home.join("credentials.toml")).ok()?;
    creds
        .get(ADMIN_CREDENTIALS_KEY)
        .filter(|s| !s.is_empty())
        .map(str::to_string)
}

fn non_empty_env(name: &str) -> Option<String> {
    std::env::var(name).ok().filter(|s| !s.is_empty())
}

/// Validate that every requested role is one the endpoint accepts.
fn validate_roles(roles: &[String]) -> anyhow::Result<()> {
    for role in roles {
        if !VALID_ROLES.contains(&role.as_str()) {
            anyhow::bail!(
                "invalid --role '{role}': expected one of {}",
                VALID_ROLES.join(", ")
            );
        }
    }
    Ok(())
}

/// Build the JSON request body from the parsed CLI flags. `allowed_operations`
/// is omitted when empty (meaning "all ops"); `operations` is never sent.
fn build_request(args: &RegisterArgs) -> RegisterRequest {
    RegisterRequest {
        name: args.name.clone(),
        source_url: args.url.clone(),
        component_ref: args.component_ref.clone(),
        component_version: args.component_version.clone(),
        component_digest: args.component_digest.clone(),
        allowed_operations: if args.allowed_ops.is_empty() {
            None
        } else {
            Some(args.allowed_ops.clone())
        },
        enabled: true,
    }
}

pub async fn run(args: Args, home: &Path) -> anyhow::Result<()> {
    match args.op {
        Op::Register(register_args) => run_register(register_args, home).await,
    }
}

async fn run_register(args: RegisterArgs, home: &Path) -> anyhow::Result<()> {
    validate_roles(&args.role)?;

    let admin_url = resolve_admin_url(args.admin_url.as_deref())?;
    let token = resolve_admin_token(args.admin_token.as_deref(), home)?;

    let client = AdminClient::new(&admin_url, &token, &args.tenant, &args.user)?;
    let body = build_request(&args);

    let component: ComponentResponse = client.register(&body).await?;

    if !args.role.is_empty() {
        client
            .set_roles(
                &component.id,
                &RolesRequest {
                    roles: args.role.clone(),
                },
            )
            .await?;
    }

    let roles_summary = if args.role.is_empty() {
        "(default)".to_string()
    } else {
        args.role.join(", ")
    };
    println!(
        "✓ registered component-tool '{}' (id={}) url={} roles={}",
        args.name, component.id, args.url, roles_summary
    );
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn base_args() -> RegisterArgs {
        RegisterArgs {
            url: "https://store.example/comp.wasm".into(),
            name: "my-tool".into(),
            tenant: "acme".into(),
            user: "admin@acme.test".into(),
            admin_url: None,
            admin_token: None,
            component_ref: None,
            component_version: None,
            component_digest: None,
            allowed_ops: Vec::new(),
            role: Vec::new(),
        }
    }

    // Precedence is unit-tested against the pure `pick_*` helpers so we never
    // mutate process-global env (which would be `unsafe` under edition 2024 and
    // race across parallel tests). The thin `resolve_*` wrappers only read env.

    #[test]
    fn admin_url_prefers_flag_over_env() {
        let url =
            pick_admin_url(Some("https://flag.example"), Some("https://env.example")).unwrap();
        assert_eq!(url, "https://flag.example");
    }

    #[test]
    fn admin_url_falls_back_to_env() {
        let url = pick_admin_url(None, Some("https://env-only.example")).unwrap();
        assert_eq!(url, "https://env-only.example");
    }

    #[test]
    fn admin_url_missing_is_error() {
        assert!(pick_admin_url(None, None).is_err());
        // An empty flag/env is treated as absent, not as a valid URL.
        assert!(pick_admin_url(Some(""), Some("")).is_err());
    }

    #[test]
    fn token_prefers_flag_over_env_and_creds() {
        let home = std::path::Path::new("/nonexistent");
        let token =
            pick_admin_token(Some("gts_flag"), Some("gts_env"), Some("gts_stored"), home).unwrap();
        assert_eq!(token, "gts_flag");
    }

    #[test]
    fn token_prefers_env_over_creds() {
        let home = std::path::Path::new("/nonexistent");
        let token = pick_admin_token(None, Some("gts_env"), Some("gts_stored"), home).unwrap();
        assert_eq!(token, "gts_env");
    }

    #[test]
    fn token_falls_back_to_stored_credentials() {
        let home = std::path::Path::new("/nonexistent");
        let token = pick_admin_token(None, None, Some("gts_stored"), home).unwrap();
        assert_eq!(token, "gts_stored");
    }

    #[test]
    fn token_missing_everywhere_is_error() {
        let home = std::path::Path::new("/nonexistent");
        assert!(pick_admin_token(None, None, None, home).is_err());
        // Empty strings are treated as absent across every source.
        assert!(pick_admin_token(Some(""), Some(""), Some(""), home).is_err());
    }

    #[test]
    fn stored_admin_token_reads_credentials_file() {
        // Exercises the real credentials.toml read path used by `resolve_admin_token`.
        use greentic_extension_sdk_registry::credentials::Credentials;
        let tmp = tempfile::TempDir::new().unwrap();
        let home = tmp.path();
        assert!(stored_admin_token(home).is_none());

        let mut creds = Credentials::default();
        creds.set(ADMIN_CREDENTIALS_KEY, "gts_stored");
        creds.save(&home.join("credentials.toml")).unwrap();
        assert_eq!(stored_admin_token(home).as_deref(), Some("gts_stored"));
    }

    #[test]
    fn valid_roles_pass_validation() {
        assert!(validate_roles(&["flow_editor".into()]).is_ok());
        assert!(validate_roles(&["agentic_worker".into()]).is_ok());
        assert!(validate_roles(&["flow_editor".into(), "agentic_worker".into()]).is_ok());
        assert!(validate_roles(&[]).is_ok());
    }

    #[test]
    fn unknown_role_is_rejected() {
        let err = validate_roles(&["super_admin".into()]).unwrap_err();
        assert!(err.to_string().contains("super_admin"));
    }

    #[test]
    fn request_omits_allowed_operations_when_empty() {
        let req = build_request(&base_args());
        let json = serde_json::to_value(&req).unwrap();
        assert_eq!(json["name"], "my-tool");
        assert_eq!(json["source_url"], "https://store.example/comp.wasm");
        assert_eq!(json["enabled"], true);
        // Empty allowed_ops means "all" → field absent.
        assert!(json.get("allowed_operations").is_none());
        // Path A: the Designer introspects ops; we never send `operations`.
        assert!(json.get("operations").is_none());
        // Optional fields absent when not provided.
        assert!(json.get("component_ref").is_none());
        assert!(json.get("component_version").is_none());
        assert!(json.get("component_digest").is_none());
    }

    #[test]
    fn request_includes_allowed_operations_when_set() {
        let mut args = base_args();
        args.allowed_ops = vec!["read".into(), "write".into()];
        args.component_ref = Some("ref-1".into());
        args.component_version = Some("1.0.0".into());
        args.component_digest = Some("sha256:abc".into());
        let req = build_request(&args);
        let json = serde_json::to_value(&req).unwrap();
        assert_eq!(
            json["allowed_operations"],
            serde_json::json!(["read", "write"])
        );
        assert_eq!(json["component_ref"], "ref-1");
        assert_eq!(json["component_version"], "1.0.0");
        assert_eq!(json["component_digest"], "sha256:abc");
        assert!(json.get("operations").is_none());
    }
}
