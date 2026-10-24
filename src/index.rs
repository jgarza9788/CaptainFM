//! Drive-wide file index for deep search: built once in the background, then searched in memory.
//!
//! The walk runs on several threads and reads no sizes or dates (only names and kinds, which the
//! directory listing already carries). Those are looked up for the few results that are shown, and for
//! the entries that survive the name match when the query filters on size or date. Names live in one
//! shared text buffer and folders are stored once, which keeps a million-entry index small.
//! A finished index is kept: after a while it only checks the folders' modified times, and rebuilds
//! (while still serving the old one) only if something changed.

use std::collections::HashMap;
use std::fs;
use std::os::unix::fs::MetadataExt;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Condvar, Mutex, OnceLock, RwLock};
use std::time::{Duration, Instant};

use crate::fuzzy::Query;
use crate::listing::{Entry, Meta};

/// Directory names never descended into (they would drown results in noise).
const SKIP_DIRS: &[&str] = &[".git", "node_modules", ".cache", ".cargo", ".rustup"];
const MAX_RESULTS: usize = 500;
/// How long a finished index is trusted before its folders are checked for changes.
const CHECK_AFTER: Duration = Duration::from_secs(30);

/// A folder in the index; entries point at it by position.
struct DirInfo {
    abs: Box<str>,
    /// Modified time when it was read, to notice later changes.
    mtime: (i64, i64),
}

struct IEntry {
    dir: u32,
    /// Where the name starts in `Store::names`.
    name_off: u32,
    name_len: u16,
    is_dir: bool,
    hidden: bool,
}

#[derive(Default)]
struct Store {
    dirs: Vec<DirInfo>,
    names: String,
    entries: Vec<IEntry>,
}

impl Store {
    fn name(&self, e: &IEntry) -> &str {
        &self.names[e.name_off as usize..e.name_off as usize + e.name_len as usize]
    }

    fn path(&self, e: &IEntry) -> PathBuf {
        Path::new(&*self.dirs[e.dir as usize].abs).join(self.name(e))
    }
}

pub struct Index {
    pub root: String,
    store: RwLock<Store>,
    pub done: AtomicBool,
    checked: Mutex<Instant>,
    refreshing: AtomicBool,
}

static CACHE: OnceLock<Mutex<HashMap<String, Arc<Index>>>> = OnceLock::new();

fn mtime_of(m: &fs::Metadata) -> (i64, i64) {
    (m.mtime(), m.mtime_nsec())
}

/// Total number of paths in all cached indexes (shown in the status bar).
pub fn total_paths() -> usize {
    CACHE
        .get()
        .and_then(|c| c.lock().ok())
        .map_or(0, |g| g.values().map(|i| i.store.read().map_or(0, |s| s.entries.len())).sum())
}

/// Get the cached index for `root` or start building one. `on_progress` fires as it fills.
pub fn get_or_build(root: &str, on_progress: impl Fn() + Send + Sync + 'static) -> Arc<Index> {
    let cache = CACHE.get_or_init(|| Mutex::new(HashMap::new()));
    let mut guard = cache.lock().unwrap();
    if let Some(existing) = guard.get(root) {
        let index = Arc::clone(existing);
        drop(guard);
        let stale = index.checked.lock().unwrap().elapsed() >= CHECK_AFTER;
        if stale && index.done.load(Ordering::SeqCst) && !index.refreshing.swap(true, Ordering::SeqCst) {
            let refresh = Arc::clone(&index);
            std::thread::spawn(move || {
                if changed(&refresh) {
                    let fresh = RwLock::new(Store::default());
                    scan(&refresh.root, &fresh, &|| {});
                    *refresh.store.write().unwrap() = fresh.into_inner().unwrap();
                    on_progress();
                }
                *refresh.checked.lock().unwrap() = Instant::now();
                refresh.refreshing.store(false, Ordering::SeqCst);
            });
        }
        return index;
    }
    if guard.len() >= 3
        && let Some(oldest) = guard.iter().min_by_key(|(_, i)| *i.checked.lock().unwrap()).map(|(k, _)| k.clone()) {
            guard.remove(&oldest);
        }
    let index = Arc::new(Index {
        root: root.to_owned(),
        store: RwLock::new(Store::default()),
        done: AtomicBool::new(false),
        checked: Mutex::new(Instant::now()),
        refreshing: AtomicBool::new(false),
    });
    guard.insert(root.to_owned(), Arc::clone(&index));
    drop(guard);

    let building = Arc::clone(&index);
    std::thread::spawn(move || {
        let t0 = Instant::now();
        scan(&building.root, &building.store, &on_progress);
        *building.checked.lock().unwrap() = Instant::now();
        building.done.store(true, Ordering::SeqCst);
        if std::env::var_os("CFM_DEBUG").is_some() {
            eprintln!("[timing] indexed {} entries under {} in {:?}", building.store.read().unwrap().entries.len(), building.root, t0.elapsed());
        }
        on_progress();
    });
    index
}

