//! `ConnectionTest` — an optional self-test an extension can declare so a
//! consumer (designer, gtdx, store) can verify a live connection/credential
//! by invoking one of the extension's own tools. `tool` names the tool (by
//! `Tool.name`) to invoke; `args` are the arguments to pass, defaulting to an
//! empty object when omitted.

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct ConnectionTest {
    /// Name of the tool (`Tool.name`) to invoke to exercise the connection.
    pub tool: String,
    /// Arguments to pass to the tool. Absent ⇒ treated as `{}`.
    #[serde(default, skip_serializing_if = "serde_json::Value::is_null")]
    pub args: serde_json::Value,
}
