//! Persistent state: config files, bookmarks and frecency-ranked recent folders.

use std::fs;
use std::path::PathBuf;
use std::sync::Mutex;
use std::time::{SystemTime, UNIX_EPOCH};

use serde::{Deserialize, Serialize};

use crate::fuzzy::{Query, natural_cmp_pub};

pub fn home() -> PathBuf {
    std::env::var_os("HOME").map(PathBuf::from).unwrap_or_else(|| PathBuf::from("/"))
}

fn xdg(var: &str, fallback: &str) -> PathBuf {
    std::env::var_os(var)
        .map(PathBuf::from)
        .filter(|p| p.is_absolute())
        .unwrap_or_else(|| home().join(fallback))
        .join("captainfm")
}

pub fn config_dir() -> PathBuf {
    xdg("XDG_CONFIG_HOME", ".config")
}
pub fn data_dir() -> PathBuf {
    xdg("XDG_DATA_HOME", ".local/share")
}
pub fn cache_dir() -> PathBuf {
    xdg("XDG_CACHE_HOME", ".cache")
}

pub fn now() -> i64 {
    SystemTime::now().duration_since(UNIX_EPOCH).map_or(0, |d| d.as_secs() as i64)
}

/// Read a named JSON config blob (`settings`, `keys`, `session`, `layouts`); `{}` when absent.
pub fn read_config(name: &str) -> String {
    fs::read_to_string(config_dir().join(format!("{}.json", safe_name(name)))).unwrap_or_else(|_| "{}".into())
}

fn safe_name(name: &str) -> String {
    name.chars().filter(|c| c.is_ascii_alphanumeric() || *c == '_').collect()
}

pub fn write_config(name: &str, json: &str) -> bool {
    let dir = config_dir();
    fs::create_dir_all(&dir).is_ok() && fs::write(dir.join(format!("{}.json", safe_name(name))), json).is_ok()
}

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct Recent {
    pub path: String,
    pub visits: u32,
    pub last: i64,
}

#[derive(Serialize, Deserialize, Default, Debug)]
pub struct Store {
    pub bookmarks: Vec<String>,
    pub recents: Vec<Recent>,
}

static STORE: Mutex<Option<Store>> = Mutex::new(None);

pub fn store_file() -> PathBuf {
    data_dir().join("state.json")
}

fn with_store<R>(f: impl FnOnce(&mut Store) -> R) -> R {
    let mut guard = STORE.lock().unwrap();
    let store = guard.get_or_insert_with(|| {
        fs::read_to_string(store_file())
            .ok()
            .and_then(|t| serde_json::from_str(&t).ok())
            .unwrap_or_default()
    });
    f(store)
}

fn save(store: &Store) {
    let _ = fs::create_dir_all(data_dir());
    if let Ok(text) = serde_json::to_string(store) {
        let _ = fs::write(store_file(), text);
    }
}

/// zoxide-style frecency: visit count weighted by how recent the last visit was.
pub fn frecency(r: &Recent, now: i64) -> f64 {
    let age = (now - r.last).max(0);
    let weight = match age {
        0..=3_599 => 4.0,
        3_600..=86_399 => 2.0,
        86_400..=604_799 => 0.5,
        _ => 0.25,
    };
    r.visits as f64 * weight
}

pub fn record_visit(path: &str) {
    if path.is_empty() || path == "/" {
        return;
    }
    with_store(|s| {
        let now = now();
        match s.recents.iter_mut().find(|r| r.path == path) {
            Some(r) => {
                r.visits += 1;
                r.last = now;
            }
            None => s.recents.push(Recent { path: path.to_owned(), visits: 1, last: now }),
        }
        // Keep the list bounded: drop the lowest-ranked entries.
        if s.recents.len() > 500 {
            s.recents.sort_by(|a, b| frecency(b, now).total_cmp(&frecency(a, now)));
            s.recents.truncate(400);
        }
        save(s);
    });
}

/// Forget the cached store so the next use re-reads it (after another instance changed the file,
/// or in tests that switch data directories).
pub fn reset_store() {
    *STORE.lock().unwrap() = None;
}

pub fn bookmarks() -> Vec<String> {
    with_store(|s| s.bookmarks.clone())
}

