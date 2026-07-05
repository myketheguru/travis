# Whisper model resources

Runtime models bundled into the installer for instant voice.

`ggml-base.en.bin` (~74 MB) is fetched at build time by
`scripts/fetch-whisper.mjs` and copied into this directory. It's
git-ignored by default (see repo `.gitignore`) — the fetch script
downloads it on demand.

The `.gitkeep` sibling exists so Tauri's `bundle.resources` glob
(`resources/whisper/**/*`) always matches at least one file, even
before the fetch step runs. Without it, `cargo check` fails in CI on
the "resources path not found" preflight.

This directory is committed to git empty-ish; contents at build time:

- `.gitkeep`  (committed — makes the glob happy)
- `README.md` (this file, committed)
- `ggml-base.en.bin` (fetched at build, not committed)
