//! File operations (copy, move, trash, rename, create) with an undo/redo journal.
//!
//! Trash follows the freedesktop spec under `$XDG_DATA_HOME/Trash`, so trashed files can be
//! restored by us (undo) or by any other file manager.

use std::fs;
use std::io;
use std::os::unix::fs::symlink;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::Mutex;

use serde::Serialize;

use crate::state::home;

#[derive(Clone, Debug)]
enum Step {
    Moved { from: String, to: String },
    Copied { from: String, to: String },
    Linked { target: String, to: String },
    Trashed { orig: String, stored: String, info: String },
    CreatedFile { path: String },
    CreatedDir { path: String },
}

#[derive(Clone, Debug)]
struct Action {
    label: String,
    steps: Vec<Step>,
}

#[derive(Default)]
struct Journal {
    undo: Vec<Action>,
    redo: Vec<Action>,
}

static JOURNAL: Mutex<Journal> = Mutex::new(Journal { undo: Vec::new(), redo: Vec::new() });

#[derive(Serialize, Default, Debug)]
pub struct Outcome {
    pub label: String,
    pub ok: usize,
    pub errors: Vec<String>,
    /// Final paths of things created/moved, so the UI can put the cursor on them.
    pub paths: Vec<String>,
}

impl Outcome {
    fn new(label: &str) -> Self {
        Outcome { label: label.to_owned(), ..Default::default() }
    }

    pub fn json(&self) -> String {
        serde_json::to_string(self).unwrap_or_default()
    }
}

fn record(action: Action) {
    if action.steps.is_empty() {
        return;
    }
    let mut j = JOURNAL.lock().unwrap();
    j.undo.push(action);
    j.redo.clear();
    let excess = j.undo.len().saturating_sub(200);
    j.undo.drain(..excess);
}

/// Make symbolic links to `paths` in `dest_dir` (named like the originals, `(copy)` when taken). Undoable.
pub fn link(paths: &[String], dest_dir: &str) -> Outcome {
    let mut out = Outcome::new("Linked");
    let mut steps = Vec::new();
    for src in paths {
        let dst = unique_dest(Path::new(dest_dir), &base_name(src));
        match symlink(src, &dst) {
            Ok(()) => {
                let to = dst.to_string_lossy().into_owned();
                out.paths.push(to.clone());
                steps.push(Step::Linked { target: src.clone(), to });
                out.ok += 1;
            }
            Err(e) => out.errors.push(format!("{}: {e}", base_name(src))),
        }
    }
    record(Action { label: out.label.clone(), steps });
    out
}

/// The file name a downloaded URL gets: its last path segment, decoded; `download` when there is none.
pub fn url_file_name(url: &str) -> String {
    let rest = url.split_once("://").map_or(url, |(_, r)| r);
    let path = rest.split(['?', '#']).next().unwrap_or("");
    let path = path.split_once('/').map_or("", |(_, p)| p);
    let name = crate::dbus::percent_decode(path.rsplit('/').next().unwrap_or(""));
    let name: String = name.chars().filter(|c| !c.is_control() && *c != '/').collect();
    let name = name.trim();
    if name.is_empty() || name == "." || name == ".." { "download".to_owned() } else { name.to_owned() }
}

/// Download http(s) URLs into `dest_dir` with curl, e.g. an image or link dragged out of a browser.
/// Not journaled: undo cannot bring a download back, and removing one is a plain delete.
pub fn download(urls: &[String], dest_dir: &str) -> Outcome {
    let mut out = Outcome::new("Downloaded");
    for url in urls {
        let dst = unique_dest(Path::new(dest_dir), &url_file_name(url));
        let status = std::process::Command::new("curl")
            .args(["-fsSL", "--max-time", "300", "--proto", "=http,https", "-o"])
            .arg(&dst)
            .args(["--", url])
            .stdin(std::process::Stdio::null())
            .status();
        match status {
            Ok(s) if s.success() => {
                out.paths.push(dst.to_string_lossy().into_owned());
                out.ok += 1;
            }
            Ok(_) => {
                let _ = fs::remove_file(&dst);
                out.errors.push(format!("{}: download failed", base_name(&dst.to_string_lossy())));
            }
            Err(e) => out.errors.push(format!("curl: {e}")),
        }
    }
    out
}

// ---- helpers ---------------------------------------------------------------------------

fn base_name(p: &str) -> String {
    Path::new(p).file_name().map_or_else(|| p.to_owned(), |n| n.to_string_lossy().into_owned())
}

