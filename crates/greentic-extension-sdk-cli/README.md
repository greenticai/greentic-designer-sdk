# greentic-extension-sdk-cli

`gtdx` — the Greentic Designer Extensions CLI for [Greentic Designer](https://greentic.ai) extensions.

Scaffold, build, validate, sign, publish and install Greentic Designer
extensions.

```bash
gtdx doctor       # check the toolchain
gtdx new          # scaffold (interactive wizard)
gtdx dev --once   # build, pack, install
gtdx publish      # pack to dist/ and publish
```

Requires Rust 1.95+, `cargo-component`, and the `wasm32-wasip2` target;
`gtdx doctor` verifies all three.

Part of the [greentic-designer-sdk](https://github.com/greenticai/greentic-designer-sdk)
workspace — see the repository README for the full workflow.

## License

MIT
