# Tidy

[![CI](https://github.com/SebAubin/tidy/actions/workflows/ci.yml/badge.svg)](https://github.com/SebAubin/tidy/actions/workflows/ci.yml)

See what fills your Mac, and clean it up with the AI you already have.

Native macOS app in Rust on [Rune](https://github.com/anomalyco/rune) (Windows next).

- **Space Map**: a squarified treemap of any folder, two levels deep, with a ranked list. Double-click to zoom in, ⎋ to go up, right-click for Finder actions.
- **Suggestions**: offline detection of space that rebuilds itself (caches, Xcode DerivedData, `node_modules`, Rust `target`, venvs, old installers…).
- **Large Files**: the biggest files, with when they were last opened; ⌘-click, ⇧-click, ⌘A to pick several.
- **Developer**: stale git worktrees (never ones with uncommitted changes), build output of idle projects, simulator runtimes and devices, Android emulators and unused system images, package caches, extra toolchains, Docker. Each item is cleaned by the tool that owns it (`git worktree remove`, `xcrun simctl delete`, `docker builder prune`…) or moved to the Trash.
- **Duplicates**: files stored more than once (size, then first/last 16 KB, then full BLAKE3), outside app libraries, build output and git checkouts; one copy is always kept.
- **Changes**: what grew or shrank since the previous scan, a week or a month ago. Tidy also opens on the last map at once and refreshes it in the background.
- **Menu bar**: free space (or % free / % used / icon only) and a menu; a notification when space runs low, with what's reclaimable.
- **Settings** (⌘,): theme, accent, Large Files threshold, staleness, what the assistant is told.
- **Assistant**: drives the AI CLI already installed (Claude Code, Codex, Gemini CLI) through Tidy's own MCP server (`tidy mcp <snapshot>`), with the person's other MCP servers, plugins and shell tools turned off for the session. It sees names, sizes and dates, never file contents; it can zoom the map and open views, *proposes* cleanups as checklists, and only the person moves things to the Trash.

Nothing is ever deleted outright: items go to the Trash in one Finder operation (Put Back works).

Privacy: macOS guards Desktop, Documents, Downloads, other apps' data, iCloud Drive, Mail… each with its own dialog. Tidy asks once for **Full Disk Access** instead, and without it skips those folders rather than raising a dialog per folder. `scripts/bundle-mac.sh` signs with your Apple Development / Developer ID identity so the grant survives rebuilds (ad-hoc signatures change on every build).

Scans stay in memory: a location inside the scan on screen opens instantly, and up to two earlier scans are kept; ⌘R rescans.

## Layout

- `crates/tidy-core`: scanner (parallel, allocated size, hard links counted once, one volume), insights, snapshot, MCP server. No UI.
- `app`: the Rune app (`tidy`), plus `tidy mcp` for the assistant.
- `app/assets/icon/make_icon.py`: draws the icon and `Tidy.icns`.

## Run

```sh
cargo run -p tidy                     # dev window
scripts/bundle-mac.sh --open          # target/Tidy.app (release)
scripts/bundle-mac.sh --debug --open  # target/Tidy Dev.app: day-to-day, keeps its permissions across rebuilds
target/debug/tidy --snap map.png --scene map --size 1180x760 --scale 2   # headless (scenes: welcome map suggestions large scanning changes duplicates developer chat; --window settings)
cargo run --release -p tidy-core --example tidy-cli -- scan ~ /tmp/snap.json   # engine only
```

## Releases

Push a tag (`git tag v0.2.0 && git push --tags`) and the Release workflow builds `Tidy.app`,
zips it and publishes a GitHub release. Add the secrets listed at the top of
`.github/workflows/release.yml` to have it signed with Developer ID and notarized.

## License

MIT, see [LICENSE](LICENSE).