/// `dir/name`, or `dir/name (copy)`, `dir/name (copy 2)`... when taken.
fn unique_dest(dir: &Path, name: &str) -> PathBuf {
    let first = dir.join(name);
    if fs::symlink_metadata(&first).is_err() {
        return first;
    }
    let (stem, ext) = match name.rfind('.') {
        Some(i) if i > 0 => (&name[..i], &name[i..]),
        _ => (name, ""),
    };
    for n in 1.. {
        let tag = if n == 1 { " (copy)".to_owned() } else { format!(" (copy {n})") };
        let candidate = dir.join(format!("{stem}{tag}{ext}"));
        if fs::symlink_metadata(&candidate).is_err() {
            return candidate;
        }
    }
    unreachable!()
}

fn copy_recursive(src: &Path, dst: &Path) -> io::Result<()> {
    let meta = fs::symlink_metadata(src)?;
    if meta.file_type().is_symlink() {
        symlink(fs::read_link(src)?, dst)
    } else if meta.is_dir() {
        // Refuse to copy a folder into itself, which would recurse forever.
        if dst.starts_with(src) {
            return Err(io::Error::other("cannot copy a folder into itself"));
        }
        fs::create_dir(dst)?;
        for entry in fs::read_dir(src)? {
            let entry = entry?;
            copy_recursive(&entry.path(), &dst.join(entry.file_name()))?;
        }
        fs::set_permissions(dst, meta.permissions())
    } else {
        fs::copy(src, dst).map(|_| ())
    }
}

fn remove_any(p: &Path) -> io::Result<()> {
    if fs::symlink_metadata(p)?.is_dir() { fs::remove_dir_all(p) } else { fs::remove_file(p) }
}

/// Rename, falling back to copy + delete across filesystems.
fn move_any(src: &Path, dst: &Path) -> io::Result<()> {
    match fs::rename(src, dst) {
        Ok(()) => Ok(()),
        Err(e) if e.raw_os_error() == Some(18) => {
            copy_recursive(src, dst)?;
            remove_any(src)
        }
        Err(e) => Err(e),
    }
}

// ---- trash -----------------------------------------------------------------------------

fn trash_dirs() -> (PathBuf, PathBuf) {
    let base = std::env::var_os("XDG_DATA_HOME")
        .map(PathBuf::from)
        .filter(|p| p.is_absolute())
        .unwrap_or_else(|| home().join(".local/share"))
        .join("Trash");
    (base.join("files"), base.join("info"))
}

pub fn percent_encode_path(p: &str) -> String {
    let mut out = String::new();
    for b in p.bytes() {
        match b {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'/' | b'-' | b'_' | b'.' | b'~' => out.push(b as char),
            _ => out.push_str(&format!("%{b:02X}")),
        }
    }
    out
}

fn deletion_date() -> String {
    Command::new("date")
        .arg("+%Y-%m-%dT%H:%M:%S")
        .output()
        .ok()
        .map(|o| String::from_utf8_lossy(&o.stdout).trim().to_owned())
        .unwrap_or_default()
}

fn trash_one(orig: &str, date: &str) -> io::Result<Step> {
    let (files, infos) = trash_dirs();
    fs::create_dir_all(&files)?;
    fs::create_dir_all(&infos)?;
    let dest = unique_dest(&files, &base_name(orig));
    let name = dest.file_name().unwrap().to_string_lossy().into_owned();
    let info = infos.join(format!("{name}.trashinfo"));
    fs::write(&info, format!("[Trash Info]\nPath={}\nDeletionDate={date}\n", percent_encode_path(orig)))?;
    if let Err(e) = move_any(Path::new(orig), &dest) {
        let _ = fs::remove_file(&info);
        return Err(e);
    }
    Ok(Step::Trashed {
        orig: orig.to_owned(),
        stored: dest.to_string_lossy().into_owned(),
        info: info.to_string_lossy().into_owned(),
    })
}

fn restore_one(orig: &str, stored: &str, info: &str) -> io::Result<()> {
    if fs::symlink_metadata(orig).is_ok() {
        return Err(io::Error::other(format!("{orig} already exists")));
    }
    move_any(Path::new(stored), Path::new(orig))?;
    let _ = fs::remove_file(info);
    Ok(())
}

// ---- public operations -----------------------------------------------------------------

