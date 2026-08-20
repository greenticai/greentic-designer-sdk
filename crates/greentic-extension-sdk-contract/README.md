# greentic-extension-sdk-contract

Contract types and `describe.json` schema for [Greentic Designer](https://greentic.ai) extensions.

Type definitions, the `describe.json` v2 JSON Schema, and the signing and
verification primitives shared by every part of the SDK.

- `DescribeJson` and the contribution types an extension declares
- `validate_describe_json` — schema validation with per-field JSON pointers
- `build_manifest` / `bind_manifest` — the `manifest.json` integrity ledger
- `sign_describe` / `verify_describe_*` — RFC 8785 canonical JSON + ed25519
- `migrate_v0_4_x_value` — v1 → v2 document migration

Every struct is `deny_unknown_fields`, so a field cannot survive parsing while
escaping the signed payload.

Part of the [greentic-designer-sdk](https://github.com/greenticai/greentic-designer-sdk)
workspace — see the repository README for the full workflow.

## License

MIT
