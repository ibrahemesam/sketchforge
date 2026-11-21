//! App-wide options stored as `options.json` next to the projects folder.

use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

pub const FILE: &str = "options.json";

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ThemeChoice {
    #[default]
    System,
    Light,
    Dark,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Options {
    pub theme: ThemeChoice,
    /// Canvas grid spacing in world units (draw.io's Grid size). Keep this a
    /// multiple of anything snapping treats as a hard floor; 10 is the default.
    pub grid_size: f64,
    /// Folder scanned for custom template libraries (M7). None when the user
    /// has not chosen one; the bundled packs always load.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub template_folder: Option<PathBuf>,
    /// Inspector section keys mapped to `false` when collapsed. Missing keys
    /// are expanded, so older option files keep their current layout.
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub inspector_sections: BTreeMap<String, bool>,
    /// The first-run tour has been shown. Old option files default to false,
    /// so existing users see it once and then keep the dismissal.
    pub onboarding_seen: bool,
}

impl Default for Options {
    fn default() -> Self {
        Options {
            theme: ThemeChoice::default(),
            grid_size: 10.0,
            template_folder: None,
            inspector_sections: BTreeMap::new(),
            onboarding_seen: false,
        }
    }
}

/// `grid_size` may load as garbage (old files hand-edited, corrupted json).
fn grid_size_default(v: f64) -> f64 {
    if v.is_finite() && (2.0..=100.0).contains(&v) {
        v
    } else {
        10.0
    }
}

impl Options {
    pub fn path_in(root: &Path) -> PathBuf {
        root.join(FILE)
    }

    pub fn load(root: &Path) -> Options {
        let path = Self::path_in(root);
        match std::fs::read_to_string(&path) {
            Ok(s) => {
                let mut o: Options = serde_json::from_str(&s).unwrap_or_else(|e| {
                    log::warn!("ignoring {}: {e}", path.display());
                    Options::default()
                });
                o.grid_size = grid_size_default(o.grid_size);
                o
            }
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Options::default(),
            Err(e) => {
                log::warn!("could not read {}: {e}", path.display());
                Options::default()
            }
        }
    }

    pub fn save(&self, root: &Path) -> std::io::Result<()> {
        std::fs::create_dir_all(root)?;
        crate::io::atomic::write(
            &Self::path_in(root),
            serde_json::to_string_pretty(self)
                .expect("options serialize")
                .as_bytes(),
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn root(tag: &str) -> PathBuf {
        std::env::temp_dir().join(format!(
            "drawio-rs-options-{tag}-{:016x}",
            fastrand::u64(..)
        ))
    }

    #[test]
    fn roundtrip() {
        let r = root("rt");
        let o = Options {
            theme: ThemeChoice::Light,
            grid_size: 20.0,
            template_folder: Some(PathBuf::from("/tmp/templates")),
            inspector_sections: BTreeMap::new(),
            onboarding_seen: false,
        };
        o.save(&r).unwrap();
        assert_eq!(Options::load(&r), o);
        std::fs::remove_dir_all(r).ok();
    }

    #[test]
    fn defaults_on_missing_or_bad() {
        let r = root("bad");
        assert_eq!(Options::load(&r), Options::default());
        std::fs::create_dir_all(&r).unwrap();
        std::fs::write(Options::path_in(&r), b"oops").unwrap();
        assert_eq!(Options::load(&r), Options::default());
        std::fs::write(Options::path_in(&r), b"{\"theme\":\"nope\"}").unwrap();
        assert_eq!(Options::load(&r), Options::default());
        std::fs::remove_dir_all(r).ok();
    }

    #[test]
    fn onboarding_seen_defaults_false_and_survives_options_round_trip() {
        let r = root("onboarding");
        assert!(!Options::load(&r).onboarding_seen);
        let o = Options {
            onboarding_seen: true,
            ..Options::default()
        };
        o.save(&r).unwrap();
        assert!(Options::load(&r).onboarding_seen);
        std::fs::remove_dir_all(r).ok();
    }

    #[test]
    fn section_states_survive_options_round_trip() {
        let r = root("sections");
        std::fs::create_dir_all(&r).unwrap();
        std::fs::write(
            Options::path_in(&r),
            br#"{"theme":"dark","grid_size":10.0,"inspector_sections":{"fill":false,"arrange":true}}"#,
        )
        .unwrap();
        let o = Options::load(&r);
        o.save(&r).unwrap();
        let saved = std::fs::read_to_string(Options::path_in(&r)).unwrap();
        let reloaded: Options = serde_json::from_str(&saved).unwrap();
        assert_eq!(reloaded.inspector_sections.get("fill"), Some(&false));
        assert_eq!(reloaded.inspector_sections.get("arrange"), Some(&true));
        std::fs::remove_dir_all(r).ok();
    }
}
