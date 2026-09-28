# klepp

Glassy clipboard manager for Apple Silicon Macs (macOS 26+). Rust, Tauri 3 alpha, AppKit via objc2.

`README.md` is external and user-facing. Maintainer notes, debugging findings and
release mechanics belong in this file, not in the README.

## Build & Run

```bash
just run          # debug build with the panel open (klepp --show)
just check        # fmt + clippy -D warnings + tests + cargo deny (what CI runs)
just bundle       # release Klepp.app (needs tauri-cli 3 alpha)
just package      # bundle + ad-hoc sign + zip, like an unsigned CI build
just clips        # newest rows in ~/.klepp/klepp.db
```

## Architecture

- `src/main.rs` — Tauri app, global shortcut, panel placement, commands, `klepp://` image protocol, retention thread, CLI flags (`--show`, `--login on|off`, `--login-status`)
- `src/clipboard.rs` — NSPasteboard poller (text + images), concealed/ignored-app filtering, writing clips back
- `src/store.rs` — SQLite (`~/.klepp/klepp.db`): clips, search, retention, `settings` table; unit tests live here
- `src/config.rs` — `~/.klepp/config.toml` (shortcut, ignore_apps, max_image_mb)
- `src/paste.rs` — re-activates the previous app, waits until frontmost, sends Cmd+V via CGEvent; Accessibility check and prompt
- `src/login.rs` — launch at login through SMAppService
- `src/glass.rs` — NSGlassEffectView under the transparent webview, content layer clipped to the corner radius
- `src/tray.rs` — menu bar item and native alerts
- `src/log.rs` — append-only `~/.klepp/klepp.log`
- `ui/index.html` — the panel, plain HTML/JS, no build step

## Key Conventions

- Apple Silicon and macOS 26 only. No cross-platform code, no fallbacks for older macOS.
- Task runner is `just`, never make.
- Conventional Commits. `feat:`/`fix:` cut releases; `docs:`, `ci:`, `chore:` do not.
- Tauri 3 has no default runtime: the builder must call `.runtime(tauri_runtime_wry::Wry::default())`.
- GUI-set preferences (retention, login item choice) go in the `settings` table; hand-edited ones in `config.toml`.
- Clip content is only ever put in the DOM with `textContent`, never `innerHTML`.
- `deny.toml` is scoped to `aarch64-apple-darwin`, which keeps Linux-only GTK crates out of the checks.

## Releasing

release-please keeps a release PR open on `main`. Merging it tags `vX.Y.Z`, builds
`Klepp-X.Y.Z-aarch64.zip` on a macOS arm64 runner, attaches it to the GitHub release,
and opens a PR on `jonasks/homebrew-tap` that rewrites `Casks/klepp.rb`. Merge that PR
to publish to brew.

The Release workflow can be dispatched by hand with an existing `tag` to re-open the
tap PR for a release that already exists.

Version lives in `Cargo.toml`, `Cargo.lock`, `tauri.conf.json` and the manifest;
release-please bumps all four.

### Secrets (repo `jonasks/klepp`)

| Secret | Purpose |
|--------|---------|
| `HOMEBREW_TAP_TOKEN` | Fine-grained PAT on `JonasKs/homebrew-tap`: Contents + Pull requests, read and write |
| `APPLE_CERTIFICATE` | base64 of the Developer ID Application `.p12` |
| `APPLE_CERTIFICATE_PASSWORD` | password of that `.p12` |
| `APPLE_SIGNING_IDENTITY` | `Developer ID Application: … (TEAMID)` |
| `APPLE_TEAM_ID` | team id |
| `APPLE_API_KEY`, `APPLE_API_ISSUER`, `APPLE_API_KEY_CONTENT` | App Store Connect API key for notarization (`.p8` contents) |

With every Apple secret present the build is Developer ID signed, notarized and
stapled, and the job asserts it with `codesign --verify` and `spctl --assess`.
If any is missing it falls back to an ad-hoc signature, warns, and the cask gains a
quarantine caveat.

## Gotchas

- **Stale Accessibility record.** A grant given to an ad-hoc signed build is bound to
  that binary's cdhash. Later signed builds are then untrusted even though System
  Settings shows Klepp as enabled, and toggling the switch does not rebind it.
  Symptom: `accessibility=false` on the `start:` line of `~/.klepp/klepp.log`.
  Fix: `tccutil reset Accessibility com.jonas.klepp` and
  `tccutil reset PostEvent com.jonas.klepp`, then grant again. Records created for a
  Developer ID build are bound to the team id and survive upgrades.
- **Local builds borrow the terminal's permissions.** A binary launched from a shell
  is attributed to the terminal app, so paste can work locally while the installed
  app fails. Verify permission-sensitive changes against the installed bundle.
- **Login item registers only from `/Applications`.** Dev bundles never register, and
  two bundles with the same identifier confuse macOS; delete `target/release/bundle`
  before judging login behaviour.
- **Old binaries ignore new flags.** An older build given an unknown flag starts the
  full app and blocks the shell. Rebuild before calling a newly added flag.
- **Pushing workflow files over HTTPS** needs the `workflow` token scope; the remote uses SSH.
- **tauri-cli is pinned** to the 3 alpha in the release workflow; bump it together with the `tauri` crates.
- **`brew upgrade` without a name upgrades everything.** Use `brew upgrade --cask klepp`.
- **KLEPP_AUTOPICK=1** (debug builds only) picks the newest clip 1.5 s after the panel opens, through the same UI path as Enter.