pub fn paste(paths: &[String], dest_dir: &str, cut: bool) -> Outcome {
    let mut out = Outcome::new(if cut { "Moved" } else { "Copied" });
    let mut steps = Vec::new();
    for src in paths {
        let name = base_name(src);
        // Moving into the folder it is already in is a no-op.
        if cut && Path::new(src).parent() == Some(Path::new(dest_dir)) {
            continue;
        }
        let dst = unique_dest(Path::new(dest_dir), &name);
        let result = if cut {
            move_any(Path::new(src), &dst)
        } else {
            copy_recursive(Path::new(src), &dst)
        };
        match result {
            Ok(()) => {
                let (from, to) = (src.clone(), dst.to_string_lossy().into_owned());
                out.paths.push(to.clone());
                steps.push(if cut { Step::Moved { from, to } } else { Step::Copied { from, to } });
                out.ok += 1;
            }
            Err(e) => out.errors.push(format!("{name}: {e}")),
        }
    }
    record(Action { label: out.label.clone(), steps });
    out
}

pub fn trash(paths: &[String]) -> Outcome {
    let mut out = Outcome::new("Trashed");
    let date = deletion_date();
    let mut steps = Vec::new();
    for p in paths {
        match trash_one(p, &date) {
            Ok(step) => {
                steps.push(step);
                out.ok += 1;
            }
            Err(e) => out.errors.push(format!("{}: {e}", base_name(p))),
        }
    }
    record(Action { label: out.label.clone(), steps });
    out
}

/// Permanent deletion. Not undoable.
pub fn delete(paths: &[String]) -> Outcome {
    let mut out = Outcome::new("Deleted");
    for p in paths {
        match remove_any(Path::new(p)) {
            Ok(()) => out.ok += 1,
            Err(e) => out.errors.push(format!("{}: {e}", base_name(p))),
        }
    }
    out
}

pub fn rename(path: &str, new_name: &str) -> Outcome {
    let mut out = Outcome::new("Renamed");
    if new_name.is_empty() || new_name.contains('/') {
        out.errors.push("invalid name".into());
        return out;
    }
    let Some(parent) = Path::new(path).parent() else {
        out.errors.push("no parent folder".into());
        return out;
    };
    let dst = parent.join(new_name);
    if dst == Path::new(path) {
        return out;
    }
    if fs::symlink_metadata(&dst).is_ok() {
        out.errors.push(format!("{new_name} already exists"));
        return out;
    }
    match fs::rename(path, &dst) {
        Ok(()) => {
            let to = dst.to_string_lossy().into_owned();
            out.ok = 1;
            out.paths.push(to.clone());
            record(Action { label: out.label.clone(), steps: vec![Step::Moved { from: path.to_owned(), to }] });
        }
        Err(e) => out.errors.push(e.to_string()),
    }
    out
}

/// Apply many renames at once as one undoable action. Pairs are `(from, to)` full paths.
pub fn rename_many(pairs: &[(String, String)]) -> Outcome {
    let mut out = Outcome::new("Renamed");
    let mut steps = Vec::new();
    // Two-phase through temporary names so swaps and shifts (a->b, b->c) work.
    let mut staged = Vec::new();
    for (i, (from, to)) in pairs.iter().enumerate() {
        if from == to {
            continue;
        }
        let tmp = format!("{from}.cfm-tmp-{i}");
        match fs::rename(from, &tmp) {
            Ok(()) => staged.push((from.clone(), tmp, to.clone())),
            Err(e) => out.errors.push(format!("{}: {e}", base_name(from))),
        }
    }
    for (from, tmp, to) in staged {
        if fs::symlink_metadata(&to).is_ok() {
            let _ = fs::rename(&tmp, &from);
            out.errors.push(format!("{}: target exists", base_name(&to)));
            continue;
        }
        match fs::rename(&tmp, &to) {
            Ok(()) => {
                out.ok += 1;
                out.paths.push(to.clone());
                steps.push(Step::Moved { from, to });
            }
            Err(e) => {
                let _ = fs::rename(&tmp, &from);
                out.errors.push(format!("{}: {e}", base_name(&to)));
            }
        }
    }
    record(Action { label: out.label.clone(), steps });
    out
}

