# klepp

A small, glassy clipboard manager for Apple Silicon Macs on macOS 26+.

- Keeps a searchable history of the text and images you copy.
- **Ctrl+Shift+V** opens a Liquid Glass panel across the bottom of the screen.
- Never records anything copied from 1Password, or any clip an app marks as concealed.
- Everything stays on your Mac, in `~/.klepp/klepp.db`.
- Lives in the menu bar and starts with your Mac.

## Install

```sh
brew tap jonasks/tap
brew trust jonasks/tap      # Homebrew requires this once for third-party taps
brew install --cask klepp
open /Applications/Klepp.app
```

Releases are signed and notarized by Apple. Upgrade with `brew upgrade --cask klepp`.

## Use

| Key | Action |
|-----|--------|
| **Ctrl+Shift+V** | Open or close the panel |
| Type | Search; every word must match |
| **↑ / ↓** | Move through clips |
| **⏎** | Paste the selected clip into the app you came from |
| **⌘⌫** or ✕ | Delete the selected clip |
| **esc** | Close |

Images appear as thumbnails. Search `image`, a size like `1200x800`, or the name of
the app you copied from to find them. Copying something again moves it to the top
instead of duplicating it.

The clipboard icon in the menu bar lets you pause recording, choose when old clips
are deleted (never, or after 1, 7, 30, 90 days or 1 year), edit the configuration,
and quit.

## Permissions

Pasting with **⏎** needs **Accessibility** access. Klepp asks for it the first time
you press ⏎; switch Klepp on in System Settings → Privacy & Security → Accessibility.
Without it the clip is still copied, and you can paste it yourself with ⌘V.

## Start at login

Klepp registers itself as a login item the first time you open it, so it starts
with your Mac from then on.

| To | Do |
|----|----|
| Turn it off or on | Menu bar icon → **Launch at login** |
| Check from a terminal | `/Applications/Klepp.app/Contents/MacOS/klepp --login-status` |
| See it in macOS | System Settings → General → Login Items & Extensions |

## Configuration

`~/.klepp/config.toml` is created on first launch:

```toml
shortcut = "ctrl+shift+v"     # e.g. "cmd+shift+space", "alt+v"
ignore_apps = ["1password"]   # apps whose clips are never recorded (bundle-id substrings)
max_image_mb = 10             # larger images are skipped
```

Open it with **Edit config.toml…** in the menu bar, then pick **Reload config**.
No restart needed.

## Build from source

Requires Rust, [just](https://just.systems) and the Tauri 3 CLI
(`cargo install tauri-cli --version "^3.0.0-alpha"`).

```sh
just run         # run from source with the panel open
just install     # build Klepp.app and copy it to /Applications
just             # list all recipes
```

## License

MIT
