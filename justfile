# See https://just.systems/man/en/ for docs

app := "target/release/bundle/macos/Klepp.app"

# List available commands
default:
    just --list

# ============ DEVELOP ============

# Build the debug binary
build:
    cargo build

# Run from source with the panel open right away
run:
    cargo run -- --show

# Run the store unit tests
test:
    cargo test

# Format + lint + tests + cargo-deny (what CI runs)
check:
    cargo fmt --check
    cargo clippy --all-targets -- -D warnings
    cargo test
    cargo deny check

# Package a release zip the same way CI does (target/Klepp-<version>-aarch64.zip)
package: bundle
    #!/usr/bin/env bash
    set -euo pipefail
    version=$(grep -m1 '^version = ' Cargo.toml | cut -d'"' -f2)
    app=target/release/bundle/macos/Klepp.app
    codesign --force --deep --sign - "$app"
    ditto -c -k --keepParent "$app" "target/Klepp-${version}-aarch64.zip"
    shasum -a 256 "target/Klepp-${version}-aarch64.zip"

# ============ SHIP ============

# Build the release Klepp.app bundle (needs tauri-cli 3: cargo install tauri-cli --version "^3.0.0-alpha")
bundle:
    cargo tauri build

# Build, copy to /Applications and launch (Klepp registers itself as a login item)
install: bundle
    rm -rf /Applications/Klepp.app
    cp -R {{app}} /Applications/Klepp.app
    -pkill -x klepp
    open /Applications/Klepp.app
    @echo '✅ Installed. Press Ctrl+Shift+V.'

# Stop Klepp and remove the app (clips in ~/.klepp are kept)
uninstall:
    -/Applications/Klepp.app/Contents/MacOS/klepp --login-status >/dev/null 2>&1
    -pkill -x klepp
    rm -rf /Applications/Klepp.app
    @echo '🗑  Removed the app. Your clips are still in ~/.klepp.'

# Is Klepp registered to launch at login?
login-status:
    /Applications/Klepp.app/Contents/MacOS/klepp --login-status

# Show the newest clips in the database
clips n="20":
    @sqlite3 -header ~/.klepp/klepp.db "SELECT id, datetime(ts/1000,'unixepoch','localtime') AS at, kind, app, substr(replace(text,char(10),' '),1,60) AS text, width||'x'||height AS size FROM clips ORDER BY ts DESC LIMIT {{n}};"

# Open the config file in your editor
config:
    open -t ~/.klepp/config.toml
