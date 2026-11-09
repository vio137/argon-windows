# Argon for Windows

Argon is a Windows desktop web browser built on the **Servo** engine. The
Windows shell in `windows/` is adapted from Servo's `servoshell` reference
port (MPL-2.0, see `NOTICES.md`) with a Zen-browser-inspired interface:
vertical tab sidebar with a compact favicon mode, a centered address bar,
pinned tabs, and Chromium-familiar keyboard shortcuts.

Chromium is used as a feature reference only - Argon does not promise
Chromium compatibility. See `docs/chromium-feature-checklist.md` for the
per-feature tier list.

## Status

Early development. The CI pipeline compiles the browser, packages it with
its engine resources, and smoke-tests the packaged zip (headless launch +
page render in a clean directory) before publishing a **nightly
pre-release**. The nightly is verified to start and render pages headless;
the full headed interface has not yet been verified on a live Windows
desktop session.

## Download and run

1. Open the [nightly pre-release](https://github.com/vio137/argon-windows/releases/tag/nightly).
2. Download `argon-windows-x64.zip`.
3. Extract **all** files (the `resources` folder and DLLs must sit next to
   the executable).
4. Run `argon.exe`.

Requires Windows 10/11 x64.

## Shortcuts

| Shortcut | Action |
| --- | --- |
| Ctrl+T / Ctrl+W | New / close tab |
| Ctrl+L or Alt+D | Focus address bar |
| Escape (while editing) | Revert address bar |
| Ctrl+R or F5 | Reload |
| Ctrl+Tab / Ctrl+Shift+Tab | Cycle tabs |
| Ctrl+1..8 | Jump to tab |
| Ctrl + / - / 0 | Page zoom in / out / reset |
| Ctrl+Shift+S | Save page screenshot (Pictures\Argon) |

Right-click a tab in the sidebar to pin it.

## Building

CI builds on `windows-2022` (`.github/workflows/windows-build.yml`). Local
build: `cd windows && cargo build --profile release` with the same toolchain
setup as the workflow (LLVM/Clang, Python 3.12, long paths enabled).

## License and attribution

Mozilla Public License 2.0 (`LICENSE`). Argon's shell is adapted from Servo's
`servoshell` and takes design cues from the Zen browser - see `NOTICES.md`.
