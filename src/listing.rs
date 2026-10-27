//! Directory listing primitives: entries, fast reads, metadata and recursive-size passes, sorting.

use std::cmp::Ordering;
use std::fs;
use std::os::unix::fs::MetadataExt;
use std::sync::Mutex;
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering as AtomicOrdering};
use std::time::{Duration, Instant};

use walkdir::WalkDir;

/// Size/mtime not read yet.
pub const UNKNOWN: i64 = -1;

/// Size and modification time of an entry, filled in after the names (see `DirModel`).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Meta {
    pub size: i64,
    pub mtime: i64,
}

impl Meta {
    pub const UNKNOWN: Meta = Meta {
        size: UNKNOWN,
        mtime: UNKNOWN,
    };
}

#[derive(Clone, Debug, PartialEq)]
pub struct Entry {
    pub name: String,
    /// Lower-cased name, precomputed so sorting never allocates.
    pub lname: String,
    pub path: String,
    /// Directory part relative to the search root (deep search only).
    pub rel: String,
    pub is_dir: bool,
    pub is_link: bool,
    pub hidden: bool,
    /// Nesting level in tree view (0 for flat listings).
    pub depth: u16,
}

/// Names and file types only (no per-entry stat), so huge folders paint immediately.
/// Uses raw directory reads: names come borrowed from the kernel buffer, one allocation less
/// per entry than `std::fs::read_dir`.
pub fn list_dir(dir: &str) -> Vec<Entry> {
    use rustix::fs::{Dir, FileType, Mode, OFlags};

    let flags = OFlags::RDONLY | OFlags::DIRECTORY | OFlags::CLOEXEC;
    let Ok(fd) = rustix::fs::open(dir, flags, Mode::empty()) else {
        return Vec::new();
    };
    let Ok(rd) = Dir::new(fd) else {
        return Vec::new();
    };
    let base = dir.trim_end_matches('/');
    let mut out = Vec::with_capacity(1024);
    for e in rd.flatten() {
        let raw = e.file_name().to_bytes();
        if raw == b"." || raw == b".." {
            continue;
        }
        let name = String::from_utf8_lossy(raw).into_owned();
        let ft = e.file_type();
        let is_link = ft == FileType::Symlink;
        // Symlinks to directories should navigate like directories; some filesystems report
        // no type at all, so stat those too.
        let is_dir = match ft {
            FileType::Directory => true,
            FileType::Symlink | FileType::Unknown => {
                fs::metadata(format!("{base}/{name}")).is_ok_and(|m| m.is_dir())
            }
            _ => false,
        };
        let mut path = String::with_capacity(base.len() + 1 + name.len());
        path.push_str(base);
        path.push('/');
        path.push_str(&name);
        out.push(Entry {
            path,
            hidden: name.starts_with('.'),
            lname: if name.is_ascii() {
                name.to_ascii_lowercase()
            } else {
                name.to_lowercase()
            },
            name,
            rel: String::new(),
            is_dir,
            is_link,
            depth: 0,
        });
    }
    out
}

/// A folder listing with the given folders expanded in place (tree view). Entries come back in
/// read order with `depth` set; [`tree_order`] arranges them.
pub fn list_tree(root: &str, expanded: &std::collections::HashSet<String>) -> Vec<Entry> {
    fn walk(
        dir: &str,
        depth: u16,
        expanded: &std::collections::HashSet<String>,
        out: &mut Vec<Entry>,
    ) {
        let mut entries = list_dir(dir);
        for e in &mut entries {
            e.depth = depth;
        }
        let nested: Vec<String> = entries
            .iter()
            .filter(|e| e.is_dir && !e.is_link && expanded.contains(&e.path))
            .map(|e| e.path.clone())
            .collect();
        out.append(&mut entries);
        for child in nested {
            walk(&child, depth + 1, expanded, out);
        }
    }
    let mut out = Vec::new();
    walk(root, 0, expanded, &mut out);
    out
}