/// Returns true when `path` is bookmarked after the call.
pub fn toggle_bookmark(path: &str) -> bool {
    with_store(|s| {
        let now_marked = if let Some(i) = s.bookmarks.iter().position(|b| b == path) {
            s.bookmarks.remove(i);
            false
        } else {
            s.bookmarks.push(path.to_owned());
            true
        };
        save(s);
        now_marked
    })
}

#[derive(Serialize)]
pub struct Candidate {
    pub path: String,
    pub kind: &'static str,
}

pub fn expand_tilde(p: &str) -> String {
    match p.strip_prefix('~') {
        Some(rest) if rest.is_empty() || rest.starts_with('/') => {
            format!("{}{}", home().to_string_lossy(), rest)
        }
        _ => p.to_owned(),
    }
}

/// Directory-name completion for a partially typed path.
fn complete_path(query: &str) -> Vec<Candidate> {
    let expanded = expand_tilde(query);
    let (dir, prefix) = match expanded.rfind('/') {
        Some(i) => (&expanded[..=i], &expanded[i + 1..]),
        None => return Vec::new(),
    };
    let lower = prefix.to_lowercase();
    let mut names: Vec<String> = fs::read_dir(dir)
        .into_iter()
        .flatten()
        .flatten()
        .filter(|e| e.path().is_dir())
        .map(|e| e.file_name().to_string_lossy().into_owned())
        .filter(|n| (prefix.starts_with('.') || !n.starts_with('.')) && n.to_lowercase().starts_with(&lower))
        .collect();
    names.sort_by(|a, b| natural_cmp_pub(a, b));
    names
        .into_iter()
        .take(60)
        .map(|n| Candidate { path: format!("{dir}{n}"), kind: "path" })
        .collect()
}

/// Well-known folders offered by the jump bar (only those that exist).
fn places() -> Vec<String> {
    let h = home();
    let mut out: Vec<String> = ["", "Desktop", "Documents", "Downloads", "Music", "Pictures", "Videos", "Projects", ".config", ".local/share/Trash/files"]
        .iter()
        .map(|d| if d.is_empty() { h.clone() } else { h.join(d) })
        .filter(|p| p.is_dir())
        .map(|p| p.to_string_lossy().into_owned())
        .collect();
    out.extend(["/", "/tmp", "/etc", "/usr", "/var"].iter().filter(|p| std::path::Path::new(p).is_dir()).map(|p| (*p).to_owned()));
    out
}

/// Candidates for the jump bar: path completion for path-like input, else bookmarks and
/// frecent folders filtered (and ranked) by the fuzzy query.
pub fn jump_candidates(query: &str) -> Vec<Candidate> {
    let query = query.trim();
    if query.starts_with('/') || query.starts_with('~') {
        return complete_path(query);
    }
    let q = Query::parse(query);
    let now = now();
    let (bookmarks, recents) = with_store(|s| (s.bookmarks.clone(), s.recents.clone()));

    let mut scored: Vec<(f64, Candidate)> = Vec::new();
    for b in &bookmarks {
        let name = b.rsplit('/').next().unwrap_or(b);
        if let Some(s) = q.score(name, b) {
            scored.push((1_000_000.0 + s as f64, Candidate { path: b.clone(), kind: "pinned" }));
        }
    }
    for r in &recents {
        if bookmarks.contains(&r.path) || !std::path::Path::new(&r.path).is_dir() {
            continue;
        }
        let name = r.path.rsplit('/').next().unwrap_or(&r.path);
        if let Some(s) = q.score(name, &r.path) {
            scored.push((frecency(r, now) + s as f64 * 4.0, Candidate { path: r.path.clone(), kind: "recent" }));
        }
    }
    for place in places() {
        if bookmarks.contains(&place) || scored.iter().any(|(_, c)| c.path == place) {
            continue;
        }
        let name = place.rsplit('/').next().filter(|n| !n.is_empty()).unwrap_or("/");
        // Places rank below frecent folders for the same query, but are always reachable.
        if let Some(s) = q.score(name, &place) {
            scored.push((s as f64 * 0.5, Candidate { path: place, kind: "place" }));
        }
    }
    scored.sort_by(|a, b| b.0.total_cmp(&a.0));
    scored.into_iter().take(60).map(|(_, c)| c).collect()
}

