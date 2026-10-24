//! App-wide options stored as `options.json` next to the projects folder.

use serde::{Deserialize, Serialize};
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

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Options {
    pub theme: ThemeChoice,
    /// Canvas grid spacing in world units (draw.io's Grid size). Keep this a
    /// multiple of anything snapping treats as a hard floor; 10 is the default.
    pub grid_size: f64,
}

impl Default for Options {
    fn default() -> Self {
        Options {
            theme: ThemeChoice::default(),
            grid_size: 10.0,
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
            "sketch-rs-options-{tag}-{:016x}",
            fastrand::u64(..)
        ))
    }

    #[test]
    fn roundtrip() {
        let r = root("rt");
        let o = Options {
            theme: ThemeChoice::Light,
            grid_size: 20.0,
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
}