/// Depth-first display order for a tree listing: siblings sorted by `key`, children right after
/// their parent. `visible` decides which entries take part.
pub fn tree_order(
    all: &[Entry],
    meta: &[Meta],
    root: &str,
    visible: &dyn Fn(usize, &Entry) -> bool,
    key: &str,
    desc: bool,
) -> Vec<u32> {
    use std::collections::HashMap;
    let mut children: HashMap<&str, Vec<u32>> = HashMap::new();
    for (i, e) in all.iter().enumerate() {
        if !visible(i, e) {
            continue;
        }
        let parent = e.path.rsplit_once('/').map_or("", |p| p.0);
        children.entry(parent).or_default().push(i as u32);
    }
    for group in children.values_mut() {
        sort_indices(all, meta, group, key, desc);
    }
    let root = if root.len() > 1 {
        root.trim_end_matches('/')
    } else {
        ""
    };
    let mut out = Vec::with_capacity(all.len());
    // Explicit stack of (children list, next position) so deep trees can't overflow the call stack.
    let mut stack: Vec<(&[u32], usize)> = Vec::new();
    if let Some(top) = children.get(root) {
        stack.push((top, 0));
    }
    while let Some((list, pos)) = stack.pop() {
        if pos >= list.len() {
            continue;
        }
        stack.push((list, pos + 1));
        let idx = list[pos];
        out.push(idx);
        if let Some(kids) = children.get(all[idx as usize].path.as_str()) {
            stack.push((kids, 0));
        }
    }
    out
}

/// `(size, mtime)` of a path without following symlinks. Directories report size `UNKNOWN`.
pub fn stat(path: &str) -> (i64, i64) {
    match fs::symlink_metadata(path) {
        Ok(m) => (if m.is_dir() { UNKNOWN } else { m.len() as i64 }, m.mtime()),
        Err(_) => (UNKNOWN, UNKNOWN),
    }
}

// ---- remote folders -----------------------------------------------------------------------

/// A mount whose lookups go over the network (or to a phone): every stat may wait on a server.
#[derive(Clone, Debug, PartialEq)]
pub struct RemoteMount {
    pub point: String,
    /// What was mounted: `user@host:/srv` (sshfs), `//nas/share` (cifs), `gdrive:` (rclone)...
    pub source: String,
}

fn remote_fstype(fstype: &str) -> bool {
    matches!(
        fstype,
        "fuse.gvfsd-fuse" | "fuse.rclone" | "fuse.sshfs" | "cifs" | "smb3" | "9p"
    ) || fstype.starts_with("nfs")
}

/// mountinfo escapes spaces and a few other bytes as `\040`-style octal.
fn unescape_mount(s: &str) -> String {
    let b = s.as_bytes();
    let mut out = Vec::with_capacity(b.len());
    let mut i = 0;
    while i < b.len() {
        if b[i] == b'\\'
            && i + 3 < b.len()
            && let Ok(v) = u8::from_str_radix(&s[i + 1..i + 4], 8)
        {
            out.push(v);
            i += 4;
        } else {
            out.push(b[i]);
            i += 1;
        }
    }
    String::from_utf8_lossy(&out).into_owned()
}

/// Remote mounts in a `/proc/self/mountinfo` table.
pub fn parse_remote_mounts(mountinfo: &str) -> Vec<RemoteMount> {
    mountinfo
        .lines()
        .filter_map(|line| {
            let (left, right) = line.split_once(" - ")?;
            let point = left.split(' ').nth(4)?;
            let mut rest = right.split(' ');
            let fstype = rest.next()?;
            let source = rest.next().unwrap_or("");
            remote_fstype(fstype).then(|| RemoteMount {
                point: unescape_mount(point),
                source: unescape_mount(source),
            })
        })
        .collect()
}

/// The top of the remote `path` lives on, from `mounts` and the gvfs folder: for gvfs that is the
/// share's own folder (`…/gvfs/sftp:host=nas`), not the gvfs folder holding every share.
pub fn remote_root_in(path: &str, mounts: &[RemoteMount], gvfs: &str) -> Option<String> {
    let under = |root: &str| {
        let root = root.trim_end_matches('/');
        path == root || path.strip_prefix(root).is_some_and(|r| r.starts_with('/'))
    };
    let gvfs = gvfs.trim_end_matches('/');
    if !gvfs.is_empty() && under(gvfs) {
        return Some(
            match path[gvfs.len()..].trim_start_matches('/').split('/').next() {
                Some(entry) if !entry.is_empty() => format!("{gvfs}/{entry}"),
                _ => gvfs.to_owned(),
            },
        );
    }
    mounts
        .iter()
        .filter(|m| under(&m.point))
        .max_by_key(|m| m.point.len())
        .map(|m| m.point.clone())
}