pub fn create(dir: &str, name: &str, is_dir: bool) -> Outcome {
    let mut out = Outcome::new(if is_dir { "Created folder" } else { "Created file" });
    if name.is_empty() || name.contains('/') {
        out.errors.push("invalid name".into());
        return out;
    }
    let path = Path::new(dir).join(name);
    let result = if is_dir {
        fs::create_dir(&path)
    } else {
        fs::OpenOptions::new().write(true).create_new(true).open(&path).map(|_| ())
    };
    match result {
        Ok(()) => {
            let p = path.to_string_lossy().into_owned();
            out.ok = 1;
            out.paths.push(p.clone());
            let step = if is_dir { Step::CreatedDir { path: p } } else { Step::CreatedFile { path: p } };
            record(Action { label: out.label.clone(), steps: vec![step] });
        }
        Err(e) => out.errors.push(e.to_string()),
    }
    out
}

fn undo_step(step: &Step) -> io::Result<Step> {
    match step {
        Step::Moved { from, to } => {
            if fs::symlink_metadata(from).is_ok() {
                return Err(io::Error::other(format!("{from} already exists")));
            }
            move_any(Path::new(to), Path::new(from))?;
            Ok(step.clone())
        }
        Step::Copied { from: _, to } => {
            // Undoing a copy moves the copy to the trash rather than deleting it outright.
            trash_one(to, &deletion_date())?;
            Ok(step.clone())
        }
        Step::Linked { to, .. } => {
            trash_one(to, &deletion_date())?;
            Ok(step.clone())
        }
        Step::Trashed { orig, stored, info } => {
            restore_one(orig, stored, info)?;
            Ok(step.clone())
        }
        Step::CreatedFile { path } | Step::CreatedDir { path } => {
            trash_one(path, &deletion_date())?;
            Ok(step.clone())
        }
    }
}

fn redo_step(step: &Step) -> io::Result<Step> {
    match step {
        Step::Moved { from, to } => {
            move_any(Path::new(from), Path::new(to))?;
            Ok(step.clone())
        }
        Step::Copied { from, to } => {
            copy_recursive(Path::new(from), Path::new(to))?;
            Ok(step.clone())
        }
        Step::Linked { target, to } => {
            symlink(target, to)?;
            Ok(step.clone())
        }
        Step::Trashed { orig, .. } => trash_one(orig, &deletion_date()),
        Step::CreatedFile { path } => {
            fs::OpenOptions::new().write(true).create_new(true).open(path)?;
            Ok(step.clone())
        }
        Step::CreatedDir { path } => {
            fs::create_dir(path)?;
            Ok(step.clone())
        }
    }
}

fn step_paths(step: &Step) -> Vec<String> {
    match step {
        Step::Moved { from, to } => vec![from.clone(), to.clone()],
        Step::Copied { to, .. } | Step::Linked { to, .. } => vec![to.clone()],
        Step::Trashed { orig, .. } => vec![orig.clone()],
        Step::CreatedFile { path } | Step::CreatedDir { path } => vec![path.clone()],
    }
}

