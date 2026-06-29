# CI / release hardening — remaining follow-ups

Tracks the parts of audit findings **H6** and **M17** that were intentionally
deferred. The pinning + research-publish guard (H6 partial, M16) already landed
in `.github/workflows/`.

## Done

- **H6 (partial)** — the three reusable workflows in `release.yml` and
  `tag-on-version-bump.yml` are pinned to a `greenticai/.github` commit SHA
  (`7fd49c4`) instead of a floating `@main`, so a force-push/compromise of that
  repo cannot run with this repo's release secrets. Bump the SHA deliberately
  when adopting upstream changes.
- **M16** — the `publish-crates` job is guarded with
  `if: ${{ !contains(github.ref_name, 'research') }}` so pre-release
  `-research` tags never publish to crates.io.

## Deferred — needs the `greenticai/.github` repo (not in this checkout)

- **H6 (secret scoping)** — `release.yml` and `tag-on-version-bump.yml` still use
  `secrets: inherit`, which forwards the full secret set (crates.io token +
  write-scoped `GITHUB_TOKEN`) to the reusable workflows. Replace with explicit
  per-workflow `secrets:` blocks passing only what each consumes
  (e.g. `CARGO_REGISTRY_TOKEN` to `crates-publish.yml`; likely nothing extra for
  `release-binaries.yml` / `tag-on-version-bump.yml`). This requires reading the
  reusable workflows' declared `secrets:` inputs in `greenticai/.github`.

- **M17 (artifact integrity)** — `release-binaries.yml@main` (in
  `greenticai/.github`) owns binary publishing, so this repo can't guarantee
  artifacts are checksummed/signed. Confirm that workflow emits per-artifact
  `SHA256SUMS` and a signature/attestation (e.g. `actions/attest-build-provenance`
  or cosign), and document verification in `README.md` next to the
  `cargo binstall` instructions. If it does not, add it there.