/// The server a remote mount talks to, for "Waiting for <server>…".
pub fn remote_server(root: &str, mounts: &[RemoteMount]) -> String {
    let base = root.rsplit('/').next().unwrap_or(root);
    // gvfs: `smb-share:server=nas,share=media`, `sftp:host=nas,user=bob`.
    if let Some((_, params)) = base.split_once(':')
        && let Some(host) = params.split(',').find_map(|kv| {
            kv.strip_prefix("host=")
                .or_else(|| kv.strip_prefix("server="))
        })
    {
        return host.to_owned();
    }
    let source = mounts
        .iter()
        .find(|m| m.point == root)
        .map_or("", |m| m.source.as_str());
    let host = if let Some(s) = source.strip_prefix("//") {
        s.split('/').next().unwrap_or("")
    } else if let Some((left, _)) = source.split_once(':') {
        left.rsplit('@').next().unwrap_or(left)
    } else {
        ""
    };
    if host.is_empty() {
        base.to_owned()
    } else {
        host.to_owned()
    }
}

type MountCache = Option<(Instant, std::sync::Arc<Vec<RemoteMount>>)>;
static REMOTE_MOUNTS: Mutex<MountCache> = Mutex::new(None);

/// Remote mounts right now; the table is read again at most every 2 s.
fn remote_mounts() -> std::sync::Arc<Vec<RemoteMount>> {
    let mut cache = REMOTE_MOUNTS.lock().unwrap_or_else(|e| e.into_inner());
    if let Some((at, mounts)) = cache.as_ref()
        && at.elapsed() < Duration::from_secs(2)
    {
        return mounts.clone();
    }
    let text = fs::read_to_string("/proc/self/mountinfo").unwrap_or_default();
    let mounts = std::sync::Arc::new(parse_remote_mounts(&text));
    *cache = Some((Instant::now(), mounts.clone()));
    mounts
}

fn gvfs_dir_str() -> String {
    crate::devices::gvfs_dir().to_string_lossy().into_owned()
}

/// Whether reading a folder failed because its mount is gone: the FUSE process behind it (rclone, sshfs,
/// gvfsd-fuse) exited ("Transport endpoint is not connected"), or an NFS handle went stale.
pub fn connection_lost(err: &std::io::Error) -> bool {
    matches!(
        err.raw_os_error(),
        Some(libc::ENOTCONN) | Some(libc::ESTALE)
    )
}

/// Top of the remote mount `path` is on, or `None` for a local path. Never touches `path` itself.
pub fn remote_root(path: &str) -> Option<String> {
    remote_root_in(path, &remote_mounts(), &gvfs_dir_str())
}

pub fn is_remote(path: &str) -> bool {
    remote_root(path).is_some()
}

/// Server name for a remote `path` ("" when local).
pub fn remote_label(path: &str) -> String {
    let mounts = remote_mounts();
    remote_root_in(path, &mounts, &gvfs_dir_str())
        .map_or_else(String::new, |root| remote_server(&root, &mounts))
}

/// Recursive size on disk of everything under `path`; `cancel` aborts early.
pub fn dir_size(path: &str, cancel: &dyn Fn() -> bool) -> i64 {
    let mut total = 0u64;
    for (i, e) in WalkDir::new(path)
        .follow_links(false)
        .into_iter()
        .flatten()
        .enumerate()
    {
        if i % 512 == 0 && cancel() {
            break;
        }
        if e.file_type().is_file()
            && let Ok(m) = e.metadata()
        {
            total += m.len();
        }
    }
    total as i64
}

