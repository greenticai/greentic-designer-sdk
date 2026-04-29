use greentic_ext_contract::DescribeJson;

/// Prints a prompt showing the extension's requested permissions and returns
/// user's y/n answer. When `auto_accept` is true, always returns true (for
/// CI / scripting / `--yes` flag).
#[must_use]
pub fn confirm_install(describe: &DescribeJson, auto_accept: bool) -> bool {
    if auto_accept {
        return true;
    }
    let perms = &describe.runtime.permissions;
    eprintln!();
    eprintln!(
        "⚠️  Extension {} v{} requests:",
        describe.metadata.id, describe.metadata.version
    );
    if !perms.network.is_empty() {
        eprintln!("  Network: {}", perms.network.join(", "));
    }
    if !perms.secrets.is_empty() {
        eprintln!("  Secrets: {}", perms.secrets.join(", "));
    }
    if !perms.call_extension_kinds.is_empty() {
        eprintln!(
            "  Cross-extension: may call {} extensions",
            perms.call_extension_kinds.join(", ")
        );
    }
    if perms.network.is_empty() && perms.secrets.is_empty() && perms.call_extension_kinds.is_empty()
    {
        eprintln!("  (no special permissions)");
    }
    eprintln!();

    dialoguer::Confirm::new()
        .with_prompt("Install?")
        .default(false)
        .interact()
        .unwrap_or(false)
}
