//! `DwProvider` contribution: a DW Composer provider card an extension offers.
//!
//! Mirrors the fields the designer's `/api/dw/providers` `EntryView` serves, so
//! an installed extension can publish a provider choice (e.g. an LLM backend)
//! without the designer carrying a static catalog entry for it.

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct DwProvider {
    /// Stable id, e.g. `provider.llm.openai.chat`.
    pub provider_id: String,
    /// Provider family, e.g. `llm`.
    pub family: String,
    pub display_name: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub summary: Option<String>,
    pub version: String,
    pub channel: String,
    /// Capability contract ids this provider satisfies, e.g. `["cap://llm/chat"]`.
    pub capability_contract_ids: Vec<String>,
    /// Setup questions rendered by the designer's `QuestionBlockField`. Kept as
    /// raw JSON objects to match the designer's untyped `question_blocks`
    /// (`Vec<serde_json::Value>`) — each is `{ block_id, kind, ... }`.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub question_blocks: Vec<serde_json::Value>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub template_compatibility: Vec<String>,
}

#[cfg(test)]
mod tests {
    use super::DwProvider;
    use serde_json::json;

    #[test]
    fn round_trips_full_entry() {
        let value = json!({
            "providerId": "provider.llm.openai.chat",
            "family": "llm",
            "displayName": "OpenAI Chat",
            "summary": "Managed chat LLM provider.",
            "version": "0.5.0",
            "channel": "stable",
            "capabilityContractIds": ["cap://llm/chat"],
            "questionBlocks": [
                { "block_id": "provider.llm.chat.openai", "kind": "secret" },
                { "block_id": "model", "kind": "text" }
            ],
            "templateCompatibility": ["dw.support-assistant"]
        });
        let parsed: DwProvider = serde_json::from_value(value.clone()).unwrap();
        assert_eq!(parsed.provider_id, "provider.llm.openai.chat");
        assert_eq!(parsed.capability_contract_ids, vec!["cap://llm/chat"]);
        assert_eq!(parsed.question_blocks.len(), 2);
        assert_eq!(serde_json::to_value(&parsed).unwrap(), value);
    }

    #[test]
    fn minimal_entry_defaults_optional_fields() {
        let value = json!({
            "providerId": "p", "family": "llm", "displayName": "P",
            "version": "1.0.0", "channel": "research",
            "capabilityContractIds": ["cap://llm/chat"]
        });
        let parsed: DwProvider = serde_json::from_value(value).unwrap();
        assert!(parsed.summary.is_none());
        assert!(parsed.question_blocks.is_empty());
        assert!(parsed.template_compatibility.is_empty());
    }

    #[test]
    fn rejects_unknown_field() {
        let value = json!({
            "providerId": "p", "family": "llm", "displayName": "P",
            "version": "1.0.0", "channel": "research",
            "capabilityContractIds": [], "bogus": true
        });
        assert!(serde_json::from_value::<DwProvider>(value).is_err());
    }
}
