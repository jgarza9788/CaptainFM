//! Free functions the bridge methods share: QString conversion, ranking, drag and drop parsing, and the worker queues.

use super::*;

pub(super) fn q(s: &str) -> QString {
    QString::from(s)
}

pub(super) fn parse_paths(json: &QString) -> Vec<String> {
    serde_json::from_str(&json.to_string()).unwrap_or_default()
}

/// Order `items` for a picker query. `re:<pattern>` or `/<pattern>/` is a regular expression
/// (smart case); anything else is zoxide-style ordered terms, falling back to fuzzy matching
/// when no title contains all the terms.
pub(super) fn rank_items(items: &[String], query: &str, fuzzy_fallback: bool) -> Vec<usize> {
    let pattern = query
        .strip_prefix("re:")
        .or_else(|| query.strip_prefix('/').and_then(|r| r.strip_suffix('/')));
    if let Some(pattern) = pattern {
        let insensitive = !pattern.chars().any(char::is_uppercase);
        // An unfinished pattern matches nothing rather than everything.
        let Ok(re) = regex::RegexBuilder::new(pattern)
            .case_insensitive(insensitive)
            .build()
        else {
            return Vec::new();
        };
        // Regex results keep the list's own order.
        return items
            .iter()
            .enumerate()
            .filter(|(_, s)| re.is_match(s))
            .map(|(i, _)| i)
            .collect();
    }
    let mut scored: Vec<(i32, usize)> = items
        .iter()
        .enumerate()
        .filter_map(|(i, s)| crate::fuzzy::ordered_terms_score(query, s).map(|sc| (sc, i)))
        .collect();
    if scored.is_empty() && fuzzy_fallback {
        let fuzzy = crate::fuzzy::Query::parse(query);
        scored = items
            .iter()
            .enumerate()
            .filter_map(|(i, s)| fuzzy.score(s, s).map(|sc| (sc, i)))
            .collect();
    }
    scored.sort_by(|a, b| b.0.cmp(&a.0).then(a.1.cmp(&b.1)));
    scored.into_iter().map(|(_, i)| i).collect()
}

pub(super) fn file_url(path: &str) -> String {
    format!("file://{}", ops::percent_encode_path(path))
}

/// The drag payload for `paths`: an RFC 2483 `text/uri-list` for file managers and upload fields, and the
/// plain paths (one per line) for editors and terminals.
pub(super) fn drag_payload(paths: &[String]) -> serde_json::Value {
    let uri_list: String = paths
        .iter()
        .map(|p| format!("{}\r\n", file_url(p)))
        .collect();
    serde_json::json!({"uriList": uri_list, "plain": paths.join("\n")})
}

/// A yes/no lookup QML waits for. On a remote it gets about 150 ms on a helper thread and is then taken as
/// yes, so a slow server never freezes the UI; whoever acts on the answer finds out soon enough.
pub(super) fn bounded_lookup(path: String, look: fn(&str) -> bool) -> bool {
    match listing::remote_root(&path) {
        None => look(&path),
        Some(root) => bounded_lookup_on(&root, path, look, Duration::from_millis(150)),
    }
}

/// Remotes that did not answer in time, and when. For a while after, lookups there do not wait at all:
/// the sidebar asks about every pin in turn, and five pins on a hung server would otherwise freeze it
/// for five waits, each leaving a thread stuck behind.
static SLOW_REMOTES: std::sync::Mutex<Vec<(String, Instant)>> = std::sync::Mutex::new(Vec::new());
const SLOW_FOR: Duration = Duration::from_secs(10);

pub(super) fn bounded_lookup_on(
    root: &str,
    path: String,
    look: fn(&str) -> bool,
    wait: Duration,
) -> bool {
    let recently_slow = |slow: &mut Vec<(String, Instant)>| {
        slow.retain(|(_, at)| at.elapsed() < SLOW_FOR);
        slow.iter().any(|(r, _)| r == root)
    };
    if recently_slow(&mut SLOW_REMOTES.locked()) {
        return true;
    }
    let (tx, rx) = std::sync::mpsc::channel();
    std::thread::spawn(move || {
        let _ = tx.send(look(&path));
    });
    rx.recv_timeout(wait).unwrap_or_else(|_| {
        let mut slow = SLOW_REMOTES.locked();
        if !recently_slow(&mut slow) {
            slow.push((root.to_owned(), Instant::now()));
        }
        true
    })
}

