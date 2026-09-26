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
}

impl Default for Config {
    fn default() -> Self {
        Config {
            shortcut: "ctrl+shift+v".into(),
            ignore_apps: vec!["1password".into()],
            max_image_mb: 10,
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
        let lower = bundle_id.to_lowercase();
        self.ignore_apps
            .iter()
            .any(|p| !p.is_empty() && lower.contains(&p.to_lowercase()))
    }
}