/// Whether any indexed folder was changed, added to or removed since it was read. A folder's modified
/// time changes when something inside it is created, deleted or renamed.
fn changed(index: &Index) -> bool {
    let store = index.store.read().unwrap();
    store.dirs.iter().any(|d| fs::symlink_metadata(&*d.abs).ok().map(|m| mtime_of(&m)) != Some(d.mtime))
}

struct Job {
    dir: u32,
    path: PathBuf,
    hidden: bool,
}

struct Queue {
    stack: Vec<Job>,
    /// Folders being read right now; the walk is over when this is 0 and the stack is empty.
    active: usize,
}

/// Walk `root` on several threads, adding what it finds to `store` as it goes.
fn scan(root: &str, store: &RwLock<Store>, on_progress: &(dyn Fn() + Sync)) {
    let Ok(meta) = fs::metadata(root) else { return };
    store.write().unwrap().dirs.push(DirInfo { abs: root.into(), mtime: mtime_of(&meta) });
    let queue = Mutex::new(Queue { stack: vec![Job { dir: 0, path: PathBuf::from(root), hidden: false }], active: 0 });
    let wake = Condvar::new();
    let last_progress = Mutex::new(Instant::now());
    let threads = std::thread::available_parallelism().map_or(4, |n| n.get().min(8));
    std::thread::scope(|scope| {
        for _ in 0..threads {
            scope.spawn(|| worker(&queue, &wake, store, meta.dev(), &last_progress, on_progress));
        }
    });
}

fn worker(
    queue: &Mutex<Queue>,
    wake: &Condvar,
    store: &RwLock<Store>,
    root_dev: u64,
    last_progress: &Mutex<Instant>,
    on_progress: &(dyn Fn() + Sync),
) {
    loop {
        let job = {
            let mut q = queue.lock().unwrap();
            loop {
                if let Some(job) = q.stack.pop() {
                    q.active += 1;
                    break Some(job);
                }
                if q.active == 0 {
                    break None;
                }
                q = wake.wait(q).unwrap();
            }
        };
        let Some(job) = job else {
            wake.notify_all();
            return;
        };

        let mut names = String::new();
        let mut batch: Vec<IEntry> = Vec::new();
        // Folders to read next: (path, hidden, modified time).
        let mut subdirs: Vec<(PathBuf, bool, (i64, i64))> = Vec::new();
        if let Ok(read) = fs::read_dir(&job.path) {
            for e in read.flatten() {
                let Ok(kind) = e.file_type() else { continue };
                let os_name = e.file_name();
                let name = os_name.to_string_lossy();
                if name.len() > u16::MAX as usize {
                    continue;
                }
                let is_dir = kind.is_dir();
                if is_dir && SKIP_DIRS.contains(&name.as_ref()) {
                    continue;
                }
                let hidden = job.hidden || name.starts_with('.');
                if is_dir
                    && let Ok(m) = e.metadata()
                    && m.dev() == root_dev
                {
                    subdirs.push((e.path(), hidden, mtime_of(&m)));
                }
                batch.push(IEntry { dir: job.dir, name_off: names.len() as u32, name_len: name.len() as u16, is_dir, hidden });
                names.push_str(&name);
            }
        }

        let children = {
            let mut s = store.write().unwrap();
            let base = s.names.len() as u32;
            for e in &mut batch {
                e.name_off += base;
            }
            s.names.push_str(&names);
            s.entries.extend(batch);
            subdirs
                .into_iter()
                .map(|(path, hidden, mtime)| {
                    s.dirs.push(DirInfo { abs: path.to_string_lossy().into(), mtime });
                    Job { dir: s.dirs.len() as u32 - 1, path, hidden }
                })
                .collect::<Vec<_>>()
        };
        {
            let mut last = last_progress.lock().unwrap();
            if last.elapsed() > Duration::from_millis(150) {
                *last = Instant::now();
                drop(last);
                on_progress();
            }
        }
        let mut q = queue.lock().unwrap();
        q.stack.extend(children);
        q.active -= 1;
        drop(q);
        wake.notify_all();
    }
}

