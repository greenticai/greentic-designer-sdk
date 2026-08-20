# greentic-extension-sdk-state

Persistent enable/disable state for [Greentic Designer](https://greentic.ai) extensions.

Tracks which installed extensions are enabled, per scope, plus update policy
and last-failure bookkeeping.

Writes go through an atomic path — temp file, fsync, rename, fsync of the
parent directory — under an advisory lock held across the whole
read-modify-write, so a crash mid-write cannot leave a truncated state file.

Part of the [greentic-designer-sdk](https://github.com/greenticai/greentic-designer-sdk)
workspace — see the repository README for the full workflow.

## License

MIT
