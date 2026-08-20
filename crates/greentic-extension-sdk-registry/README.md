# greentic-extension-sdk-registry

Registry client and install lifecycle for [Greentic Designer](https://greentic.ai) extensions.

Fetching, verifying and installing extensions.

- `GreenticStoreRegistry` (HTTP) and `OciRegistry` (OCI), plus a local
  filesystem registry for development
- `Installer` — the install lifecycle: fetch → verify → consent → stage → commit
- Verification is not optional: integrity is enforced for every policy, and
  `TrustPolicy::Loose` waives *authenticity* only
- Archive extraction rejects traversal, symlinks, duplicate entries and
  zip bombs before anything reaches disk

Part of the [greentic-designer-sdk](https://github.com/greenticai/greentic-designer-sdk)
workspace — see the repository README for the full workflow.

## License

MIT