/// Undo (`redo == false`) or redo the latest journaled action.
pub fn step_history(redo: bool) -> Outcome {
    let action = {
        let mut j = JOURNAL.lock().unwrap();
        if redo { j.redo.pop() } else { j.undo.pop() }
    };
    let Some(action) = action else {
        let mut out = Outcome::new(if redo { "Nothing to redo" } else { "Nothing to undo" });
        out.errors.push(out.label.clone());
        return out;
    };
    let mut out = Outcome::new(&format!("{} {}", if redo { "Redid" } else { "Undid" }, action.label.to_lowercase()));
    let mut done = Vec::new();
    // Undo walks steps in reverse so dependent steps unwind in order.
    let ordered: Vec<&Step> = if redo { action.steps.iter().collect() } else { action.steps.iter().rev().collect() };
    for step in ordered {
        match if redo { redo_step(step) } else { undo_step(step) } {
            Ok(new_step) => {
                out.ok += 1;
                out.paths.extend(step_paths(step));
                done.push(new_step);
            }
            Err(e) => out.errors.push(e.to_string()),
        }
    }
    if !redo {
        done.reverse();
    }
    let inverse = Action { label: action.label, steps: done };
    let mut j = JOURNAL.lock().unwrap();
    if inverse.steps.is_empty() {
        return out;
    }
    if redo { j.undo.push(inverse) } else { j.redo.push(inverse) }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tmp(name: &str) -> PathBuf {
        let d = std::env::temp_dir().join(format!("cfm-test-{name}-{}", std::process::id()));
        let _ = fs::remove_dir_all(&d);
        fs::create_dir_all(&d).unwrap();
        d
    }

    #[test]
    fn linking_makes_undoable_symlinks() {
        let sb = crate::testutil::Sandbox::new("ops-link");
        sb.touch("src/a.txt", "A");
        sb.touch("dst/", "");
        let out = link(&[sb.path("src/a.txt")], &sb.path("dst"));
        assert_eq!(out.ok, 1, "{:?}", out.errors);
        let l = sb.path("dst/a.txt");
        assert_eq!(fs::read_link(&l).unwrap(), Path::new(&sb.path("src/a.txt")));
        assert_eq!(fs::read_to_string(&l).unwrap(), "A");
        // A second link gets a fresh name.
        link(&[sb.path("src/a.txt")], &sb.path("dst"));
        assert!(Path::new(&sb.path("dst/a (copy).txt")).exists());
    }

    #[test]
    fn download_names_come_from_the_url() {
        assert_eq!(url_file_name("https://example.com/a/b%20c.png?x=1#top"), "b c.png");
        assert_eq!(url_file_name("https://example.com/"), "download");
        assert_eq!(url_file_name("https://example.com"), "download");
        assert_eq!(url_file_name("http://h/dir/.."), "download");
    }

    #[test]
    fn unique_names() {
        let d = tmp("unique");
        fs::write(d.join("a.txt"), "x").unwrap();
        assert_eq!(unique_dest(&d, "a.txt"), d.join("a (copy).txt"));
        fs::write(d.join("a (copy).txt"), "x").unwrap();
        assert_eq!(unique_dest(&d, "a.txt"), d.join("a (copy 2).txt"));
        assert_eq!(unique_dest(&d, "new"), d.join("new"));
    }

    #[test]
    fn copy_dir_and_refuse_self_copy() {
        let d = tmp("copy");
        fs::create_dir_all(d.join("src/sub")).unwrap();
        fs::write(d.join("src/sub/f"), "hi").unwrap();
        copy_recursive(&d.join("src"), &d.join("dst")).unwrap();
        assert_eq!(fs::read_to_string(d.join("dst/sub/f")).unwrap(), "hi");
        assert!(copy_recursive(&d.join("src"), &d.join("src/inner")).is_err());
    }

    #[test]
    fn rename_many_handles_shift() {
        let d = tmp("shift");
        for n in ["a", "b"] {
            fs::write(d.join(n), n).unwrap();
        }
        let p = |n: &str| d.join(n).to_string_lossy().into_owned();
        let out = rename_many(&[(p("a"), p("b")), (p("b"), p("c"))]);
        assert_eq!(out.ok, 2, "{:?}", out.errors);
        assert_eq!(fs::read_to_string(d.join("c")).unwrap(), "b");
    }

    #[test]
    fn percent_encoding() {
        assert_eq!(percent_encode_path("/a b/é"), "/a%20b/%C3%A9");
    }
}

#[cfg(test)]
mod journal_tests {
    use super::*;
    use crate::testutil::Sandbox;

    fn p(sb: &Sandbox, rel: &str) -> String {
        sb.path(rel)
    }

    #[test]
    fn trash_then_undo_then_redo() {
        let sb = Sandbox::new("trash");
        sb.touch("a.txt", "A");
        let out = trash(&[p(&sb, "a.txt")]);
        assert_eq!((out.ok, out.errors.len()), (1, 0), "{:?}", out.errors);
        assert!(!Path::new(&p(&sb, "a.txt")).exists());
        // Freedesktop layout: the file and its .trashinfo record.
        let (files, infos) = trash_dirs();
        assert!(files.join("a.txt").exists());
        let info = fs::read_to_string(infos.join("a.txt.trashinfo")).unwrap();
        assert!(info.contains("[Trash Info]") && info.contains(&format!("Path={}", percent_encode_path(&p(&sb, "a.txt")))));

        let undo = step_history(false);
        assert_eq!(undo.ok, 1, "{:?}", undo.errors);
        assert_eq!(fs::read_to_string(p(&sb, "a.txt")).unwrap(), "A");
        assert!(!infos.join("a.txt.trashinfo").exists());

        let redo = step_history(true);
        assert_eq!(redo.ok, 1, "{:?}", redo.errors);
        assert!(!Path::new(&p(&sb, "a.txt")).exists());
    }

    #[test]
    fn undo_refuses_to_overwrite_a_new_file() {
        let sb = Sandbox::new("undo-clash");
        sb.touch("a.txt", "old");
        trash(&[p(&sb, "a.txt")]);
        sb.touch("a.txt", "new");
        let undo = step_history(false);
        assert_eq!(undo.ok, 0);
        assert!(undo.errors[0].contains("already exists"));
        assert_eq!(fs::read_to_string(p(&sb, "a.txt")).unwrap(), "new");
    }

