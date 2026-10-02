//! `~/.klepp/config.toml`, created with defaults on first run.

use serde::{Deserialize, Serialize};
use std::fs;
use std::path::{Path, PathBuf};

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(default)]
pub struct Config {
    /// Global shortcut that toggles the panel, e.g. "ctrl+shift+v" or "cmd+shift+space".
    pub shortcut: String,
    /// Clips copied while an app whose bundle id contains one of these
    /// (case-insensitive) is frontmost are never recorded.
    pub ignore_apps: Vec<String>,
    /// Images larger than this are not recorded.
    pub max_image_mb: u64,
    /// Apps (bundle-id substrings) where an image clip is pasted with Ctrl+V
    /// instead of Cmd+V. Terminals: that is how Claude Code takes images.
    pub ctrl_v_image_apps: Vec<String>,
}

impl Default for Config {
    fn default() -> Self {
        Config {
            shortcut: "ctrl+shift+v".into(),
            ignore_apps: vec!["1password".into()],
            max_image_mb: 10,
            ctrl_v_image_apps: [
                "com.mitchellh.ghostty",
                "com.cmuxterm.",
                "com.apple.Terminal",
                "com.googlecode.iterm2",
                "dev.warp.",
                "net.kovidgoyal.kitty",
                "org.alacritty",
                "com.github.wez.wezterm",
            ]
            .map(String::from)
            .to_vec(),
        }
    }
}

const TEMPLATE: &str = r#"# Klepp configuration. Pick "Reload config" from the menu bar after editing.

# Global shortcut that toggles the panel.
# Modifiers: ctrl, shift, alt (option), cmd (super). Keys: letters, digits, space, f1-f12, ...
shortcut = "ctrl+shift+v"

# Clips copied while an app whose bundle id contains one of these strings is
# frontmost are never recorded. Clips flagged as concealed (which 1Password
# sets) are always skipped regardless of this list.
ignore_apps = ["1password"]

# Images larger than this (in megabytes) are not recorded.
max_image_mb = 10

# Apps (bundle-id substrings) where an image is pasted with Ctrl+V instead of
# Cmd+V. Terminals paste text on Cmd+V; Claude Code in a terminal takes images
# on Ctrl+V. Ctrl+Enter in the panel forces Ctrl+V anywhere.
ctrl_v_image_apps = [
  "com.mitchellh.ghostty",
  "com.cmuxterm.",
  "com.apple.Terminal",
  "com.googlecode.iterm2",
  "dev.warp.",
  "net.kovidgoyal.kitty",
  "org.alacritty",
  "com.github.wez.wezterm",
]
"#;

impl Config {
    pub fn path(root: &Path) -> PathBuf {
        root.join("config.toml")
    }

    /// Load the config, writing the commented template first if it is missing.
    pub fn load(root: &Path) -> Result<Config, String> {
        let path = Self::path(root);
        if !path.exists() {
            fs::write(&path, TEMPLATE).map_err(|e| e.to_string())?;
        }
        let raw = fs::read_to_string(&path).map_err(|e| e.to_string())?;
        toml::from_str(&raw).map_err(|e| format!("{}: {e}", path.display()))
    }

    pub fn max_image_bytes(&self) -> usize {
        (self.max_image_mb as usize).saturating_mul(1024 * 1024)
    }

    pub fn ignores(&self, bundle_id: &str) -> bool {
        matches_any(&self.ignore_apps, bundle_id)
    }

    /// Should an image clip be pasted into this app with Ctrl+V?
    pub fn image_paste_uses_ctrl_v(&self, bundle_id: &str) -> bool {
        matches_any(&self.ctrl_v_image_apps, bundle_id)
    }
}

/// Case-insensitive "bundle id contains one of these patterns".
fn matches_any(patterns: &[String], bundle_id: &str) -> bool {
    let lower = bundle_id.to_lowercase();
    patterns
        .iter()
        .any(|p| !p.is_empty() && lower.contains(&p.to_lowercase()))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn terminals_paste_images_with_ctrl_v() {
        let cfg = Config::default();
        assert!(cfg.image_paste_uses_ctrl_v("com.mitchellh.ghostty"));
        assert!(cfg.image_paste_uses_ctrl_v("com.cmuxterm.app"));
        assert!(cfg.image_paste_uses_ctrl_v("com.apple.Terminal"));
        assert!(cfg.image_paste_uses_ctrl_v("dev.warp.Warp-Stable"));
        assert!(!cfg.image_paste_uses_ctrl_v("org.mozilla.firefox"));
        assert!(!cfg.image_paste_uses_ctrl_v("com.apple.TextEdit"));
    }

    #[test]
    fn template_parses_to_defaults() {
        let parsed: Config = toml::from_str(TEMPLATE).unwrap();
        let default = Config::default();
        assert_eq!(parsed.shortcut, default.shortcut);
        assert_eq!(parsed.ignore_apps, default.ignore_apps);
        assert_eq!(parsed.ctrl_v_image_apps, default.ctrl_v_image_apps);
    }

    #[test]
    fn old_config_without_new_keys_still_loads() {
        let cfg: Config = toml::from_str("shortcut = \"cmd+shift+v\"\n").unwrap();
        assert_eq!(cfg.shortcut, "cmd+shift+v");
        assert!(cfg.image_paste_uses_ctrl_v("com.mitchellh.ghostty"));
        assert!(cfg.ignores("com.1password.1password"));
    }
}
