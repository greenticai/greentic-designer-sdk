//! `.gtxpack` builder: stages describe + wasm + assets and hands off to the
//! shared `greentic-extension-sdk-contract::pack_writer` for deterministic ZIP emission.

use std::path::{Path, PathBuf};

use greentic_extension_sdk_contract::pack_writer::{
    PackEntry, build_gtxpack_with_manifest, sha256_hex,
};
use greentic_extension_sdk_contract::{DescribeJson, bind_manifest, sign_describe};
use walkdir::WalkDir;

/// Summary of a packed `.gtxpack`.
#[derive(Debug, Clone)]
pub struct PackInfo {
    pub pack_path: PathBuf,
    pub pack_name: String,
    pub size: u64,
    pub sha256: String,
    pub ext_name: String,
    pub ext_version: String,
    #[allow(dead_code)] // Reserved for richer InstallOk events in Phase 2.
    pub ext_kind: String,
    /// The final `describe.json` bytes written into the pack — manifest-bound
    /// (and signed, when a key was supplied). Publish sends these so the
    /// registry metadata matches the pack exactly.
    pub describe_bytes: Vec<u8>,
}

/// Walk `describe.runtime.components.<key>.gtpack` and append every referenced
/// file (other than `extension.wasm` and the output pack itself) to `entries`,
/// sha256-verifying each. Returns silently if the components map is absent or
/// empty (Design/Bundle/Deploy extensions with self-contained wasm).
fn collect_runtime_component_files(
    describe: &serde_json::Value,
    project_dir: &Path,
    output_pack: &Path,
    entries: &mut Vec<PackEntry>,
) -> anyhow::Result<()> {
    let output_pack_name = output_pack
        .file_name()
        .and_then(|s| s.to_str())
        .unwrap_or("");
    let Some(components) = describe["runtime"]
        .get("components")
        .and_then(|v| v.as_object())
    else {
        return Ok(());
    };
    let mut seen_files: std::collections::HashSet<String> = std::collections::HashSet::new();
    for (comp_key, comp) in components {
        let Some(gtpack) = comp.get("gtpack").filter(|v| !v.is_null()) else {
            continue;
        };
        let file_rel = gtpack["file"].as_str().ok_or_else(|| {
            anyhow::anyhow!(
                "describe.runtime.components.{comp_key}.gtpack.file missing or not a string"
            )
        })?;
        if file_rel == "extension.wasm" || file_rel == output_pack_name {
            continue;
        }
        if !seen_files.insert(file_rel.to_string()) {
            continue;
        }
        let expected_sha = gtpack["sha256"].as_str().ok_or_else(|| {
            anyhow::anyhow!(
                "describe.runtime.components.{comp_key}.gtpack.sha256 missing or not a string"
            )
        })?;
        // `file_rel` comes from describe.json (author-controlled). Reject
        // absolute paths and any `..` component so a crafted describe cannot
        // make build_pack read a file outside the project dir (e.g. /etc/passwd)
        // and embed it in the published pack.
        let candidate = Path::new(file_rel);
        if candidate.is_absolute()
            || candidate
                .components()
                .any(|c| matches!(c, std::path::Component::ParentDir))
        {
            anyhow::bail!(
                "describe.runtime.components.{comp_key}.gtpack.file = {file_rel:?} must be a \
                 project-relative path with no `..` or absolute components"
            );
        }
        let abs = project_dir.join(file_rel);
        if !abs.exists() {
            anyhow::bail!(
                "describe.runtime.components.{comp_key}.gtpack.file = {file_rel:?} but file not found at {}.\n\
                 Multi-component extensions must stage their runtime .gtpack into the project before publish.\n\
                 For pilot/dev, ship a placeholder file at the declared path with sha256 matching describe.json.",
                abs.display()
            );
        }
        let bytes = std::fs::read(&abs)
            .map_err(|e| anyhow::anyhow!("read runtime gtpack at {}: {e}", abs.display()))?;
        let actual_sha = sha256_hex(&bytes);
        if actual_sha != expected_sha {
            anyhow::bail!(
                "describe.runtime.components.{comp_key}.gtpack.sha256 mismatch for {file_rel}:\n\
                 declared: {expected_sha}\n\
                 actual:   {actual_sha}\n\
                 Either rebuild the runtime + update describe.json, or update describe.json to match the staged file."
            );
        }
        entries.push(PackEntry::file(file_rel.to_string(), bytes));
    }
    Ok(())
}