/// Size and modified time of `path` itself (a link is not followed), `-1` where unknown or not a file.
fn meta_of(path: &Path) -> Meta {
    match fs::symlink_metadata(path) {
        Ok(m) => Meta { size: if m.is_file() { m.len() as i64 } else { -1 }, mtime: m.mtime() },
        Err(_) => Meta { size: -1, mtime: -1 },
    }
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
    let root_len = index.root.trim_end_matches('/').len();
    let guard = index.store.read().unwrap();
    let store: &Store = &guard;
    let all: &[IEntry] = &store.entries;
    let needs_meta = query.needs_meta();

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
                    // The folder part plus the name, for matching against the path below the root.
                    let mut rel = String::new();
                    for (i, e) in part.iter().enumerate() {
                        if i % 4096 == 0 && cancel() {
                            break;
                        }
                        if e.hidden && !show_hidden {
                            continue;
                        }
                        let name = store.name(e);
                        if !query.accepts(name, e.is_dir, -1, -1) {
                            continue;
                        }
                        let dir = &*store.dirs[e.dir as usize].abs;
                        rel.clear();
                        if let Some(d) = dir.get(root_len + 1..).filter(|d| !d.is_empty()) {
                            rel.push_str(d);
                            rel.push('/');
                        }
                        rel.push_str(name);
                        let Some(s) = query.score(name, &rel) else { continue };
                        if needs_meta {
                            let m = meta_of(&store.path(e));
                            if !query.accepts(name, e.is_dir, m.size, m.mtime) {
                                continue;
                            }
                        }
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

    // Equal scores come out in path order: the walk order differs from run to run.
    let order = |i: usize| {
        let e = &all[i];
        (&*store.dirs[e.dir as usize].abs, store.name(e))
    };
    scored.sort_by(|a, b| b.0.cmp(&a.0).then_with(|| order(a.1).cmp(&order(b.1))));
    scored.truncate(MAX_RESULTS);
    if std::env::var_os("CFM_DEBUG").is_some() {
        eprintln!("[timing] search {:?} over {} entries: {:?}", query_text, all.len(), t0.elapsed());
    }
    scored
        .into_iter()
        .map(|(_, i)| {
            let e = &all[i];
            let name = store.name(e).to_owned();
            let path = store.path(e);
            let dir = &*store.dirs[e.dir as usize].abs;
            let entry = Entry {
                hidden: name.starts_with('.'),
                lname: name.to_lowercase(),
                path: path.to_string_lossy().into_owned(),
                name,
                rel: dir.get(root_len + 1..).unwrap_or("").to_owned(),
                is_dir: e.is_dir,
                is_link: false,
                depth: 0,
            };
            (entry, meta_of(&path))
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
    fn size_and_date_filters_still_work_without_indexed_metadata() {
        let sb = Sandbox::new("index-meta");
        sb.touch("big.dat", &"x".repeat(5000));
        sb.touch("small.dat", "x");
        let index = built(&sb.path(""));
        let (hits, metas) = search(&index, "dat size>1k", false, &|| false);
        assert_eq!(names(&hits), ["big.dat"]);
        assert_eq!(metas[0].size, 5000);
        assert_eq!(search(&index, "dat", false, &|| false).0.len(), 2);
    }

    #[test]
    fn a_stale_index_notices_new_files() {
        let sb = Sandbox::new("index-refresh");
        sb.touch("a.txt", "");
        let root = sb.path("");
        let index = built(&root);
        assert!(!changed(&index));
        std::thread::sleep(Duration::from_millis(20));
        sb.touch("later.txt", "");
        assert!(changed(&index));
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
