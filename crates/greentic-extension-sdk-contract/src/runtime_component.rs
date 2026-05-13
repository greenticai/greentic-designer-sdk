//! `RuntimeComponent` — one entry in `Runtime.components`. At least one of
//! `oci_ref` or `gtpack` must be present; both may be present (OCI is the
//! preferred channel, gtpack is the offline fallback).

use serde::{Deserialize, Deserializer, Serialize};

use crate::describe::RuntimeGtpack;
use crate::sha256::Sha256;

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct RuntimeComponent {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub oci_ref: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub gtpack: Option<RuntimeGtpack>,
    pub sha256: Sha256,
    pub world: String,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct RuntimeComponentRaw {
    #[serde(default)]
    oci_ref: Option<String>,
    #[serde(default)]
    gtpack: Option<RuntimeGtpack>,
    sha256: Sha256,
    world: String,
}

impl<'de> Deserialize<'de> for RuntimeComponent {
    fn deserialize<D: Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        let raw = RuntimeComponentRaw::deserialize(d)?;
        if raw.oci_ref.is_none() && raw.gtpack.is_none() {
            return Err(serde::de::Error::custom(
                "RuntimeComponent requires at least one of oci_ref or gtpack",
            ));
        }
        Ok(Self {
            oci_ref: raw.oci_ref,
            gtpack: raw.gtpack,
            sha256: raw.sha256,
            world: raw.world,
        })
    }
}
