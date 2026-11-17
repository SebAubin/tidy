<div align="center">

<img src="docs/images/icon.png" width="128" height="128" alt="Tidy icon">

# Tidy

**See what fills your computer, and clean it up with the AI you already have.**

A native disk map for **macOS and Windows**, with a developer cleanup view and an assistant that runs
on Claude Code, Codex or Gemini CLI. Built in Rust. Nothing is deleted without you.

[![CI](https://github.com/SebAubin/tidy/actions/workflows/ci.yml/badge.svg)](https://github.com/SebAubin/tidy/actions/workflows/ci.yml)
[![Release](https://img.shields.io/github/v/release/SebAubin/tidy?include_prereleases&label=release&color=1fbf8f)](https://github.com/SebAubin/tidy/releases)
![macOS 13+](https://img.shields.io/badge/macOS-13%2B-1d1d1f?logo=apple)
![Windows 10/11](https://img.shields.io/badge/Windows-10%20%7C%2011-0078d4?logo=windows)
![Rust](https://img.shields.io/badge/Rust-2024-dea584?logo=rust)
[![License: MIT](https://img.shields.io/badge/license-MIT-blue)](LICENSE)

[Download](https://github.com/SebAubin/tidy/releases/latest) ·
[Features](#features) ·
[The assistant](#the-assistant) ·
[Privacy](#privacy-and-safety) ·
[Build from source](#build-from-source)

<br>

<picture>
  <source media="(prefers-color-scheme: dark)" srcset="docs/images/assistant-dark.png">
  <img src="docs/images/assistant-light.png" alt="Tidy's space map with the assistant proposing a cleanup" width="900">
</picture>

</div>

## Why Tidy

Disk tools show you a big chart and leave the hard part to you: *is this safe to delete?*
Tidy answers that. It knows which folders rebuild themselves, which worktrees are stale,
which simulator runtimes you never boot, and it lets the AI you already pay for look at the
picture with you, without ever handing it your files.

- **Native on Mac and Windows.** One GPU-rendered app (Rust on [Rune](https://github.com/anomalyco/rune))
  that follows each system: the Mac's menu bar, SF Pro and Finder; Windows' File Explorer,
  Recycle Bin, Ctrl shortcuts and toggles. Four million files scanned in under 30 seconds.
- **Made for developers too.** Git worktrees, Xcode and Android emulators, `node_modules`,
  `target/`, `bin/obj`, NuGet, npm and Cargo caches and Docker, each cleaned by the tool that owns it.
- **Your AI, not ours.** Uses the Claude Code, Codex or Gemini CLI already signed in on your
  computer. No API key to paste, no account to create.
- **Reversible by default.** Everything goes to the Trash (or the Recycle Bin on Windows), where
  you can put it back.

## Features

<table>
<tr>
<td width="50%" valign="top">

### Space Map
A squarified treemap of any folder, two levels deep, beside a ranked list. Double-click to
zoom in, <kbd>Esc</kbd> to go back up, right-click to reveal in Finder or File Explorer. The scan fills the map
in live, and the last map opens instantly on the next launch.

</td>
<td width="50%">

<picture>
  <source media="(prefers-color-scheme: dark)" srcset="docs/images/map-dark.png">
  <img src="docs/images/map-light.png" alt="Space map">
</picture>

</td>
</tr>
<tr>
<td width="50%">

<picture>
  <source media="(prefers-color-scheme: dark)" srcset="docs/images/developer-dark.png">
  <img src="docs/images/developer-light.png" alt="Developer view">
</picture>

</td>
<td width="50%" valign="top">

### Developer
Stale **git worktrees** (checked with `git status`, never removed with work in them unless you
say so), unused **iOS simulators and runtimes**, **Android** emulators and orphaned system
images, **build output** of idle projects, **package caches**, extra **toolchains** and
**Docker**. Each with a tip, its last use, and how it's cleaned:
`git worktree remove`, `xcrun simctl delete`, `docker builder prune`… or the Trash / Recycle Bin.

</td>
</tr>
<tr>
<td width="50%" valign="top">

### Duplicates
Files stored more than once, compared by size, then their first and last 16 KB, then a full
BLAKE3 hash, so most files are never read. App libraries, build output and git checkouts are
left out. The original is kept by default, and Tidy never lets you remove the last copy.

</td>
<td width="50%">

<picture>
  <source media="(prefers-color-scheme: dark)" srcset="docs/images/duplicates-dark.png">
  <img src="docs/images/duplicates-light.png" alt="Duplicates">
</picture>

</td>
</tr>
<tr>
<td width="50%">

<picture>
  <source media="(prefers-color-scheme: dark)" srcset="docs/images/changes-dark.png">
  <img src="docs/images/changes-light.png" alt="Changes">
</picture>

</td>
<td width="50%" valign="top">

### Changes
Every scan is remembered as a small summary, so Tidy can tell you *where the space went*:
which folders grew or shrank since the last scan, a week or a month ago. It points at the
subfolder responsible, not just its parent.

</td>
</tr>
</table>

**And also:** offline **Suggestions** (caches, old installers, build folders), **Large Files**
with multi-select, a native **Settings** window, light and dark themes and six accent colours.
On macOS, also a **menu bar** item with free space (or % free / used), a low-space notification
and the full menu bar with Finder's shortcuts.

<div align="center">
<picture>
  <source media="(prefers-color-scheme: dark)" srcset="docs/images/scanning-dark.png">
  <img src="docs/images/scanning-light.png" alt="The map filling in during a scan" width="720">
</picture>
<br><sub>The map fills in as the scan runs.</sub>
</div>

## The assistant

Tidy doesn't ship an AI. It drives the coding assistant you already use, headless, and gives it
exactly one way to see your disk: Tidy's own **read-only MCP server**.

```mermaid
flowchart LR
    You([You]) -->|ask| Panel[Tidy assistant panel]
    Panel -->|prompt| CLI["Claude Code · Codex · Gemini CLI<br/>(your login)"]
    CLI <-->|MCP tools| MCP["tidy mcp<br/>read-only, the scan only"]
    MCP --- Snapshot[(Scan snapshot<br/>names · sizes · dates)]
    CLI -->|propose_cleanup| Panel
    Panel -->|you confirm| Trash[(Trash / Recycle Bin)]
```

| Tool | What it gives the assistant |
| --- | --- |
| `overview`, `list_folder`, `find_files` | Sizes, kinds and dates from the scan |
| `developer_findings` | What the Developer view found |
| `show_in_tidy`, `open_view` | Steer the window: zoom the map, open a view |
| `propose_cleanup` | A checklist in the panel. It deletes nothing; you tick and confirm |

Pick the assistant and its model in **Settings › Assistant** (`opus`, `sonnet`… or the CLI's
default), turn any of them off, and Tidy remembers the last one you used.

## Privacy and safety

- **Your files stay on your computer.** The assistant sees paths, sizes and dates through Tidy's
  tools, never file contents.
- **Only Tidy's tools.** For each conversation, your other MCP servers, plugins (computer use,
  browser…) and shell tools are turned off, without touching your own configuration.
- **Nothing is deleted without you.** The assistant can only propose. Every removal is a
  checklist you confirm, and goes to the Trash or Recycle Bin unless a developer tool owns the cleanup.
- **Anonymous telemetry, said out loud.** What's sent is listed [below](#telemetry), and one
  switch turns it off.
- **One permission, not fifteen.** On macOS, Tidy asks once for Full Disk Access instead of letting
  the system prompt for each protected folder, and skips those folders if you'd rather not. Windows
  needs no permission at all.

<div align="center">
<picture>
  <source media="(prefers-color-scheme: dark)" srcset="docs/images/settings-dark.png">
  <img src="docs/images/settings-light.png" alt="Settings" width="420">
</picture>
</div>

## Telemetry

Released builds send **anonymous** usage statistics and crash reports to
[Aptabase](https://aptabase.com), an open-source, privacy-first analytics service, so we can see
what's used and what breaks. It's on by default, said so on first launch, and off with one switch
in **Settings › Privacy** (or `DO_NOT_TRACK=1`). Builds from source send nothing.

| Event | Properties |
| --- | --- |
| `app_started` | CPU architecture |
| `scan_finished` | where (`home`, `disk`, `inside home`, `other folder`), size, file count and duration in broad ranges, Full Disk Access on or off |
| `view_opened` | which view |
| `cleanup`, `developer_cleanup` | item count and size freed in broad ranges, the view it came from |
| `duplicates_found` | group count and extra space in broad ranges |
| `assistant_message`, `assistant_proposal` | which assistant (Claude Code, Codex, Gemini); item count in ranges |
| crash reports | the error and stack trace, with every path and user name removed |

Every event also carries the app and system (macOS or Windows) versions, the locale and a random session id.
**Never** sent: paths, file or folder names, exact sizes, your user name, or anything you ask the
assistant. The code is in [`app/src/telemetry.rs`](app/src/telemetry.rs).

## Code signing policy

Windows releases are signed through SignPath. Free code signing provided by
[SignPath.io](https://about.signpath.io), certificate by [SignPath Foundation](https://signpath.org).

- **Committers and reviewers:** [Sebastien Aubin](https://github.com/SebAubin). Changes from anyone
  else come as pull requests and are reviewed before they're merged.
- **Approvers:** [Sebastien Aubin](https://github.com/SebAubin) approves each signing request.
- **Builds:** every signed file is built by the
  [Release workflow](.github/workflows/release.yml) from a tagged commit of this repository.
- **Privacy:** Tidy sends anonymous usage statistics as described under [Telemetry](#telemetry), on
  by default and off with one switch (also offered by the Windows installer, which shows this notice
  first). The assistant sends what you ask it, and what it looks up in the scan, to the AI
  command-line tool you chose (Claude Code, Codex or Gemini CLI), under that tool's privacy policy.
  Tidy transfers nothing else to other networked systems unless you ask it to.

## Install

### macOS

1. Download **`Tidy-*-macos-universal.dmg`** from
   [Releases](https://github.com/SebAubin/tidy/releases/latest), open it and drag **Tidy** into
   **Applications**. One download for Apple silicon and Intel.
2. Open Tidy. If the build isn't notarized, macOS blocks it the first time: go to
   **System Settings › Privacy & Security** and click **Open Anyway**.
3. Optional: turn on **Full Disk Access** for Tidy when it asks, so it can measure everything.
4. Optional: sign in once to [Claude Code](https://claude.com/claude-code), Codex or Gemini CLI
   to use the assistant.

### Windows

Download **`Tidy-*-windows-x64-setup.exe`** (or `arm64` for Windows on ARM) from
[Releases](https://github.com/SebAubin/tidy/releases/latest) and run it. It installs for you
only, with no administrator prompt, adds Tidy to the Start menu and to Settings › Apps for
uninstalling. A portable `.zip` with `Tidy.exe` is attached too. If SmartScreen warns about an
unrecognized app, choose **More info › Run anyway**.

### Both

Requires macOS 13 or later, or Windows 10/11. A `.zip` of the app and SHA-256 sums are attached to each release
too, and every push to `main` leaves a `.dmg` in the
[CI run's artifacts](https://github.com/SebAubin/tidy/actions/workflows/ci.yml).

## Build from source

```sh
git clone https://github.com/SebAubin/tidy && cd tidy
cargo run -p tidy                        # the app, from a dev build
scripts/bundle-mac.sh --debug --open     # "Tidy Dev.app": keeps its macOS permissions across rebuilds
scripts/bundle-mac.sh --open             # target/Tidy.app, release
scripts/package-mac.sh                   # dist/: universal .dmg (drag to Applications), .zip, SHA-256
pwsh scripts/package-windows.ps1         # Windows, dist\: installer (Inno Setup), portable .zip, SHA-256
cargo test --workspace
```

On Windows, install the Visual Studio C++ Build Tools (with the ARM64 tools on Windows on ARM) and
[Inno Setup](https://jrsoftware.org/isinfo.php) for the installer. The toolchain is pinned in `rust-toolchain.toml`. Bundles are signed with your Apple Development
or Developer ID certificate when one is installed, so privacy permissions survive rebuilds.

Every screen renders headless for tests and screenshots, with no window:

```sh
target/debug/tidy --snap map.png --scene map --size 1240x780 --scale 2 --theme dark
# scenes: welcome map suggestions large scanning changes duplicates developer chat
target/debug/tidy --window settings --snap settings.png
```

### Project layout

| Path | What's there |
| --- | --- |
| `crates/tidy-core` | The engine, no UI: parallel scanner, insights, developer findings, duplicates, history, snapshot, MCP server |
| `app` | The Rune app: views, assistant sessions, settings, the macOS menu bar item and the per-system bits (`os.rs`) |
| `app/assets/icon` | `make_icon.py` draws the icon and builds `Tidy.icns`; `make_ico.py` builds `Tidy.ico` for Windows |
| `installer/windows` | The Inno Setup script for the Windows installer |
| `.github/workflows` | CI on every push; a release on every `v*` tag (signed and notarized once the secrets are set) |

## Releases

Push a tag (`git tag v0.2.0 && git push --tags`) and the Release workflow builds a universal
`Tidy.app`, packed as a `.dmg` with a drag-to-Applications window plus a `.zip`, and Windows
installers and portable `.zip`s for x64 and arm64, then publishes a GitHub release with install
notes. Add the secrets listed at the top of
`.github/workflows/release.yml` to have it signed with Developer ID and notarized.

## Roadmap

- Windows: a system tray item with free space, and Developer findings for Visual Studio and WSL
- Scheduled cleanup rules ("every Monday, clear DerivedData and merged worktrees")
- External and network volumes, folder comparison
- Sunburst view and search in the scan

## License

[MIT](LICENSE) © 2026 Sebastien Aubin
