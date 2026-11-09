# Notices and attribution

Argon is licensed under the Mozilla Public License 2.0 (see `LICENSE`).

## Servo and servoshell

The engine is [Servo](https://github.com/servo/servo) (tag `v0.7.0`), and the
Windows shell in `windows/` is adapted from Servo's `servoshell` reference
port. Both are Copyright The Servo Project Developers and licensed under the
Mozilla Public License 2.0. Source: https://github.com/servo/servo

## Zen browser

The chrome layout (vertical tabs, compact sidebar, centered address bar) and
visual tokens are modeled on the [Zen browser](https://github.com/zen-browser/desktop),
Copyright Zen Browser, licensed under the Mozilla Public License 2.0. No Zen
source code is vendored; the layout was reimplemented for the egui shell.

## Chromium

Chromium is a feature reference only; no Chromium code is used.

## Original Argon workspace design

The rebuilt workspace UI uses the design process from **unslop by Damir
Sokolovsky** (https://github.com/heydamir/unslop), licensed CC BY 4.0
(https://creativecommons.org/licenses/by/4.0/). Adapted for a native Rust/egui
browser. Reference artwork is not included. Design changes and validation
notes are in `docs/design/argon-workspace.md`.

## Windows acrylic composition

window-vibrancy 0.8.1 by Tauri Programme within The Commons Conservancy,
https://github.com/tauri-apps/window-vibrancy . Licensed under Apache-2.0 OR MIT.
Native Windows acrylic is requested through this library; support depends on
Windows version and composition settings.
