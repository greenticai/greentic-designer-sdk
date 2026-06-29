use greentic_extension_sdk_contract::DescribeJson;

/// Prints a prompt showing the extension's requested permissions and returns
/// user's y/n answer. When `auto_accept` is true, always returns true (for
/// CI / scripting / `--yes` flag).
#[must_use]
pub fn confirm_install(describe: &DescribeJson, auto_accept: bool) -> bool {
    let perms = &describe.runtime.permissions;
    let requests_sensitive = !perms.network.is_empty()
        || !perms.secrets.is_empty()
        || !perms.call_extension_kinds.is_empty();
    // Pre-approved (e.g. `--yes`/CI), or nothing sensitive is requested — no
    // prompt needed, so this stays non-interactive in those cases.
    if auto_accept || !requests_sensitive {
        return true;
    }
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
    eprintln!();

    dialoguer::Confirm::new()
        .with_prompt("Install?")
        .default(false)
        .interact()
        .unwrap_or(false)
}