/// Fill in the digests of the component this build itself produces.
///
/// A scaffold declares `gtpack.file = "extension.wasm"` with an all-zero
/// `sha256`, because the digest of a wasm that does not exist yet is unknowable
/// and changes on every rebuild. Everything else in `runtime.components` stays
/// the author's to supply: externally staged `.gtpack` files are *verified*
/// against disk by [`collect_runtime_component_files`], and an `oci_ref`
/// component describes an artifact this build never touched — filling either
/// would assert something the packer cannot know.
///
/// Returns `true` when a digest changed, so the caller can persist the describe
/// only when there is something to persist.
fn fill_self_contained_hashes(describe: &mut serde_json::Value, wasm_sha: &str) -> bool {
    let Some(components) = describe
        .pointer_mut("/runtime/components")
        .and_then(|v| v.as_object_mut())
    else {
        return false;
    };
    let mut changed = false;
    for component in components.values_mut() {
        if component.pointer("/gtpack/file").and_then(|v| v.as_str()) != Some("extension.wasm") {
            continue;
        }
        for pointer in ["/gtpack/sha256", "/sha256"] {
            if let Some(slot) = component.pointer_mut(pointer)
                && slot.as_str() != Some(wasm_sha)
            {
                *slot = serde_json::Value::String(wasm_sha.to_string());
                changed = true;
            }
        }
    }
    changed
}

/// Read the full `SecretRequirement`s declared in a nested runtime `.gtpack`
/// (a ZIP carrying `manifest.cbor`). Returns an error when the file is not a
/// readable `.gtpack` / the manifest is missing / decode fails, so the caller
/// can treat it as best-effort and skip the component.
fn read_gtpack_secret_requirements(
    gtpack_path: &Path,
) -> anyhow::Result<Vec<greentic_types::secrets::SecretRequirement>> {
    use std::io::Read as _;
    let bytes = std::fs::read(gtpack_path)
        .map_err(|e| anyhow::anyhow!("read {}: {e}", gtpack_path.display()))?;
    let mut archive = zip::ZipArchive::new(std::io::Cursor::new(bytes))
        .map_err(|e| anyhow::anyhow!("open {} as zip: {e}", gtpack_path.display()))?;
    let mut entry = archive
        .by_name("manifest.cbor")
        .map_err(|e| anyhow::anyhow!("no manifest.cbor in {}: {e}", gtpack_path.display()))?;
    let mut cbor = Vec::new();
    entry
        .read_to_end(&mut cbor)
        .map_err(|e| anyhow::anyhow!("read manifest.cbor in {}: {e}", gtpack_path.display()))?;
    let manifest = greentic_types::decode_pack_manifest(&cbor)
        .map_err(|e| anyhow::anyhow!("decode manifest.cbor in {}: {e}", gtpack_path.display()))?;
    Ok(manifest.secret_requirements)
}

/// Populate `describe.required_secrets` with the union of `SecretRequirement`s
/// from nested runtime `.gtpack` manifests merged with those the author already
/// declared in `describe.required_secrets`. Results are deduped by `key` (first
/// occurrence wins) and sorted by key string for stable output.
///
/// `describe.runtime.permissions.secrets` is intentionally left untouched —
/// that field is authored and does not carry the structured requirement objects.
///
/// Best-effort per component: a component whose `gtpack.file` is not a readable
/// `.gtpack` (e.g. a plain wasm runtime) or whose `manifest.cbor` is absent /
/// fails to decode is skipped with a `tracing::warn!` — it never fails publish.
fn enrich_describe_secrets(describe: &mut DescribeJson, project_dir: &Path) {
    // Seed with the author's existing requirements (preserves any extra metadata
    // they provided such as `description`, `format`, `scope`).
    let mut seen: std::collections::BTreeMap<String, greentic_types::secrets::SecretRequirement> =
        describe
            .required_secrets
            .drain(..)
            .map(|r| (r.key.as_str().to_string(), r))
            .collect();

    for (comp_key, component) in &describe.runtime.components {
        let Some(gtpack) = component.gtpack.as_ref() else {
            continue;
        };
        let abs = project_dir.join(&gtpack.file);
        match read_gtpack_secret_requirements(&abs) {
            Ok(reqs) => {
                for req in reqs {
                    // Author-listed entries take precedence; gtpack entries fill gaps.
                    seen.entry(req.key.as_str().to_string()).or_insert(req);
                }
            }
            Err(err) => {
                tracing::warn!(
                    component = %comp_key,
                    file = %gtpack.file,
                    error = %err,
                    "skipping secret enrichment for runtime component: \
                     not a readable .gtpack with a decodable manifest.cbor"
                );
            }
        }
    }

    // `BTreeMap` is already keyed in byte-sorted order, giving stable output.
    describe.required_secrets = seen.into_values().collect();
}

