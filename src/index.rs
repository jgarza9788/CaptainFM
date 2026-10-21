//! Drive-wide file index for deep search: built once in the background, then searched in memory.

use std::collections::HashMap;
use std::os::unix::fs::MetadataExt;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex, OnceLock, RwLock};
use std::time::{Duration, Instant};

use walkdir::WalkDir;

use crate::fuzzy::Query;
use crate::listing::{Entry, Meta};

/// Directory names never descended into (they would drown results in noise).
const SKIP_DIRS: &[&str] = &[".git", "node_modules", ".cache", ".cargo", ".rustup"];
const MAX_RESULTS: usize = 500;
const MAX_AGE: Duration = Duration::from_secs(120);

pub struct IEntry {
    pub path: Box<str>,
    /// Byte offset of the file name inside `path`.
    pub name_off: u32,
    pub is_dir: bool,
    pub hidden: bool,
    pub size: i64,
    pub mtime: i64,
}

pub struct Index {
    pub root: String,
    pub entries: RwLock<Vec<IEntry>>,
    pub done: AtomicBool,
    built: Instant,
}

static CACHE: OnceLock<Mutex<HashMap<String, Arc<Index>>>> = OnceLock::new();

/// Get the cached index for `root` or start building one. `on_progress` fires as it fills.
/// Total number of paths in all cached indexes (shown in the status bar).
pub fn total_paths() -> usize {
    CACHE
        .get()
        .and_then(|c| c.lock().ok())
        .map_or(0, |g| g.values().map(|i| i.entries.read().map_or(0, |e| e.len())).sum())
}

pub fn get_or_build(root: &str, on_progress: impl Fn() + Send + 'static) -> Arc<Index> {
    let cache = CACHE.get_or_init(|| Mutex::new(HashMap::new()));
    let mut guard = cache.lock().unwrap();
    if let Some(existing) = guard.get(root)
        && (!existing.done.load(Ordering::SeqCst) || existing.built.elapsed() < MAX_AGE) {
            return Arc::clone(existing);
        }
    if guard.len() >= 3
        && let Some(oldest) = guard.iter().min_by_key(|(_, i)| i.built).map(|(k, _)| k.clone()) {
            guard.remove(&oldest);
        }
    let index = Arc::new(Index {
        root: root.to_owned(),
        entries: RwLock::new(Vec::new()),
        done: AtomicBool::new(false),
        built: Instant::now(),
    });
    guard.insert(root.to_owned(), Arc::clone(&index));
    drop(guard);

    let building = Arc::clone(&index);
    std::thread::spawn(move || {
        let t0 = Instant::now();
        build(&building, &on_progress);
        building.done.store(true, Ordering::SeqCst);
        if std::env::var_os("CFM_DEBUG").is_some() {
            eprintln!("[timing] indexed {} entries under {} in {:?}", building.entries.read().unwrap().len(), building.root, t0.elapsed());
        }
        on_progress();
    });
    index
}

fn build(index: &Index, on_progress: &dyn Fn()) {
    let root_len = index.root.trim_end_matches('/').len();
    let mut batch = Vec::with_capacity(8192);
    let mut last_progress = Instant::now();
    let walker = WalkDir::new(&index.root)
        .follow_links(false)
        .same_file_system(true)
        .min_depth(1)
        .into_iter()
        .filter_entry(|e| {
            !(e.file_type().is_dir() && SKIP_DIRS.contains(&e.file_name().to_string_lossy().as_ref()))
        });
    let flush = |batch: &mut Vec<IEntry>| {
        index.entries.write().unwrap().append(batch);
    };
    for e in walker.flatten() {
        let path = e.path().to_string_lossy();
        let name_off = e.path().file_name().map_or(path.len(), |n| path.len() - n.len());
        let meta = e.metadata().ok();
        let hidden = path[root_len..].split('/').any(|c| c.starts_with('.') && !c.is_empty());
        batch.push(IEntry {
            name_off: name_off as u32,
            is_dir: e.file_type().is_dir(),
            hidden,
            size: meta.as_ref().filter(|m| m.is_file()).map_or(-1, |m| m.len() as i64),
            mtime: meta.as_ref().map_or(-1, |m| m.mtime()),
            path: path.into_owned().into_boxed_str(),
        });
        if batch.len() >= 8192 {
            flush(&mut batch);
            if last_progress.elapsed() > Duration::from_millis(150) {
                last_progress = Instant::now();
                on_progress();
            }
        }
    }
    flush(&mut batch);
}