/// Compute recursive sizes for `dirs` (index, path) on a small worker pool.
/// `flush` receives batches of `(index, size)`, at most roughly every `interval`.
pub fn dir_sizes_parallel(
    dirs: Vec<(usize, String)>,
    interval: Duration,
    cancel: impl Fn() -> bool + Sync,
    flush: impl Fn(Vec<(usize, i64)>) + Sync,
) {
    let next = AtomicUsize::new(0);
    let pending: Mutex<(Vec<(usize, i64)>, Instant)> = Mutex::new((Vec::new(), Instant::now()));
    let stop = AtomicBool::new(false);
    let workers = std::thread::available_parallelism().map_or(2, |n| n.get().clamp(2, 6));

    std::thread::scope(|scope| {
        for _ in 0..workers {
            scope.spawn(|| {
                loop {
                    let i = next.fetch_add(1, AtomicOrdering::SeqCst);
                    if i >= dirs.len() || stop.load(AtomicOrdering::SeqCst) {
                        break;
                    }
                    let (idx, path) = &dirs[i];
                    let size = dir_size(path, &cancel);
                    if cancel() {
                        stop.store(true, AtomicOrdering::SeqCst);
                        break;
                    }
                    let batch = {
                        let mut guard = pending.lock().unwrap();
                        guard.0.push((*idx, size));
                        if guard.1.elapsed() >= interval {
                            guard.1 = Instant::now();
                            Some(std::mem::take(&mut guard.0))
                        } else {
                            None
                        }
                    };
                    if let Some(batch) = batch {
                        flush(batch);
                    }
                }
            });
        }
    });
    if !stop.load(AtomicOrdering::SeqCst) {
        let rest = std::mem::take(&mut pending.lock().unwrap().0);
        if !rest.is_empty() {
            flush(rest);
        }
    }
}

/// Natural comparison of two already lower-cased strings: digit runs compare numerically
/// (`a2` < `a10`). Works on bytes; UTF-8 byte order is good enough for non-ASCII names.
pub fn natural_cmp_ci(a: &str, b: &str) -> Ordering {
    let (a, b) = (a.as_bytes(), b.as_bytes());
    let (mut i, mut j) = (0, 0);
    while i < a.len() && j < b.len() {
        if a[i].is_ascii_digit() && b[j].is_ascii_digit() {
            let (si, sj) = (i, j);
            while i < a.len() && a[i].is_ascii_digit() {
                i += 1;
            }
            while j < b.len() && b[j].is_ascii_digit() {
                j += 1;
            }
            let x = trim_zeros(&a[si..i]);
            let y = trim_zeros(&b[sj..j]);
            let ord = x.len().cmp(&y.len()).then_with(|| x.cmp(y));
            if ord != Ordering::Equal {
                return ord;
            }
        } else {
            if a[i] != b[j] {
                return a[i].cmp(&b[j]);
            }
            i += 1;
            j += 1;
        }
    }
    (a.len() - i).cmp(&(b.len() - j))
}

fn trim_zeros(d: &[u8]) -> &[u8] {
    let n = d.iter().take_while(|&&c| c == b'0').count();
    &d[n..]
}