/// Build a `.gtxpack` at `output_pack` from `project_dir` + the already-built
/// `wasm_path`. The ZIP contains `describe.json`, the wasm renamed to
/// `extension.wasm`, and any optional asset dirs that exist (`i18n/`,
/// `schemas/`, `prompts/`, `assets/`). `assets/` is what `describe.metadata.icon`
/// resolves against once the extension is unpacked.
///
/// For multi-component extensions (v2 schema), each entry in
/// `describe.runtime.components.<key>.gtpack` whose `file` is not
/// `extension.wasm` is read, sha256-verified, and embedded in the archive at
/// the declared project-relative path (e.g. `runtime/provider.gtpack`).
pub fn build_pack(
    project_dir: &Path,
    wasm_path: &Path,
    output_pack: &Path,
) -> anyhow::Result<PackInfo> {
    build_pack_with_key(project_dir, wasm_path, output_pack, None)
}

/// Read the raw `manifest.json` bytes from a freshly-built `.gtxpack`.
fn read_manifest_from_zip(zip_bytes: &[u8]) -> anyhow::Result<Vec<u8>> {
    use std::io::Read as _;
    let mut archive = zip::ZipArchive::new(std::io::Cursor::new(zip_bytes))?;
    let mut entry = archive
        .by_name("manifest.json")
        .map_err(|e| anyhow::anyhow!("packer produced no manifest.json: {e}"))?;
    let mut buf = Vec::new();
    entry.read_to_end(&mut buf)?;
    Ok(buf)
}