/// Search a snapshot of `index`. Returns the best `MAX_RESULTS` entries in score order.
pub fn search(
    index: &Index,
    query_text: &str,
    show_hidden: bool,
    cancel: &(dyn Fn() -> bool + Sync),
) -> (Vec<Entry>, Vec<Meta>) {
    let t0 = Instant::now();
    let query = Query::parse(query_text);
    let root_len = index.root.trim_end_matches('/').len() + 1;
    let guard = index.entries.read().unwrap();
    let all: &[IEntry] = &guard;

    let threads = std::thread::available_parallelism().map_or(4, |n| n.get().min(8));
    let chunk = all.len().div_ceil(threads).max(1);

    let mut scored: Vec<(i32, usize)> = std::thread::scope(|scope| {
        let handles: Vec<_> = all
            .chunks(chunk)
            .enumerate()
            .map(|(ci, part)| {
                let query = &query;
                scope.spawn(move || {
                    let mut out: Vec<(i32, usize)> = Vec::new();
                    for (i, e) in part.iter().enumerate() {
                        if i % 4096 == 0 && cancel() {
                            break;
                        }
                        if e.hidden && !show_hidden {
                            continue;
                        }
                        let name = &e.path[e.name_off as usize..];
                        if !query.accepts(name, e.is_dir, e.size, e.mtime) {
                            continue;
                        }
                        let rel = e.path.get(root_len..).unwrap_or(name);
                        let Some(s) = query.score(name, rel) else { continue };
                        out.push((s, ci * chunk + i));
                    }
                    // Keep each thread's list bounded.
                    if out.len() > MAX_RESULTS * 4 {
                        out.select_nth_unstable_by(MAX_RESULTS, |a, b| b.0.cmp(&a.0));
                        out.truncate(MAX_RESULTS);
                    }
                    out
                })
            })
            .collect();
        handles.into_iter().flat_map(|h| h.join().unwrap_or_default()).collect()
    });

    scored.sort_by(|a, b| b.0.cmp(&a.0).then(a.1.cmp(&b.1)));
    scored.truncate(MAX_RESULTS);
    if std::env::var_os("CFM_DEBUG").is_some() {
        eprintln!("[timing] search {:?} over {} entries: {:?}", query_text, all.len(), t0.elapsed());
    }
    scored
        .into_iter()
        .map(|(_, i)| {
            let e = &all[i];
            let name = e.path[e.name_off as usize..].to_owned();
            let rel_full = e.path.get(root_len..).unwrap_or(&name);
            let rel = rel_full.rsplit_once('/').map_or("", |x| x.0).to_owned();
            let entry = Entry {
                hidden: name.starts_with('.'),
                lname: name.to_lowercase(),
                path: e.path.to_string(),
                name,
                rel,
                is_dir: e.is_dir,
                is_link: false,
                depth: 0,
            };
            (entry, Meta { size: e.size, mtime: e.mtime })
        })
        .unzip()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::testutil::Sandbox;

    fn built(root: &str) -> Arc<Index> {
        let index = get_or_build(root, || {});
        let deadline = Instant::now() + Duration::from_secs(10);
        while !index.done.load(Ordering::SeqCst) && Instant::now() < deadline {
            std::thread::sleep(Duration::from_millis(10));
        }
        assert!(index.done.load(Ordering::SeqCst), "index did not finish");
        index
    }

    fn names(results: &[Entry]) -> Vec<String> {
        results.iter().map(|e| e.name.clone()).collect()
    }

    #[test]
    fn searches_names_and_paths_with_filters() {
        let sb = Sandbox::new("index");
        sb.touch("src/main.rs", "fn main() {}");
        sb.touch("src/lib.rs", "");
        sb.touch("docs/readme.md", "# hi");
        sb.touch("docs/notes.txt", "");
        sb.touch("node_modules/dep/index.js", "");
        sb.touch(".git/config", "");
        sb.touch(".secret/token.rs", "");
        let root = sb.path("");
        let index = built(&root);
        let search = |q: &str, hidden: bool| search(&index, q, hidden, &|| false).0;

        assert_eq!(names(&search("main", false)), ["main.rs"]);
        assert_eq!(search("main", false)[0].rel, "src");
        // Directory part matches too, and extension filters narrow the set.
        let rs = names(&search("ext:rs", false));
        assert!(rs.contains(&"main.rs".to_string()) && rs.contains(&"lib.rs".to_string()));
        assert!(!rs.contains(&"readme.md".to_string()));
        // Noise folders are never indexed; hidden files appear only on request.
        assert!(search("index.js", false).is_empty());
        assert!(search("config", true).iter().all(|e| !e.path.contains("/.git/")));
        assert!(search("token", false).is_empty());
        assert_eq!(names(&search("token", true)), ["token.rs"]);
        // Folders only.
        assert!(search("type:d", false).iter().all(|e| e.is_dir));
    }

    #[test]
    fn regex_queries_and_results_are_bounded() {
        let sb = Sandbox::new("index-regex");
        for i in 0..40 {
            sb.touch(&format!("many/file{i}.txt"), "x");
        }
        sb.touch("many/other.log", "");
        let index = built(&sb.path(""));
        let (hits, metas) = search(&index, "re:^file1\\d\\.txt$", false, &|| false);
        assert_eq!(hits.len(), 10);
        assert_eq!(hits.len(), metas.len());
        assert!(search(&index, "re:(", false, &|| false).0.is_empty());
        assert!(total_paths() >= 41);
    }

    #[test]
    fn cancelled_searches_return_quickly() {
        let sb = Sandbox::new("index-cancel");
        sb.touch("a.txt", "x");
        let index = built(&sb.path(""));
        // A cancel flag that is already set must not hang or panic.
        let _ = search(&index, "a", false, &|| true);
    }
}