/// Group (rank for ordering, label for the section header) of an entry under `group_by`
/// (`type`, `date` or `size`). `now` is the current unix time.
pub fn group_of(e: &Entry, m: Meta, group_by: &str, now: i64) -> (u8, &'static str) {
    match group_by {
        "type" => {
            if e.is_dir {
                return (0, "Folders");
            }
            let ext = e.lname.rsplit_once('.').map_or("", |x| x.1);
            match ext {
                "png" | "jpg" | "jpeg" | "gif" | "webp" | "svg" | "bmp" | "ico" | "avif"
                | "tif" | "tiff" => (1, "Images"),
                "mp4" | "mkv" | "webm" | "mov" | "avi" | "m4v" => (2, "Video"),
                "mp3" | "flac" | "ogg" | "opus" | "wav" | "m4a" | "aac" => (3, "Audio"),
                "pdf" | "doc" | "docx" | "odt" | "xls" | "xlsx" | "ppt" | "pptx" | "md" | "txt"
                | "epub" | "csv" => (4, "Documents"),
                "zip" | "tar" | "gz" | "xz" | "zst" | "7z" | "rar" | "bz2" | "iso" => {
                    (5, "Archives")
                }
                "rs" | "py" | "js" | "ts" | "c" | "h" | "cpp" | "hpp" | "go" | "lua" | "sh"
                | "java" | "rb" | "qml" | "html" | "css" | "json" | "toml" | "yml" | "yaml"
                | "nix" => (6, "Code and config"),
                _ => (7, "Other"),
            }
        }
        "date" => {
            if m.mtime < 0 {
                return (9, "Unknown");
            }
            let age = (now - m.mtime).max(0);
            match age {
                0..=86_399 => (0, "Today"),
                86_400..=172_799 => (1, "Yesterday"),
                172_800..=604_799 => (2, "This week"),
                604_800..=2_592_000 => (3, "This month"),
                2_592_001..=31_536_000 => (4, "This year"),
                _ => (5, "Older"),
            }
        }
        "size" => {
            if e.is_dir {
                return (0, "Folders");
            }
            match m.size {
                s if s < 0 => (9, "Unknown"),
                0 => (1, "Empty"),
                1..=10_239 => (2, "Tiny (under 10K)"),
                10_240..=1_048_575 => (3, "Small (under 1M)"),
                1_048_576..=104_857_599 => (4, "Medium (under 100M)"),
                104_857_600..=1_073_741_823 => (5, "Large (under 1G)"),
                _ => (6, "Huge (1G and up)"),
            }
        }
        _ => (0, ""),
    }
}

/// Case-insensitive natural comparison of arbitrary strings.
pub fn natural_cmp(a: &str, b: &str) -> Ordering {
    natural_cmp_ci(&a.to_lowercase(), &b.to_lowercase())
}

fn ext_of(e: &Entry) -> &str {
    e.lname.rsplit_once('.').map_or("", |x| x.1)
}

/// Order `idx` (indices into `all`): directories first, then by `key`
/// (`name`, `size`, `mtime`, `ext`), ties broken by natural name order.
pub fn sort_indices(all: &[Entry], meta: &[Meta], idx: &mut [u32], key: &str, desc: bool) {
    idx.sort_unstable_by(|&ia, &ib| {
        let (a, b) = (&all[ia as usize], &all[ib as usize]);
        let dirs = b.is_dir.cmp(&a.is_dir);
        if dirs != Ordering::Equal {
            return dirs;
        }
        let ord = match key {
            "size" => meta[ia as usize].size.cmp(&meta[ib as usize].size),
            "mtime" => meta[ia as usize].mtime.cmp(&meta[ib as usize].mtime),
            "ext" => ext_of(a).cmp(ext_of(b)),
            _ => Ordering::Equal,
        }
        .then_with(|| natural_cmp_ci(&a.lname, &b.lname))
        .then(ia.cmp(&ib));
        if desc { ord.reverse() } else { ord }
    });
}

/// Sort entries in place (used for small lists such as folder previews).
pub fn sort_entries(entries: &mut Vec<Entry>, key: &str, desc: bool) {
    let mut idx: Vec<u32> = (0..entries.len() as u32).collect();
    let meta = vec![Meta::UNKNOWN; entries.len()];
    sort_indices(entries, &meta, &mut idx, key, desc);
    let mut taken: Vec<Option<Entry>> = std::mem::take(entries).into_iter().map(Some).collect();
    *entries = idx
        .into_iter()
        .filter_map(|i| taken[i as usize].take())
        .collect();
}

#[cfg(test)]
mod tests {

    #[test]
    fn a_dead_mount_is_told_apart_from_other_errors() {
        use super::connection_lost;
        use std::io::Error;
        assert!(connection_lost(&Error::from_raw_os_error(libc::ENOTCONN)));
        assert!(connection_lost(&Error::from_raw_os_error(libc::ESTALE)));
        assert!(!connection_lost(&Error::from_raw_os_error(libc::ENOENT)));
        assert!(!connection_lost(&Error::from_raw_os_error(libc::EACCES)));
        assert!(!connection_lost(&Error::other("no os error")));
    }
    use super::*;

    #[test]
    fn natural_order() {
        assert_eq!(natural_cmp("img2.png", "img10.png"), Ordering::Less);
        assert_eq!(natural_cmp("B", "a"), Ordering::Greater);
        assert_eq!(natural_cmp("a01", "a1"), Ordering::Equal);
    }

