# Chromium feature checklist for Argon

The user asked to "add all other functionalities from chromium/chromium".
Chromium is a **feature reference**, not a compatibility promise: Argon runs on
the Servo engine (v0.7.0), so each Chromium feature lands in one of four tiers.
Grounded against the Servo embedder API at our pin
(`components/servo/webview.rs` and friends, tag v0.7.0).

## Tier A - shell features (implementable in Argon now, no engine work)

- [x] Tabs and windows (vertical sidebar, compact mode) - v0.1 chrome
- [x] Address bar with search-or-URL entry - v0.1 chrome
- [x] Back / forward / reload / stop - `WebView::go_back`, `go_forward`, `reload`
- [x] Per-page zoom - `WebView::set_page_zoom`, `adjust_pinch_zoom`
- [x] Dark and light chrome themes - Zen design tokens in `desktop/gui.rs`
- [ ] Omnibox search suggestions - shell-side, needs a suggestion source choice
- [ ] Bookmarks (sidebar section, folders) - local storage, no engine dependency
- [ ] Browsing history UI - shell-side navigation log (engine exposes no
      enumeration API; `WebView::clear_session_history` exists)
- [ ] Pinned tabs / essential tabs (Zen-style) - `RunningAppState` + sidebar
- [ ] Workspaces (Zen spaces) - tab grouping in `RunningAppState`
- [x] Keyboard shortcut map (Ctrl+T/W/L/R, Alt+D, F5 reload, Esc revert urlbar, Ctrl+Tab/Shift+Tab, Ctrl+1-8) - shell input routing
- [ ] Find-in-page UI - **blocked on engine API**: v0.7.0 exposes no
      find-in-page on `WebView`; track upstream or patch `components/servo`
- [ ] Reader mode - feasible via `user_content_manager` (inject reader script +
      stylesheet per page); quality will differ from Chromium's distilled mode
- [ ] User scripts / styles (cosmetic filtering, per-site darkening) -
      `user_content_manager.add_script` / `add_stylesheet` exist
- [ ] Screenshots of pages - `WebView::take_screenshot`
- [ ] Page JavaScript evaluation hooks - `WebView::evaluate_javascript`

## Tier B - needs Servo engine work (feasible, larger effort, maybe upstream)

- [ ] Media playback hardening (servo media is partial; gstreamer/wmf paths)
- [ ] Picture-in-picture - needs engine media session + compositor support
- [ ] Printing / print-to-PDF - engine has no print pipeline
- [ ] PDF viewer - engine has none; option: bundle pdf.js as web content
- [ ] DRM / Widevine (EME) - not in Servo; large
- [ ] WebExtensions / Chrome extensions - no extension platform in Servo; large
- [ ] DevTools - Servo has a partial devtools server; expose + wire UI
- [ ] Site isolation / multi-process polish - Servo `multiprocess` is experimental

## Tier C - needs external services or accounts (restricted, ask first)

These send data to third parties or need a sync backend. Project restrictions
require explicit user approval before any of these start.

- [ ] Search suggestion provider (queries leave the machine)
- [ ] Safe Browsing-style phishing/malware checks
- [ ] Sync (bookmarks/history/passwords across devices) - needs an account backend
- [ ] Translate - needs a translation service
- [ ] Password manager sync - local vault integration possible without a service

## Tier D - not feasible on Servo today

- Full Chromium extension ecosystem compatibility
- Full Chromium web-platform compatibility (Servo's layout/script support is a
  subset; sites depending on Chromium-only behavior will break)
- Chrome Web Store integration, Google account integration, Cast

## Working rule

Every new feature commit names its tier. Nothing in Tier C starts without an
explicit user approval recorded in the task thread.
