//! System font family discovery for the inspector's font dropdown.
//!
//! One background scan at startup walks the standard font directories and
//! lists every family it can parse. Faces load from disk lazily in text.rs
//! the first time a family is picked, so an untouched document costs nothing
//! and stale files never block startup.

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::OnceLock;

/// One style face inside a font file.
#[derive(Debug, Clone)]
pub struct FaceFile {
    pub file: PathBuf,
    pub index: u32,
    pub bold: bool,
    pub italic: bool,
}

/// A family name and the faces that can serve it.
#[derive(Debug)]
pub struct Family {
    pub name: String,
    pub faces: Vec<FaceFile>,
}

#[derive(Debug, Default)]
pub struct Library {
    /// Families sorted by name (case-insensitive).
    pub families: Vec<Family>,
}

impl Library {
    /// Exact match first, then ASCII case-insensitive.
    pub fn find(&self, name: &str) -> Option<&Family> {
        self.families.iter().find(|f| f.name == name).or_else(|| {
            self.families
                .iter()
                .find(|f| f.name.eq_ignore_ascii_case(name))
        })
    }
}

static LIBRARY: OnceLock<Library> = OnceLock::new();
static SCAN_STARTED: OnceLock<()> = OnceLock::new();
/// Bumped when the scan finishes, so render caches built before any system
/// family was resolvable can throw away their fallback-font output.
static EPOCH: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);

/// Kick off the scan once. Cheap to call repeatedly.
pub fn scan_once() {
    if SCAN_STARTED.set(()).is_ok() {
        let _ = std::thread::Builder::new()
            .name("font-scan".into())
            .spawn(|| {
                let lib = scan_system_dirs();
                let _ = LIBRARY.set(lib);
                EPOCH.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
            });
    }
}

/// How many times the library has landed in this process (0 while the scan
/// runs). Render caches compare this to rebuild once families appear.
pub fn library_epoch() -> u64 {
    EPOCH.load(std::sync::atomic::Ordering::Relaxed)
}

/// The scanned library, or None while the scan is still running.
pub fn library() -> Option<&'static Library> {
    LIBRARY.get()
}

/// Replace the library wholesale. Tests point it at bundled font files so
/// assertions stay deterministic regardless of what the host has installed.
#[cfg(test)]
pub(crate) fn install_test_library(dirs: &[PathBuf]) {
    SCAN_STARTED.set(()).ok();
    LIBRARY.set(scan_dirs(dirs)).ok();
}

fn font_dirs() -> Vec<PathBuf> {
    let mut dirs = Vec::new();
    let home = std::env::var_os("HOME")
        .map(PathBuf::from)
        .unwrap_or_default();
    if let Some(d) = std::env::var_os("XDG_DATA_HOME") {
        dirs.push(PathBuf::from(d).join("fonts"));
    } else {
        dirs.push(home.join(".local/share/fonts"));
    }
    dirs.push(home.join("Library/Fonts"));
    dirs.push(PathBuf::from("/usr/local/share/fonts"));
    dirs.push(PathBuf::from("/usr/share/fonts"));
    dirs
}

fn scan_system_dirs() -> Library {
    scan_dirs(&font_dirs())
}

/// Scan explicit directories (also used by tests with bundled font files).
pub fn scan_dirs(dirs: &[PathBuf]) -> Library {
    let mut by_name: HashMap<String, Vec<FaceFile>> = HashMap::new();
    for dir in dirs {
        walk(dir, 0, &mut |path| examine(path, &mut by_name));
    }
    let mut names: Vec<String> = by_name.keys().cloned().collect();
    names.sort_by_key(|n| n.to_lowercase());
    Library {
        families: names
            .into_iter()
            .map(|name| {
                let faces = by_name.remove(&name).unwrap_or_default();
                Family { name, faces }
            })
            .collect(),
    }
}

fn walk(dir: &Path, depth: u8, f: &mut impl FnMut(&Path)) {
    if depth > 6 {
        return;
    }
    let Ok(rd) = std::fs::read_dir(dir) else {
        return;
    };
    for e in rd.flatten() {
        let p = e.path();
        if p.is_dir() {
            walk(&p, depth + 1, f);
        } else if matches!(
            p.extension().and_then(|x| x.to_str()),
            Some("ttf" | "otf" | "TTF" | "OTF")
        ) {
            f(&p);
        }
    }
}

fn examine(path: &Path, out: &mut HashMap<String, Vec<FaceFile>>) {
    let Ok(bytes) = std::fs::read(path) else {
        return;
    };
    let count = ttf_parser::fonts_in_collection(&bytes).unwrap_or(1);
    for index in 0..count {
        let Ok(face) = ttf_parser::Face::parse(&bytes, index) else {
            continue;
        };
        let Some(name) = family_name(&face) else {
            continue;
        };
        let bold = face.is_bold();
        let italic = face.is_italic();
        let faces = out.entry(name).or_default();
        // Skip files already visited for this family (aliases across machines can
        // point at the same family name twice).
        if faces.iter().any(|f| f.file == path && f.index == index) {
            continue;
        }
        faces.push(FaceFile {
            file: path.to_path_buf(),
            index,
            bold,
            italic,
        });
    }
}

fn family_name(face: &ttf_parser::Face) -> Option<String> {
    use ttf_parser::name_id;
    // Prefer the full/typographic family, falling back to the legacy one for
    // files that only carry the four-style split (Arial Black style naming).
    for id in [name_id::TYPOGRAPHIC_FAMILY, name_id::FAMILY] {
        for n in face.names().into_iter() {
            if n.name_id == id
                && let Some(s) = n.to_string()
                && !s.trim().is_empty()
            {
                return Some(s.trim().to_string());
            }
        }
    }
    None
}