    #[test]
    fn cut_paste_moves_and_undo_moves_back() {
        let sb = Sandbox::new("cutpaste");
        sb.touch("src/a.txt", "A");
        sb.touch("dst/", "");
        let out = paste(&[p(&sb, "src/a.txt")], &p(&sb, "dst"), true);
        assert_eq!(out.ok, 1);
        assert_eq!(out.paths, [p(&sb, "dst/a.txt")]);
        assert!(!Path::new(&p(&sb, "src/a.txt")).exists());
        assert_eq!(step_history(false).ok, 1);
        assert_eq!(fs::read_to_string(p(&sb, "src/a.txt")).unwrap(), "A");
    }

    #[test]
    fn copy_paste_into_same_folder_makes_a_copy_and_undo_trashes_it() {
        let sb = Sandbox::new("copy");
        sb.touch("a.txt", "A");
        let out = paste(&[p(&sb, "a.txt")], &p(&sb, ""), false);
        assert_eq!(out.paths, [p(&sb, "a (copy).txt")]);
        assert_eq!(fs::read_to_string(p(&sb, "a (copy).txt")).unwrap(), "A");
        step_history(false);
        assert!(!Path::new(&p(&sb, "a (copy).txt")).exists());
        assert!(Path::new(&p(&sb, "a.txt")).exists());
    }

    #[test]
    fn moving_into_the_same_folder_is_a_no_op() {
        let sb = Sandbox::new("noop");
        sb.touch("a.txt", "A");
        let out = paste(&[p(&sb, "a.txt")], &p(&sb, ""), true);
        assert_eq!(out.ok, 0);
        assert!(out.errors.is_empty());
    }

    #[test]
    fn rename_create_and_delete() {
        let sb = Sandbox::new("rename");
        sb.touch("a.txt", "A");
        assert_eq!(rename(&p(&sb, "a.txt"), "b.txt").ok, 1);
        assert!(Path::new(&p(&sb, "b.txt")).exists());
        // Names with slashes, empty names and existing targets are rejected.
        assert!(!rename(&p(&sb, "b.txt"), "x/y").errors.is_empty());
        assert!(!rename(&p(&sb, "b.txt"), "").errors.is_empty());
        sb.touch("c.txt", "");
        assert!(rename(&p(&sb, "b.txt"), "c.txt").errors[0].contains("already exists"));

        assert_eq!(create(&p(&sb, ""), "new.txt", false).ok, 1);
        assert_eq!(create(&p(&sb, ""), "newdir", true).ok, 1);
        assert!(Path::new(&p(&sb, "newdir")).is_dir());
        assert!(!create(&p(&sb, ""), "new.txt", false).errors.is_empty(), "must not clobber");

        let gone = delete(&[p(&sb, "newdir"), p(&sb, "new.txt")]);
        assert_eq!(gone.ok, 2);
        assert!(!Path::new(&p(&sb, "new.txt")).exists());
        // Permanent deletion is not journaled: the next undo reverts something older.
        assert!(delete(&[p(&sb, "missing")]).errors.len() == 1);
    }

    #[test]
    fn batch_rename_is_one_undo_step() {
        let sb = Sandbox::new("batch");
        for n in ["a", "b", "c"] {
            sb.touch(n, n);
        }
        let pairs: Vec<(String, String)> =
            ["a", "b", "c"].iter().map(|n| (p(&sb, n), p(&sb, &format!("{n}.bak")))).collect();
        assert_eq!(rename_many(&pairs).ok, 3);
        let undo = step_history(false);
        assert_eq!(undo.ok, 3, "{:?}", undo.errors);
        assert!(["a", "b", "c"].iter().all(|n| Path::new(&p(&sb, n)).exists()));
    }

    #[test]
    fn empty_history_reports_nothing_to_undo() {
        let _sb = Sandbox::new("empty-history");
        // Drain whatever earlier tests left behind.
        while step_history(false).ok > 0 {}
        let out = step_history(false);
        assert_eq!(out.ok, 0);
        assert_eq!(out.errors, ["Nothing to undo"]);
    }

    #[test]
    fn copying_a_folder_into_itself_fails_cleanly() {
        let sb = Sandbox::new("self-copy");
        sb.touch("d/f", "x");
        let out = paste(&[p(&sb, "d")], &p(&sb, "d"), false);
        assert_eq!(out.ok, 0);
        assert!(!out.errors.is_empty());
    }
}