    #[test]
    fn tree_order_nests_children_under_parents() {
        let mk = |path: &str, is_dir: bool| {
            let name = path.rsplit('/').next().unwrap();
            Entry {
                name: name.into(),
                lname: name.to_lowercase(),
                path: path.into(),
                rel: String::new(),
                is_dir,
                is_link: false,
                hidden: false,
                depth: path.matches('/').count() as u16 - 2,
            }
        };
        let all = vec![
            mk("/r/b.txt", false),
            mk("/r/a", true),
            mk("/r/a/z", false),
            mk("/r/a/y", false),
            mk("/r/c", true),
        ];
        let meta = vec![Meta::UNKNOWN; all.len()];
        let order = tree_order(&all, &meta, "/r", &|_, _| true, "name", false);
        let paths: Vec<_> = order
            .iter()
            .map(|&i| all[i as usize].path.as_str())
            .collect();
        assert_eq!(paths, ["/r/a", "/r/a/y", "/r/a/z", "/r/c", "/r/b.txt"]);
    }

    #[test]
    fn grouping_by_type_date_and_size() {
        let mk = |name: &str, is_dir| Entry {
            name: name.into(),
            lname: name.to_lowercase(),
            path: name.into(),
            rel: String::new(),
            is_dir,
            is_link: false,
            hidden: false,
            depth: 0,
        };
        let now = 100_000_000;
        let unk = Meta::UNKNOWN;
        assert_eq!(group_of(&mk("d", true), unk, "type", now), (0, "Folders"));
        assert_eq!(group_of(&mk("a.PNG", false), unk, "type", now).1, "Images");
        assert_eq!(
            group_of(&mk("main.rs", false), unk, "type", now).1,
            "Code and config"
        );
        assert_eq!(group_of(&mk("blob", false), unk, "type", now).1, "Other");
        let at = |ago| Meta {
            size: 0,
            mtime: now - ago,
        };
        let f = mk("f", false);
        assert_eq!(group_of(&f, at(60), "date", now).1, "Today");
        assert_eq!(group_of(&f, at(100_000), "date", now).1, "Yesterday");
        assert_eq!(group_of(&f, at(400_000), "date", now).1, "This week");
        assert_eq!(group_of(&f, at(40_000_000), "date", now).1, "Older");
        assert_eq!(group_of(&f, unk, "date", now).1, "Unknown");
        let sz = |size| Meta { size, mtime: 0 };
        assert_eq!(group_of(&f, sz(0), "size", now).1, "Empty");
        assert_eq!(group_of(&f, sz(5_000), "size", now).1, "Tiny (under 10K)");
        assert_eq!(
            group_of(&f, sz(5_000_000), "size", now).1,
            "Medium (under 100M)"
        );
        assert_eq!(
            group_of(&f, sz(2_000_000_000), "size", now).1,
            "Huge (1G and up)"
        );
        assert_eq!(group_of(&f, sz(1), "none", now), (0, ""));
        // Ranks put earlier groups first.
        assert!(group_of(&f, at(60), "date", now).0 < group_of(&f, at(40_000_000), "date", now).0);
    }

    #[test]
    fn dirs_sort_first() {
        let mk = |name: &str, is_dir| Entry {
            name: name.into(),
            lname: name.to_lowercase(),
            path: name.into(),
            rel: String::new(),
            is_dir,
            is_link: false,
            hidden: false,
            depth: 0,
        };
        let mut v = vec![mk("b", false), mk("z", true), mk("a", false)];
        sort_entries(&mut v, "name", false);
        let names: Vec<_> = v.iter().map(|e| e.name.as_str()).collect();
        assert_eq!(names, ["z", "a", "b"]);
    }