/// Walk up from `path` to the top of its filesystem (the "drive" for whole-drive search).
pub fn mount_root(path: &str) -> String {
    use std::os::unix::fs::MetadataExt;
    let Ok(dev) = fs::metadata(path).map(|m| m.dev()) else {
        return "/".into();
    };
    let mut best = PathBuf::from(path);
    let mut cur = PathBuf::from(path);
    while let Some(parent) = cur.parent() {
        if fs::metadata(parent).map(|m| m.dev()).ok() != Some(dev) {
            break;
        }
        best = parent.to_path_buf();
        cur = parent.to_path_buf();
    }
    best.to_string_lossy().into_owned()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn frecency_prefers_recent() {
        let now = 1_000_000;
        let fresh = Recent { path: "a".into(), visits: 2, last: now - 10 };
        let stale = Recent { path: "b".into(), visits: 2, last: now - 90 * 86_400 };
        assert!(frecency(&fresh, now) > frecency(&stale, now));
    }

    #[test]
    fn tilde_expansion() {
        assert_eq!(expand_tilde("/x"), "/x");
        assert!(expand_tilde("~/y").ends_with("/y"));
        assert_eq!(expand_tilde("~other"), "~other");
    }

    #[test]
    fn config_name_is_sanitised() {
        assert_eq!(safe_name("../../etc/passwd"), "etcpasswd");
    }
}

#[cfg(test)]
mod store_tests {
    use super::*;
    use crate::testutil::Sandbox;

    #[test]
    fn bookmarks_toggle_and_persist() {
        let sb = Sandbox::new("bookmarks");
        let dir = sb.path("proj");
        assert!(toggle_bookmark(&dir));
        assert_eq!(bookmarks(), std::slice::from_ref(&dir));
        // The file is written, so a fresh process would see it.
        reset_store();
        assert_eq!(bookmarks(), std::slice::from_ref(&dir));
        assert!(!toggle_bookmark(&dir));
        assert!(bookmarks().is_empty());
    }

    #[test]
    fn visits_rank_frequent_folders_first() {
        let sb = Sandbox::new("visits");
        let (a, b) = (sb.touch("alpha/", ""), sb.touch("beta/", ""));
        let (a, b) = (a.to_string_lossy().into_owned(), b.to_string_lossy().into_owned());
        record_visit(&a);
        for _ in 0..3 {
            record_visit(&b);
        }
        let hits = jump_candidates("");
        let pos = |p: &str| hits.iter().position(|c| c.path == p).unwrap();
        assert!(pos(&b) < pos(&a));
        assert_eq!(hits[pos(&b)].kind, "recent");
    }

    #[test]
    fn jump_offers_common_places_and_pins_first() {
        let sb = Sandbox::new("places");
        sb.touch("Downloads/", "");
        let docs = sb.touch("Documents/", "");
        toggle_bookmark(&docs.to_string_lossy());
        let hits = jump_candidates("");
        assert_eq!(hits[0].kind, "pinned");
        assert!(hits.iter().any(|c| c.kind == "place" && c.path.ends_with("Downloads")));
        // Names are matched fuzzily and by ordered words.
        assert!(jump_candidates("down").iter().any(|c| c.path.ends_with("Downloads")));
    }

    #[test]
    fn path_completion_lists_matching_folders_only() {
        let sb = Sandbox::new("complete");
        sb.touch("Projects/a.txt", "x");
        sb.touch("Pictures/", "");
        sb.touch("Music/", "");
        sb.touch(".hidden/", "");
        let prefix = sb.path("P");
        let names: Vec<String> = jump_candidates(&prefix).into_iter().map(|c| c.path).collect();
        assert_eq!(names, [sb.path("Pictures"), sb.path("Projects")]);
        // Hidden folders only show up when asked for.
        assert!(jump_candidates(&format!("{}/", sb.root.display())).iter().all(|c| !c.path.contains("/.hidden")));
        assert!(jump_candidates(&sb.path(".h")).iter().any(|c| c.path.ends_with(".hidden")));
    }

    #[test]
    fn config_round_trip() {
        let _sb = Sandbox::new("config");
        assert_eq!(read_config("settings"), "{}");
        assert!(write_config("settings", "{\"a\":1}"));
        assert_eq!(read_config("settings"), "{\"a\":1}");
    }

    #[test]
    fn mount_root_stays_on_the_same_filesystem() {
        let sb = Sandbox::new("mount");
        let nested = sb.touch("a/b/", "");
        let root = mount_root(&nested.to_string_lossy());
        assert!(nested.starts_with(&root));
        assert!(std::path::Path::new(&root).is_dir());
    }
}
