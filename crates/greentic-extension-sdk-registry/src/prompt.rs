use greentic_extension_sdk_contract::DescribeJson;
use greentic_extension_sdk_contract::describe::Permissions;

/// Does this permission set warrant an interactive consent prompt?
///
/// Destructured deliberately: adding a field to `Permissions` then breaks this
/// function to compile, forcing a decision about whether it is consent-worthy.
/// The previous form listed three fields positively and silently ignored
/// `llmRoles` and `oauthProviders` — so an extension requesting OAuth tokens
/// on the user's behalf installed with no prompt at all (audit finding).
#[must_use]
fn requests_sensitive(perms: &Permissions) -> bool {
    let Permissions {
        network,
        secrets,
        call_extension_kinds,
        llm_roles,
        oauth_providers,
    } = perms;
    !network.is_empty()
        || !secrets.is_empty()
        || !call_extension_kinds.is_empty()
        || !llm_roles.is_empty()
        || !oauth_providers.is_empty()
}

/// Prints a prompt showing the extension's requested permissions and returns
/// user's y/n answer. When `auto_accept` is true, always returns true (for
/// CI / scripting / `--yes` flag).
#[must_use]
pub fn confirm_install(describe: &DescribeJson, auto_accept: bool) -> bool {
    let perms = &describe.runtime.permissions;
    // Pre-approved (e.g. `--yes`/CI), or nothing sensitive is requested — no
    // prompt needed, so this stays non-interactive in those cases.
    if auto_accept || !requests_sensitive(perms) {
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
    if !perms.oauth_providers.is_empty() {
        eprintln!(
            "  OAuth: may request access tokens for {}",
            perms.oauth_providers.join(", ")
        );
    }
    if !perms.llm_roles.is_empty() {
        eprintln!(
            "  LLM: may call the host LLM as {}",
            perms.llm_roles.join(", ")
        );
    }
    eprintln!();

    dialoguer::Confirm::new()
        .with_prompt("Install?")
        .default(false)
        .interact()
        .unwrap_or(false)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn empty_permissions_need_no_prompt() {
        assert!(!requests_sensitive(&Permissions::default()));
    }

    /// The regression this function was rewritten for: `oauthProviders` grants
    /// the extension live OAuth tokens on the user's behalf, and used to
    /// install silently.
    #[test]
    fn oauth_providers_alone_require_consent() {
        let perms = Permissions {
            oauth_providers: vec!["hubspot".to_string()],
            ..Permissions::default()
        };
        assert!(requests_sensitive(&perms));
    }

    #[test]
    fn llm_roles_alone_require_consent() {
        let perms = Permissions {
            llm_roles: vec!["composer".to_string()],
            ..Permissions::default()
        };
        assert!(requests_sensitive(&perms));
    }

    #[test]
    fn each_previously_covered_field_still_requires_consent() {
        for perms in [
            Permissions {
                network: vec!["api.example.com".to_string()],
                ..Permissions::default()
            },
            Permissions {
                secrets: vec!["greentic:secret/prod-db".to_string()],
                ..Permissions::default()
            },
            Permissions {
                call_extension_kinds: vec!["design".to_string()],
                ..Permissions::default()
            },
        ] {
            assert!(requests_sensitive(&perms));
        }
    }
}