/// Like [`build_pack`], but binds the whole-archive manifest into `describe.json`
/// (`manifestSha256`) and, when `signing_key` is `Some`, signs the bound
/// describe. The binding is keyless and always applied so the produced pack
/// passes manifest-binding verification; signing is what makes it install under
/// the Normal/Strict trust policies.
pub fn build_pack_with_key(
    project_dir: &Path,
    wasm_path: &Path,
    output_pack: &Path,
    signing_key: Option<&ed25519_dalek::SigningKey>,
) -> anyhow::Result<PackInfo> {
    let describe_path = project_dir.join("describe.json");
    let describe_bytes =
        std::fs::read(&describe_path).map_err(|e| anyhow::anyhow!("read describe.json: {e}"))?;
    let mut describe: serde_json::Value = serde_json::from_slice(&describe_bytes)
        .map_err(|e| anyhow::anyhow!("parse describe.json: {e}"))?;

    // The wasm this build just produced is the one artifact whose digest the
    // author could not have written by hand. Fill it in before anything binds
    // or signs the describe, and persist it to the project so
    // `gtdx lint --publish` stops reporting placeholders after a publish.
    let wasm_bytes = std::fs::read(wasm_path)
        .map_err(|e| anyhow::anyhow!("read {}: {e}", wasm_path.display()))?;
    let describe_bytes = if fill_self_contained_hashes(&mut describe, &sha256_hex(&wasm_bytes)) {
        let updated = serde_json::to_string_pretty(&describe)? + "\n";
        std::fs::write(&describe_path, &updated)
            .map_err(|e| anyhow::anyhow!("write {}: {e}", describe_path.display()))?;
        updated.into_bytes()
    } else {
        describe_bytes
    };

    let ext_name = describe["metadata"]["name"]
        .as_str()
        .ok_or_else(|| anyhow::anyhow!("describe.metadata.name missing"))?
        .to_string();
    let ext_version = describe["metadata"]["version"]
        .as_str()
        .ok_or_else(|| anyhow::anyhow!("describe.metadata.version missing"))?
        .to_string();
    let ext_kind = describe["kind"]
        .as_str()
        .ok_or_else(|| anyhow::anyhow!("describe.kind missing"))?
        .to_string();

    let mut entries = vec![
        PackEntry::file("describe.json", describe_bytes),
        PackEntry::file("extension.wasm", wasm_bytes),
    ];

    collect_runtime_component_files(&describe, project_dir, output_pack, &mut entries)?;

    for asset_dir in ["i18n", "schemas", "prompts", "assets"] {
        let src = project_dir.join(asset_dir);
        if !src.is_dir() {
            continue;
        }
        let mut paths: Vec<PathBuf> = WalkDir::new(&src)
            .into_iter()
            .flatten()
            .filter(|e| e.file_type().is_file())
            .map(|e| e.path().to_path_buf())
            .collect();
        paths.sort();
        for abs in paths {
            let rel = abs
                .strip_prefix(project_dir)
                .expect("asset under project")
                .to_string_lossy()
                .replace('\\', "/");
            entries.push(PackEntry::file(rel, std::fs::read(&abs)?));
        }
    }

    if let Some(parent) = output_pack.parent() {
        std::fs::create_dir_all(parent)?;
    }
    // Pass 1: pack to obtain the canonical `manifest.json` bytes the writer
    // produces (post-normalization, sorted) — the exact bytes a verifier will
    // hash. `manifest.json` excludes `describe.json`, so editing describe below
    // does not change it on the second pass.
    let zip1 = build_gtxpack_with_manifest(entries.clone())
        .map_err(|e| anyhow::anyhow!("build_gtxpack_with_manifest: {e}"))?;
    let manifest_bytes = read_manifest_from_zip(&zip1)?;

    // Bind the manifest into describe (and sign when a key is supplied), then
    // re-emit describe.json so the pack carries an authenticated descriptor.
    let mut describe_typed: DescribeJson = serde_json::from_value(describe.clone())
        .map_err(|e| anyhow::anyhow!("parse describe.json as typed: {e}"))?;
    // Populate `requiredSecrets` from nested runtime `.gtpack` manifests,
    // merged with any the author already listed, so the designer-admin
    // "Add credential" dialog renders all declared fields for providers
    // without requiring authors to hand-duplicate them. Best-effort:
    // unreadable/non-gtpack components are skipped with a warning
    // (see `enrich_describe_secrets`). Mutated on `describe_typed` before
    // `sign_describe` so the signature covers the enriched secrets.
    enrich_describe_secrets(&mut describe_typed, project_dir);
    bind_manifest(&mut describe_typed, &manifest_bytes);
    if let Some(key) = signing_key {
        sign_describe(&mut describe_typed, key).map_err(|e| anyhow::anyhow!("sign: {e}"))?;
    }
    let final_describe_bytes = serde_json::to_vec_pretty(&describe_typed)?;
    if let Some(entry) = entries.iter_mut().find(|e| e.path == "describe.json") {
        entry.bytes.clone_from(&final_describe_bytes);
    }

    // Pass 2: final pack with the bound/signed describe.json.
    let zip_bytes = build_gtxpack_with_manifest(entries)
        .map_err(|e| anyhow::anyhow!("build_gtxpack_with_manifest: {e}"))?;

    // Re-verify the produced pack with exactly the checks the registry runs at
    // install time, *before* writing it to disk or uploading. A signing-key,
    // JCS-canonicalization, or serialization defect is caught here rather than
    // surfacing only at a consumer's failed install (audit N4).
    verify_produced_pack(&zip_bytes, &describe_typed, signing_key)?;

    std::fs::write(output_pack, &zip_bytes)?;

    let size = u64::try_from(zip_bytes.len()).unwrap_or(u64::MAX);
    let pack_name = output_pack
        .file_name()
        .and_then(|s| s.to_str())
        .unwrap_or("pack.gtxpack")
        .to_string();
    let sha256 = sha256_hex(&zip_bytes);

    Ok(PackInfo {
        pack_path: output_pack.to_path_buf(),
        pack_name,
        size,
        sha256,
        ext_name,
        ext_version,
        ext_kind,
        describe_bytes: final_describe_bytes,
    })
}

/// Re-verify a freshly produced pack with exactly the checks the registry runs
/// at install time (archive↔manifest integrity, describe↔manifest binding, and
/// — when signed — anchored signature). Catches a producer-side defect before
/// the artifact is written or uploaded (audit N4).
fn verify_produced_pack(
    zip_bytes: &[u8],
    describe: &DescribeJson,
    signing_key: Option<&ed25519_dalek::SigningKey>,
) -> anyhow::Result<()> {
    let manifest_bytes = read_manifest_from_zip(zip_bytes)?;
    greentic_extension_sdk_contract::verify_archive_against_manifest(zip_bytes)
        .map_err(|e| anyhow::anyhow!("self-verify (archive vs manifest): {e}"))?;
    greentic_extension_sdk_contract::verify_manifest_binding(describe, &manifest_bytes)
        .map_err(|e| anyhow::anyhow!("self-verify (manifest binding): {e}"))?;
    if let Some(key) = signing_key {
        greentic_extension_sdk_contract::verify_describe_with_key(describe, &key.verifying_key())
            .map_err(|e| anyhow::anyhow!("self-verify (signature): {e}"))?;
    }
    Ok(())
}

#[cfg(test)]
mod tests;