    const MOUNTINFO: &str = "\
22 1 259:2 / / rw,relatime shared:1 - btrfs /dev/nvme0n1p2 rw
30 22 0:26 / /home rw,relatime shared:2 - btrfs /dev/nvme0n1p2 rw
40 22 0:40 / /run/user/1000/gvfs rw,nosuid,nodev shared:3 - fuse.gvfsd-fuse gvfsd-fuse rw,user_id=1000
41 30 0:41 / /home/me/nas rw shared:4 - cifs //nas.local/media rw,vers=3.1.1
42 30 0:42 / /home/me/My\\040Drive rw shared:5 - fuse.rclone gdrive: rw
43 30 0:43 / /home/me/box rw shared:6 - fuse.sshfs bob@host.example:/srv rw
44 22 0:44 / /mnt/nfs rw shared:7 - nfs4 fileserver:/export rw
45 22 0:45 / /mnt/usb rw shared:8 - vfat /dev/sdb1 rw
";

    #[test]
    fn remote_mounts_come_from_the_mount_table() {
        let mounts = parse_remote_mounts(MOUNTINFO);
        let points: Vec<&str> = mounts.iter().map(|m| m.point.as_str()).collect();
        assert_eq!(
            points,
            [
                "/run/user/1000/gvfs",
                "/home/me/nas",
                "/home/me/My Drive",
                "/home/me/box",
                "/mnt/nfs"
            ]
        );
        assert!(parse_remote_mounts("garbage\n\n").is_empty());
    }

    #[test]
    fn remote_roots_and_servers() {
        let mounts = parse_remote_mounts(MOUNTINFO);
        let gvfs = "/run/user/1000/gvfs";
        let root = |p: &str| remote_root_in(p, &mounts, gvfs);
        assert_eq!(root("/home/me/docs"), None);
        assert_eq!(root("/mnt/usb/x"), None);
        // A sibling whose name only starts like the mount point is local.
        assert_eq!(root("/home/me/nasty"), None);
        assert_eq!(
            root("/home/me/nas/films/a.mkv").as_deref(),
            Some("/home/me/nas")
        );
        assert_eq!(
            root("/home/me/My Drive").as_deref(),
            Some("/home/me/My Drive")
        );
        assert_eq!(
            root("/run/user/1000/gvfs/sftp:host=nas,user=bob/srv").as_deref(),
            Some("/run/user/1000/gvfs/sftp:host=nas,user=bob")
        );
        assert_eq!(root("/run/user/1000/gvfs").as_deref(), Some(gvfs));
        // The gvfs folder counts even before its mount shows up in the table.
        assert_eq!(
            remote_root_in("/g/smb-share:server=x,share=y/a", &[], "/g").as_deref(),
            Some("/g/smb-share:server=x,share=y")
        );

        let server = |r: &str| remote_server(r, &mounts);
        assert_eq!(server("/run/user/1000/gvfs/sftp:host=nas,user=bob"), "nas");
        assert_eq!(
            server("/run/user/1000/gvfs/smb-share:server=nas2,share=m"),
            "nas2"
        );
        assert_eq!(server("/home/me/nas"), "nas.local");
        assert_eq!(server("/home/me/box"), "host.example");
        assert_eq!(server("/mnt/nfs"), "fileserver");
        assert_eq!(server("/home/me/My Drive"), "gdrive");
    }
}

#[cfg(test)]
mod fs_tests {
    use super::*;
    use crate::testutil::Sandbox;
    use std::collections::HashSet;

    #[test]
    fn list_dir_reports_types_hidden_and_links() {
        let sb = Sandbox::new("listdir");
        sb.touch("sub/", "");
        sb.touch("file.txt", "hello");
        sb.touch(".dot", "");
        std::os::unix::fs::symlink(sb.path("sub"), sb.path("to-sub")).unwrap();
        std::os::unix::fs::symlink(sb.path("file.txt"), sb.path("to-file")).unwrap();
        let entries = list_dir(&sb.root.to_string_lossy());
        let by = |n: &str| {
            entries
                .iter()
                .find(|e| e.name == n)
                .unwrap_or_else(|| panic!("no {n}"))
        };
        assert!(by("sub").is_dir && !by("sub").is_link);
        assert!(!by("file.txt").is_dir);
        assert!(by(".dot").hidden && !by("file.txt").hidden);
        // A link to a folder navigates like a folder; a link to a file does not.
        assert!(by("to-sub").is_link && by("to-sub").is_dir);
        assert!(by("to-file").is_link && !by("to-file").is_dir);
        assert_eq!(by("file.txt").path, sb.path("file.txt"));
        assert_eq!(by("File.TXT".to_lowercase().as_str()).lname, "file.txt");
    }

