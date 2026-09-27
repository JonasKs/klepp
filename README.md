# klepp

A small, glassy clipboard manager for Apple Silicon Macs on macOS 26+, written in Rust (Tauri 3 alpha + AppKit).

- Text and image clips are stored in one SQLite database, `~/.klepp/klepp.db`
  (images as PNG blobs, up to `max_image_mb`).
- **Ctrl+Shift+V** (configurable) toggles a Liquid Glass panel across the bottom of the screen.
- Type to search (all words must match), **↑/↓** to move, **⏎** to paste,
  **⌘⌫** or the ✕ button to delete, **esc** to close.
- Nothing copied from 1Password is recorded: klepp skips clips flagged
  `org.nspasteboard.ConcealedType` (which 1Password sets) and anything copied
  while a 1Password app is frontmost.
- Identical clips are de-duplicated (re-copying moves the clip to the top).
- When run from /Applications, Klepp registers itself as a login item, so it
  starts with your Mac after `brew install`. Toggle it under **Launch at login**
  in the menu bar or with `klepp --login off`; it shows up in
  System Settings → General → Login Items. Dev builds never register.
- A clipboard icon in the menu bar offers Open, Pause recording, a
  **Delete clips older than** schedule (never / 1 / 7 / 30 / 90 days / 1 year,
  enforced at launch and every 30 minutes), config editing, and Quit.
- Images show as thumbnails; searching `image`, `png`, `1200x800` or the source
  app name finds them.

## Configuration

`~/.klepp/config.toml` is created on first launch:

```toml
shortcut = "ctrl+shift+v"     # e.g. "cmd+shift+space", "alt+v"
ignore_apps = ["1password"]   # bundle-id substrings that are never recorded
max_image_mb = 10
```

Edit it from the menu bar (**Edit config.toml…**) and pick **Reload config**;
no restart needed. A bad shortcut is reported in an alert and the previous
binding is dropped until the file is fixed.

## Install

```sh
brew tap jonasks/tap
brew trust jonasks/tap      # Homebrew requires this once for third-party taps
brew install --cask --no-quarantine klepp
```

Klepp is ad-hoc signed, not notarized, so Gatekeeper blocks it unless it is
installed with `--no-quarantine`. If you already installed it without that flag:
`xattr -dr com.apple.quarantine /Applications/Klepp.app`. Apple Silicon and macOS 26+ only.

## Build from source

```sh
just install     # builds Klepp.app, copies it to /Applications, adds a login item, launches it
just uninstall   # removes the app and login item (clips in ~/.klepp are kept)
just run         # run from source with the panel open
just test        # unit tests for the store
just             # list all recipes
```

Requires Rust, [just](https://just.systems) and the Tauri 3 CLI
(`cargo install tauri-cli --version "^3.0.0-alpha"`, only for `just install` / `just bundle`).
`klepp --show` opens the panel immediately on launch.

## Signing

With the `APPLE_CERTIFICATE`, `APPLE_CERTIFICATE_PASSWORD`, `APPLE_SIGNING_IDENTITY`,
`APPLE_TEAM_ID`, `APPLE_API_ISSUER`, `APPLE_API_KEY` and `APPLE_API_KEY_CONTENT` secrets set,
the release build is signed with a Developer ID certificate and notarized, and the cask
drops its quarantine caveat. Without them the release is ad-hoc signed and the workflow
prints a warning.

## Releasing

Commits on `main` follow [Conventional Commits](https://www.conventionalcommits.org).
[release-please](https://github.com/googleapis/release-please) keeps a release PR open;
merging it tags `vX.Y.Z`, builds `Klepp-X.Y.Z-aarch64.zip`, attaches it to the GitHub
release, and pushes the updated cask to [jonasks/homebrew-tap](https://github.com/jonasks/homebrew-tap)
(over SSH with a deploy key stored as the `HOMEBREW_TAP_DEPLOY_KEY` secret). The workflow can also be
dispatched by hand with an existing tag to re-publish its cask. CI runs fmt, clippy, tests and `cargo deny` on every PR.

## Permissions

Pressing ⏎ copies the clip and sends ⌘V to the previous app. That keystroke
only works if Klepp has **Accessibility** access (System Settings → Privacy &
Security → Accessibility). Without it the clip is still on your clipboard;
just paste with ⌘V yourself.

## Layout

```
src/main.rs        Tauri app, hotkey, window placement, commands
src/clipboard.rs   NSPasteboard poller (text + images), 1Password / concealed filtering
src/config.rs      ~/.klepp/config.toml
src/store.rs       SQLite storage, search, retention, settings
src/glass.rs       NSGlassEffectView behind the transparent webview
src/paste.rs       ⌘V via CGEvent
src/tray.rs        Menu bar status item
ui/index.html      The panel (no build step, plain HTML/JS)
```