/// Whether moving `paths` into `dest` stays on one filesystem. Remote mounts are compared by their mount
/// point, so hovering a drag over a remote folder never waits on the server.
pub(super) fn same_device(paths: &[String], dest: &str) -> bool {
    use std::os::unix::fs::MetadataExt;
    if paths.is_empty() {
        return false;
    }
    let dest_remote = listing::remote_root(dest);
    if dest_remote.is_some() || paths.iter().any(|p| listing::is_remote(p)) {
        return paths.iter().all(|p| listing::remote_root(p) == dest_remote);
    }
    let Ok(target) = std::fs::metadata(dest) else {
        return false;
    };
    !paths.is_empty()
        && paths.iter().all(|p| {
            // A link is judged by the link itself, since that is what a move relocates.
            std::fs::symlink_metadata(p).is_ok_and(|m| m.dev() == target.dev())
        })
}

/// Local paths in a drop: `file:` URLs (also `file://localhost/...`), else absolute paths or `file:` URLs
/// one per line in `text`. Remote schemes (http, smb, ...) are not local files and are skipped.
pub(super) fn drop_paths(urls: &[String], text: &str) -> Vec<String> {
    let local = |s: &str| -> Option<String> {
        let s = s.trim();
        let rest = s
            .strip_prefix("file://localhost")
            .or_else(|| s.strip_prefix("file://"));
        match rest {
            Some(r) if r.starts_with('/') => Some(percent_decode(r)),
            Some(_) => None,
            None => s.starts_with('/').then(|| s.to_owned()),
        }
    };
    let mut out: Vec<String> = urls.iter().filter_map(|u| local(u)).collect();
    if out.is_empty() {
        out = text
            .lines()
            .filter_map(local)
            .filter(|p| std::path::Path::new(p).exists())
            .collect();
    }
    out.dedup();
    out
}

/// Web addresses in a drop: `http(s)` URLs in `urls`, else one per line in `text` (browsers offer a dragged link or
/// image as text plus their own formats, not as a URI list).
pub(super) fn drop_urls(urls: &[String], text: &str) -> Vec<String> {
    let web = |s: &str| {
        let s = s.trim();
        (s.starts_with("http://") || s.starts_with("https://")).then(|| s.to_owned())
    };
    let mut out: Vec<String> = urls.iter().filter_map(|u| web(u)).collect();
    if out.is_empty() {
        let lines: Vec<&str> = text
            .lines()
            .map(str::trim)
            .filter(|l| !l.is_empty())
            .collect();
        // Only when every line is an address: selected text that merely contains one is not a download.
        if !lines.is_empty() && lines.iter().all(|l| web(l).is_some() && !l.contains(' ')) {
            out = lines.iter().filter_map(|l| web(l)).collect();
        }
    }
    out.dedup();
    out
}

pub(super) fn parse_file_list(text: &str) -> (Vec<String>, bool) {
    let mut cut = false;
    let mut paths = Vec::new();
    for line in text.lines() {
        match line.trim() {
            "cut" => cut = true,
            "copy" | "" => {}
            l if l.starts_with("file://") => {
                let raw = &l["file://".len()..];
                paths.push(percent_decode(raw));
            }
            _ => {}
        }
    }
    (paths, cut)
}

use crate::dbus::percent_decode;

/// A one-slot queue with a single worker: a new item replaces the one waiting, so the worker only ever
/// picks up the latest. `offer` says whether to start the worker; the worker calls `next` until `None`.
pub(super) struct LatestOnly<T> {
    state: std::sync::Mutex<(bool, Option<T>)>,
}

impl<T> LatestOnly<T> {
    pub(super) const fn new() -> Self {
        LatestOnly {
            state: std::sync::Mutex::new((false, None)),
        }
    }

    pub(super) fn offer(&self, item: T) -> bool {
        let mut s = self.state.locked();
        s.1 = Some(item);
        !std::mem::replace(&mut s.0, true)
    }

    pub(super) fn next(&self) -> Option<T> {
        let mut s = self.state.locked();
        let item = s.1.take();
        s.0 = item.is_some();
        item
    }
}

pub(super) type Job = Box<dyn FnOnce() + Send>;

/// Hand a job to the single file-operation worker thread (started on first use).
pub(super) fn op_queue(job: Job) {
    use std::sync::{OnceLock, mpsc};
    static TX: OnceLock<mpsc::Sender<Job>> = OnceLock::new();
    let tx = TX.get_or_init(|| {
        let (tx, rx) = mpsc::channel::<Job>();
        std::thread::spawn(move || {
            for job in rx {
                job();
            }
        });
        tx
    });
    let _ = tx.send(job);
}

/// Start a program that outlives this call, reaping it in the background.
pub(super) fn spawn_detached(program: &str, args: &[&str], cwd: Option<&str>) {
    let mut cmd = Command::new(program);
    cmd.args(args)
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null());
    if let Some(dir) = cwd {
        cmd.current_dir(dir);
    }
    if let Ok(mut child) = cmd.spawn() {
        std::thread::spawn(move || {
            let _ = child.wait();
        });
    }
}