    #[test]
    fn list_dir_of_a_missing_folder_is_empty() {
        assert!(list_dir("/definitely/not/here").is_empty());
    }

    #[test]
    fn stat_reads_size_and_marks_folders_unknown() {
        let sb = Sandbox::new("stat");
        let f = sb.touch("a.bin", "12345");
        let (size, mtime) = stat(&f.to_string_lossy());
        assert_eq!(size, 5);
        assert!(mtime > 0);
        assert_eq!(stat(&sb.path("")).0, UNKNOWN);
        assert_eq!(stat("/nope"), (UNKNOWN, UNKNOWN));
    }

    #[test]
    fn dir_size_sums_nested_files_and_can_be_cancelled() {
        let sb = Sandbox::new("dirsize");
        sb.touch("d/a", "1234");
        sb.touch("d/sub/b", "123456");
        assert_eq!(dir_size(&sb.path("d"), &|| false), 10);
        // Cancelling stops early with a partial (here: possibly zero) result.
        assert!(dir_size(&sb.path("d"), &|| true) <= 10);
    }

    #[test]
    fn parallel_sizes_report_every_folder() {
        let sb = Sandbox::new("parsizes");
        let mut dirs = Vec::new();
        for i in 0..8 {
            sb.touch(&format!("d{i}/f"), &"x".repeat(i + 1));
            dirs.push((i, sb.path(&format!("d{i}"))));
        }
        let got = std::sync::Mutex::new(Vec::new());
        dir_sizes_parallel(
            dirs,
            Duration::from_millis(1),
            || false,
            |batch| got.lock().unwrap().extend(batch),
        );
        let mut got = got.into_inner().unwrap();
        got.sort();
        assert_eq!(got, (0..8).map(|i| (i, i as i64 + 1)).collect::<Vec<_>>());
    }

    #[test]
    fn tree_listing_expands_only_requested_folders() {
        let sb = Sandbox::new("tree");
        sb.touch("a/inner/deep.txt", "x");
        sb.touch("a/top.txt", "x");
        sb.touch("b/other.txt", "x");
        let expanded: HashSet<String> = [sb.path("a")].into();
        let entries = list_tree(&sb.root.to_string_lossy(), &expanded);
        let names: Vec<(&str, u16)> = entries.iter().map(|e| (e.name.as_str(), e.depth)).collect();
        assert!(names.contains(&("a", 0)) && names.contains(&("b", 0)));
        assert!(names.contains(&("inner", 1)) && names.contains(&("top.txt", 1)));
        // `inner` was not expanded and `b` neither.
        assert!(
            !names
                .iter()
                .any(|(n, _)| *n == "deep.txt" || *n == "other.txt")
        );
    }

    #[test]
    fn sorting_by_size_and_extension() {
        let mk = |name: &str| Entry {
            name: name.into(),
            lname: name.to_lowercase(),
            path: name.into(),
            rel: String::new(),
            is_dir: false,
            is_link: false,
            hidden: false,
            depth: 0,
        };
        let all = vec![mk("b.txt"), mk("a.rs"), mk("c.md")];
        let meta = vec![
            Meta { size: 30, mtime: 1 },
            Meta { size: 10, mtime: 3 },
            Meta { size: 20, mtime: 2 },
        ];
        let order = |key: &str, desc: bool| {
            let mut idx: Vec<u32> = (0..3).collect();
            sort_indices(&all, &meta, &mut idx, key, desc);
            idx.into_iter()
                .map(|i| all[i as usize].name.clone())
                .collect::<Vec<_>>()
        };
        assert_eq!(order("size", false), ["a.rs", "c.md", "b.txt"]);
        assert_eq!(order("size", true), ["b.txt", "c.md", "a.rs"]);
        assert_eq!(order("mtime", false), ["b.txt", "c.md", "a.rs"]);
        assert_eq!(order("ext", false), ["c.md", "a.rs", "b.txt"]);
    }
}
